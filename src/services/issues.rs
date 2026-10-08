use crate::authz;
use crate::db::{
    DbPool,
    models::{
        AttachmentActor, CommentActor, CreateIssue, CreateLabel, Issue, Label, ListIssuesQuery,
        Module, Role, UpdateIssue,
    },
};
use crate::error::LificError;
use crate::realtime::{RealtimeEvent, RealtimeHub};
use crate::resolve_caller::ResolvedIdentity;

#[derive(Debug)]
pub(crate) struct IssueCollection {
    pub project: crate::db::models::Project,
    pub modules: Vec<Module>,
    pub labels: Vec<Label>,
    pub issues: Vec<Issue>,
}

pub(crate) fn list_project_collection(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project: &str,
) -> Result<IssueCollection, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let identity = crate::auth::refresh_identity(&tx, identity.as_ref())?;
    crate::api::require_user(&identity)?;
    let id = crate::db::queries::resolve_project_identifier(&tx, project)?;
    let (modules, labels) = issue_create_catalog_conn(&tx, &identity, id)?;
    let project = crate::db::queries::get_project(&tx, id)?;
    let mut issues = Vec::new();
    let mut offset = 0;
    loop {
        let page = crate::db::queries::list_issues_page(
            &tx,
            &ListIssuesQuery {
                project_id: Some(id),
                limit: Some(500),
                offset: Some(offset),
                order_by: Some("sequence".into()),
                ..Default::default()
            },
        )?;
        issues.extend(page.items);
        if !page.has_more {
            break;
        }
        offset += 500;
    }
    // Main adapts the sync index, whose stable initial order is its seq.
    // The description on that surface is the skinny first-line preview.
    issues.sort_by_key(|issue| issue.seq);
    for issue in &mut issues {
        issue.description = crate::db::queries::changes::preview_of(&issue.description);
    }
    tx.commit()?;
    Ok(IssueCollection {
        project,
        modules,
        labels,
        issues,
    })
}

/// Private issue listing shared by REST and native views, retaining the
/// existing project gate, cross-project filtering and relation visibility.
pub(crate) fn list_issues(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    query: &ListIssuesQuery,
) -> Result<Vec<Issue>, LificError> {
    if let Some(project_id) = query.project_id {
        authz::require_role(db, identity, project_id, Role::Viewer)?;
        let mut issues = {
            let conn = db.read()?;
            crate::db::queries::list_issues(&conn, query)?
        };
        retain_visible_relations(db, identity, &mut issues)?;
        return Ok(issues);
    }
    let visible = authz::visible_project_ids(db, identity)?;
    let mut issues = {
        let conn = db.read()?;
        let mut issues = crate::db::queries::list_issues(&conn, query)?;
        crate::db::queries::retain_visible_relations(&conn, &mut issues, visible.as_ref());
        issues
    };
    issues = authz::filter_visible(issues, &visible, |issue| Some(issue.project_id));
    Ok(issues)
}

/// Read the private issue shape through the same Viewer and relation scope
/// rules as REST. Published-project reads use their separate public boundary.
pub(crate) fn get_issue(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    id: i64,
) -> Result<Issue, LificError> {
    let mut issue = {
        let conn = db.read()?;
        crate::db::queries::get_issue(&conn, id)?
    };
    authz::require_role(db, identity, issue.project_id, Role::Viewer)?;
    retain_visible_relations(db, identity, std::slice::from_mut(&mut issue))?;
    Ok(issue)
}

pub(crate) fn resolve_issue(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    identifier: &str,
) -> Result<Issue, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let issue = resolve_issue_conn(&tx, identity, identifier)?;
    tx.commit()?;
    Ok(issue)
}

/// Resolve an issue and its visible relations on the caller's read snapshot.
pub(crate) fn resolve_issue_conn(
    conn: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    identifier: &str,
) -> Result<Issue, LificError> {
    let identity = crate::auth::refresh_identity(conn, identity.as_ref())?;
    let id = crate::db::queries::resolve_identifier(conn, identifier)?;
    let mut issue = crate::db::queries::get_issue(conn, id)?;
    authz::require_role_conn(conn, &identity, issue.project_id, Role::Viewer)?;
    retain_visible_relations_conn(conn, &identity, std::slice::from_mut(&mut issue))?;
    Ok(issue)
}

/// One relation-visibility policy shared by every private issue response.
pub(crate) fn retain_visible_relations(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    issues: &mut [Issue],
) -> Result<(), LificError> {
    let conn = db.read()?;
    retain_visible_relations_conn(&conn, identity, issues)
}

pub(crate) fn retain_visible_relations_conn(
    conn: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    issues: &mut [Issue],
) -> Result<(), LificError> {
    let visible = authz::visible_project_ids_conn(conn, identity)?;
    crate::db::queries::retain_visible_relations(conn, issues, visible.as_ref());
    Ok(())
}

/// Commit a maintainer's issue creation and publish its cursor after commit.
/// The caller supplies transport identity while this boundary assigns the
/// authenticated actor used to link attachment references in the description.
pub(crate) fn commit_issue_create(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    mut input: CreateIssue,
) -> Result<Issue, LificError> {
    authz::require_role(db, identity, input.project_id, Role::Maintainer)?;
    let issue = db.transaction(|conn| {
        // Recheck on the writer so a role change cannot slip between the
        // permission read and the attachment-linking issue transaction.
        let identity = crate::auth::refresh_identity(conn, identity.as_ref())?;
        authz::require_role_conn(conn, &identity, input.project_id, Role::Maintainer)?;
        let user = crate::api::require_user(&identity)?;
        input.attachments = AttachmentActor::Authenticated(CommentActor::from(&user));
        crate::db::queries::create_issue(conn, &input)
    })?;
    realtime.send_with_seq(
        RealtimeEvent::IssueCreated {
            project_id: issue.project_id,
            issue_id: issue.id,
        },
        issue.seq,
    );
    Ok(issue)
}

/// Read issue-create options on the same snapshot as page permission inputs.
pub(crate) fn issue_create_catalog_conn(
    conn: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
) -> Result<(Vec<Module>, Vec<Label>), LificError> {
    authz::require_role_conn(conn, identity, project_id, Role::Viewer)?;
    Ok((
        crate::db::queries::list_modules(conn, project_id)?,
        crate::db::queries::list_labels(conn, project_id)?,
    ))
}

/// Create a project label from the issue composer and publish the project
/// update only after the write succeeds.
pub(crate) fn commit_issue_label_create(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    input: CreateLabel,
) -> Result<Label, LificError> {
    crate::services::structure::commit_create(db, realtime, identity, input.project_id, |conn| {
        crate::db::queries::create_label(conn, &input)
    })
}

/// Commit an authenticated issue edit through the shared domain transaction.
/// The caller supplies its actor scope; this service preserves that transport.
/// Success and conflict snapshots retain only relations visible to the caller
/// on the same writer connection, before commit or publication.
pub(crate) fn commit_issue_update(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
    input: UpdateIssue,
) -> Result<Issue, LificError> {
    commit_issue_update_with(db, realtime, identity, id, |_| input)
}

pub(crate) enum IssueLabelChange<'a> {
    Attach(&'a str),
    Remove(&'a str),
}

/// Apply one label intent to the current writer snapshot, preserving labels
/// and unrelated fields that changed after the picker was rendered.
pub(crate) fn commit_issue_label_change(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
    change: IssueLabelChange<'_>,
) -> Result<Issue, LificError> {
    commit_issue_update_with(db, realtime, identity, id, |current| {
        let mut labels = current.labels.clone();
        match change {
            IssueLabelChange::Attach(name) => {
                if !labels.iter().any(|label| label == name) {
                    labels.push(name.to_owned());
                }
            }
            IssueLabelChange::Remove(name) => labels.retain(|label| label != name),
        }
        UpdateIssue {
            labels: Some(labels),
            ..Default::default()
        }
    })
}

fn commit_issue_update_with(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
    patch: impl FnOnce(&Issue) -> UpdateIssue,
) -> Result<Issue, LificError> {
    let issue = db.transaction(|conn| {
        let identity = crate::auth::refresh_identity(conn, identity.as_ref())?;
        let user = crate::api::require_user(&identity)?;
        // Same recheck as the create path, against the issue's project as it
        // stands inside this transaction rather than as it read a moment ago.
        // An update cannot move an issue between projects, so reading it here
        // and writing below are the same project by construction.
        let current = crate::db::queries::get_issue(conn, id)?;
        authz::require_role_conn(conn, &identity, current.project_id, Role::Maintainer)?;
        let mut input = patch(&current);
        input.attachments = AttachmentActor::Authenticated(CommentActor::from(&user));
        // LIF-262: `update_issue` re-scans the stored description and
        // reconciles links in the same savepoint as the edit.
        match crate::db::queries::update_issue(conn, id, &input) {
            Ok(mut issue) => {
                retain_visible_relations_conn(conn, &identity, std::slice::from_mut(&mut issue))?;
                Ok(issue)
            }
            Err(LificError::UpdateConflict { message, current }) => {
                // Preserve the exact losing-write snapshot and sequence. A new
                // read could observe a later edit and disagree with the conflict.
                let mut issue: Issue = serde_json::from_value(*current).map_err(|error| {
                    LificError::Internal(format!("failed to read conflicting issue: {error}"))
                })?;
                retain_visible_relations_conn(conn, &identity, std::slice::from_mut(&mut issue))?;
                let current = serde_json::to_value(issue).map_err(|error| {
                    LificError::Internal(format!("failed to project conflicting issue: {error}"))
                })?;
                Err(LificError::UpdateConflict {
                    message,
                    current: Box::new(current),
                })
            }
            Err(error) => Err(error),
        }
    })?;
    realtime.send_with_seq(
        RealtimeEvent::IssueUpdated {
            project_id: issue.project_id,
            issue_id: issue.id,
        },
        issue.seq,
    );
    Ok(issue)
}

/// Identifiers and the committed deletion cursor; carries no private issue DTO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IssueDelete {
    pub(crate) issue_id: i64,
    pub(crate) project_id: i64,
    pub(crate) tombstone_seq: i64,
}

/// Tombstone through the shared authorized transaction. The caller supplies
/// its actor scope, and publication follows the successful commit.
pub(crate) fn commit_issue_delete(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
) -> Result<IssueDelete, LificError> {
    let project_id = {
        let conn = db.read()?;
        crate::db::queries::get_issue(&conn, id)?.project_id
    };
    // Preserve REST's missing-issue-before-authorization error ordering and
    // the existing legacy authority, including an anonymous caller.
    authz::require_role(db, identity, project_id, Role::Maintainer)?;
    let deleted = db.transaction(|conn| {
        let project_id = crate::db::queries::get_issue(conn, id)?.project_id;
        authz::require_role_conn(conn, identity, project_id, Role::Maintainer)?;
        crate::db::queries::delete_issue(conn, id)?;
        // Read the tombstone cursor after the write; seq is a global cursor.
        let tombstone_seq = crate::db::queries::issue_seq(conn, id)?;
        Ok(IssueDelete {
            issue_id: id,
            project_id,
            tombstone_seq,
        })
    })?;
    realtime.send_with_seq(
        RealtimeEvent::IssueDeleted {
            project_id: deleted.project_id,
            issue_id: deleted.issue_id,
        },
        deleted.tombstone_seq,
    );
    Ok(deleted)
}

#[cfg(test)]
mod read_tests {
    use super::*;
    use crate::{
        actor::Transport,
        db::{
            models::{CreateIssue, CreateProject, CreateWait, UpdateProject, User},
            queries,
        },
    };

    fn identity(user: &User) -> ResolvedIdentity {
        crate::auth::fresh_identity(user, Transport::Web)
    }

    fn fixture() -> (DbPool, User, User, User, Issue) {
        let (db, admin, _, _, viewer, outsider, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let issue = {
            let conn = db.write().unwrap();
            let issue = queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id,
                    title: "Private subject <script>".into(),
                    description: "Description with ![reference](/api/attachments/unlinked)".into(),
                    source: Some("github".into()),
                    ..Default::default()
                },
            )
            .unwrap();
            queries::waits::add_wait(
                &conn,
                issue.id,
                &CreateWait {
                    user: Some(viewer.username.clone()),
                    note: Some("Private blocker note".into()),
                    ..Default::default()
                },
                Some(admin.id),
            )
            .unwrap();
            queries::get_issue(&conn, issue.id).unwrap()
        };
        (db, admin, viewer, outsider, issue)
    }

    #[test]
    fn native_issue_read_preserves_private_fields_and_scopes_all_relation_directions() {
        let (db, admin, viewer, _, issue) = fixture();
        let (raw, visible_identifiers) = {
            let conn = db.write().unwrap();
            let hidden = queries::create_project(
                &conn,
                &CreateProject {
                    identifier: "HIDE".into(),
                    name: "Hidden relations".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let mut visible_identifiers = Vec::new();
            for (relation, reverse) in [
                ("blocks", false),
                ("blocks", true),
                ("relates_to", false),
                ("duplicate", false),
                ("duplicate", true),
            ] {
                for project_id in [issue.project_id, hidden.id] {
                    let neighbor = queries::create_issue(
                        &conn,
                        &CreateIssue {
                            project_id,
                            title: format!("{relation} reverse={reverse}"),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    if project_id == issue.project_id {
                        visible_identifiers.push(neighbor.identifier);
                    }
                    let (source, target) = if reverse {
                        (neighbor.id, issue.id)
                    } else {
                        (issue.id, neighbor.id)
                    };
                    queries::link_issues(&conn, source, target, relation).unwrap();
                }
            }
            (
                queries::get_issue(&conn, issue.id).unwrap(),
                visible_identifiers,
            )
        };
        for relations in [
            &raw.blocks,
            &raw.blocked_by,
            &raw.relates_to,
            &raw.duplicates,
            &raw.duplicated_by,
        ] {
            assert_eq!(relations.len(), 2);
            assert!(relations.iter().any(|id| id.starts_with("HIDE-")));
        }
        let scoped = get_issue(&db, &Some(identity(&viewer)), issue.id).unwrap();
        for (index, relations) in [
            &scoped.blocks,
            &scoped.blocked_by,
            &scoped.relates_to,
            &scoped.duplicates,
            &scoped.duplicated_by,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                relations.as_slice(),
                std::slice::from_ref(&visible_identifiers[index])
            );
        }
        assert_eq!(scoped.title, raw.title);
        assert_eq!(scoped.description, raw.description);
        assert_eq!(scoped.source, raw.source);
        assert_eq!(scoped.waits, raw.waits);
        assert_eq!(scoped.seq, raw.seq);
        // An authorized private reader keeps its private shape. This read
        // does not grant access to bytes merely referenced in description.
        assert!(scoped.description.contains("/api/attachments/unlinked"));
        let full = get_issue(&db, &Some(identity(&admin)), issue.id).unwrap();
        assert_eq!(
            serde_json::to_value(full).unwrap(),
            serde_json::to_value(raw).unwrap()
        );
    }

    #[test]
    fn native_issue_read_reuses_viewer_membership_and_bot_owner_authority() {
        let (db, admin, viewer, outsider, issue) = fixture();
        let bot = queries::users::create_bot_user(
            &db.write().unwrap(),
            viewer.id,
            "reader-bot",
            "Reader bot",
            None,
        )
        .unwrap();
        assert!(get_issue(&db, &Some(identity(&viewer)), issue.id).is_ok());
        assert!(get_issue(&db, &Some(identity(&bot)), issue.id).is_ok());
        assert!(matches!(
            get_issue(&db, &Some(identity(&outsider)), issue.id),
            Err(LificError::Forbidden(_))
        ));
        queries::members::remove_member(&db.write().unwrap(), issue.project_id, viewer.id).unwrap();
        for caller in [identity(&viewer), identity(&bot)] {
            assert!(matches!(
                get_issue(&db, &Some(caller), issue.id),
                Err(LificError::Forbidden(_))
            ));
        }
        assert!(get_issue(&db, &Some(identity(&admin)), issue.id).is_ok());
    }

    #[test]
    fn native_issue_read_preserves_not_found_legacy_and_distinct_public_boundaries() {
        let (db, _, _, outsider, issue) = fixture();
        assert!(matches!(
            get_issue(&db, &Some(identity(&outsider)), i64::MAX),
            Err(LificError::NotFound(_))
        ));
        {
            let conn = db.write().unwrap();
            assert!(
                queries::public::public_project(&conn, "MEM")
                    .unwrap()
                    .is_none()
            );
            queries::update_project(
                &conn,
                issue.project_id,
                &UpdateProject {
                    is_public: Some(true),
                    ..Default::default()
                },
            )
            .unwrap();
            let published = queries::public::public_project(&conn, "MEM")
                .unwrap()
                .unwrap();
            let public = queries::public::public_issue(&conn, &published, issue.id)
                .unwrap()
                .unwrap();
            assert_eq!(public.source, None);
            assert!(public.waits.is_empty());
        }
        // Publishing does not make the private Viewer-gated read anonymous.
        assert!(matches!(
            get_issue(&db, &None, issue.id),
            Err(LificError::Forbidden(_))
        ));
        assert!(matches!(
            get_issue(&db, &Some(identity(&outsider)), issue.id),
            Err(LificError::Forbidden(_))
        ));
        queries::settings::update(
            &db.write().unwrap(),
            queries::settings::InstanceSettingsPatch {
                authz_enforced: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        let legacy = get_issue(&db, &None, issue.id).unwrap();
        assert_eq!(legacy.source, issue.source);
        assert_eq!(legacy.waits, issue.waits);
        assert_eq!(legacy.description, issue.description);
    }

    #[test]
    fn native_issue_read_propagates_database_faults_as_database_errors() {
        let (db, admin, _, _, issue) = fixture();
        db.write()
            .unwrap()
            .execute(
                "ALTER TABLE issue_relations RENAME TO unavailable_issue_relations",
                [],
            )
            .unwrap();
        assert!(matches!(
            get_issue(&db, &Some(identity(&admin)), issue.id),
            Err(LificError::Database(_))
        ));
    }

    #[test]
    fn native_issue_resolve_preserves_identifier_errors_scope_and_visible_relations() {
        let (db, admin, viewer, outsider, issue) = fixture();
        assert!(matches!(
            resolve_issue(&db, &Some(identity(&outsider)), "MEM-invalid"),
            Err(LificError::BadRequest(_))
        ));
        assert!(matches!(
            resolve_issue(&db, &Some(identity(&outsider)), "MEM-9223372036854775807"),
            Err(LificError::NotFound(_))
        ));
        let (visible, hidden) = {
            let conn = db.write().unwrap();
            let hidden_project = queries::create_project(
                &conn,
                &CreateProject {
                    identifier: "HIDE".into(),
                    name: "Hidden resolve".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let make_issue = |project_id| {
                queries::create_issue(
                    &conn,
                    &CreateIssue {
                        project_id,
                        title: "Resolve neighbor".into(),
                        ..Default::default()
                    },
                )
                .unwrap()
            };
            let visible = make_issue(issue.project_id);
            let hidden = make_issue(hidden_project.id);
            for neighbor in [&visible, &hidden] {
                queries::link_issues(&conn, issue.id, neighbor.id, "relates_to").unwrap();
            }
            (visible, hidden)
        };
        let scoped = resolve_issue(
            &db,
            &Some(identity(&viewer)),
            &issue.identifier.to_lowercase(),
        )
        .unwrap();
        assert_eq!(scoped.id, issue.id);
        assert_eq!(scoped.relates_to, std::slice::from_ref(&visible.identifier));
        assert_eq!(scoped.source, issue.source);
        assert_eq!(scoped.waits, issue.waits);
        assert!(matches!(
            resolve_issue(&db, &Some(identity(&viewer)), &hidden.identifier),
            Err(LificError::Forbidden(_))
        ));
        let full = resolve_issue(&db, &Some(identity(&admin)), &issue.identifier).unwrap();
        assert_eq!(full.relates_to.len(), 2);
        assert!(full.relates_to.contains(&hidden.identifier));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::{ActorCtx, Transport};
    use crate::db::{
        models::{AttachmentActor, AttachmentEntity, AuthUser, CreateIssue, Role},
        queries,
    };
    use crate::realtime::RealtimeEvent;

    fn fixture() -> (DbPool, ResolvedIdentity, Issue) {
        let (db, _, _, maintainer, _, _, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let issue = queries::create_issue(
            &db.write().unwrap(),
            &CreateIssue {
                project_id,
                title: "Native issue edit".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let identity = ResolvedIdentity {
            user: AuthUser {
                id: maintainer.id,
                username: maintainer.username,
                display_name: maintainer.display_name,
                is_admin: false,
            },
            transport: Transport::Web,
        };
        (db, identity, issue)
    }

    #[tokio::test]
    async fn native_issue_edit_commits_actor_links_and_sequence_before_publishing() {
        let (db, identity, before) = fixture();
        let (mine, theirs) = {
            let conn = db.write().unwrap();
            let mine = queries::attachments::create_attachment(
                &conn,
                &"1".repeat(64),
                "mine.png",
                "image/png",
                1,
                Some(identity.user.id),
            )
            .unwrap();
            let other_user: i64 = conn
                .query_row(
                    "SELECT id FROM users WHERE username = 'non_member'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let theirs = queries::attachments::create_attachment(
                &conn,
                &"2".repeat(64),
                "theirs.png",
                "image/png",
                1,
                Some(other_user),
            )
            .unwrap();
            (mine, theirs)
        };
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        let description = format!(
            "![mine](/api/attachments/{}) ![theirs](/api/attachments/{})",
            mine.id, theirs.id
        );
        let saved = crate::actor::scope(
            ActorCtx {
                user_id: Some(identity.user.id),
                transport: Transport::Web,
            },
            async {
                commit_issue_update(
                    &db,
                    &realtime,
                    &Some(identity.clone()),
                    before.id,
                    UpdateIssue {
                        title: Some("Saved natively".into()),
                        description: Some(description.clone()),
                        expected_seq: Some(before.seq),
                        // A caller cannot bypass uploader checks through the DTO.
                        attachments: AttachmentActor::TrustedLocal,
                        ..Default::default()
                    },
                )
            },
        )
        .await
        .unwrap();

        let persisted = queries::get_issue(&db.read().unwrap(), before.id).unwrap();
        assert_eq!(persisted.title, "Saved natively");
        assert_eq!(persisted.description, description);
        assert_eq!(persisted.seq, saved.seq);
        assert!(saved.seq > before.seq);
        let conn = db.read().unwrap();
        let linked =
            queries::attachments::list_for_entity(&conn, AttachmentEntity::Issue, before.id)
                .unwrap();
        assert_eq!(
            linked
                .iter()
                .map(|attachment| attachment.id)
                .collect::<Vec<_>>(),
            vec![mine.id]
        );
        let actor: (Option<i64>, String) = conn.query_row(
            "SELECT actor_user_id, transport FROM audit_log WHERE entity_type = 'issue' AND entity_id = ?1 AND field = 'title' ORDER BY id DESC LIMIT 1",
            [before.id], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(actor, (Some(identity.user.id), "web".into()));
        let event = events.try_recv().unwrap();
        assert_eq!(
            event.event,
            RealtimeEvent::IssueUpdated {
                project_id: saved.project_id,
                issue_id: saved.id
            }
        );
        let envelope: serde_json::Value =
            serde_json::from_str(event.message.to_text().unwrap()).unwrap();
        assert_eq!(envelope["seq"], saved.seq);
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn native_issue_edit_conflict_preserves_row_and_publishes_nothing() {
        let (db, identity, before) = fixture();
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        let result = commit_issue_update(
            &db,
            &realtime,
            &Some(identity),
            before.id,
            UpdateIssue {
                title: Some("Stale edit".into()),
                expected_seq: Some(before.seq - 1),
                ..Default::default()
            },
        );
        let Err(LificError::UpdateConflict { current, .. }) = result else {
            panic!("a stale native edit must return the shared conflict payload");
        };
        assert_eq!(current["seq"], before.seq);
        let after = queries::get_issue(&db.read().unwrap(), before.id).unwrap();
        assert_eq!(after.title, before.title);
        assert_eq!(after.seq, before.seq);
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn native_issue_edit_rechecks_revoked_membership_on_the_writer() {
        let (db, identity, before) = fixture();
        queries::members::upsert_member(
            &db.write().unwrap(),
            before.project_id,
            identity.user.id,
            Role::Viewer,
        )
        .unwrap();
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        let result = commit_issue_update(
            &db,
            &realtime,
            &Some(identity),
            before.id,
            UpdateIssue {
                title: Some("Revoked edit".into()),
                expected_seq: Some(before.seq),
                ..Default::default()
            },
        );
        assert!(matches!(result, Err(LificError::Forbidden(_))));
        let after = queries::get_issue(&db.read().unwrap(), before.id).unwrap();
        assert_eq!(after.title, before.title);
        assert_eq!(after.seq, before.seq);
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn native_issue_edit_attachment_failure_rolls_back_and_publishes_nothing() {
        let (db, identity, before) = fixture();
        let attachment = {
            let conn = db.write().unwrap();
            let attachment = queries::attachments::create_attachment(
                &conn,
                &"3".repeat(64),
                "rollback.png",
                "image/png",
                1,
                Some(identity.user.id),
            )
            .unwrap();
            conn.execute_batch("CREATE TEMP TRIGGER fail_native_attachment_link BEFORE INSERT ON attachment_links BEGIN SELECT RAISE(ABORT, 'native link write failed'); END").unwrap();
            attachment
        };
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        let result = commit_issue_update(
            &db,
            &realtime,
            &Some(identity),
            before.id,
            UpdateIssue {
                title: Some("Must roll back".into()),
                description: Some(format!("![rollback](/api/attachments/{})", attachment.id)),
                expected_seq: Some(before.seq),
                ..Default::default()
            },
        );
        assert!(matches!(result, Err(LificError::Database(_))));
        let conn = db.read().unwrap();
        let after = queries::get_issue(&conn, before.id).unwrap();
        assert_eq!(after.title, before.title);
        assert_eq!(after.description, before.description);
        assert_eq!(after.seq, before.seq);
        assert!(
            queries::attachments::list_for_entity(&conn, AttachmentEntity::Issue, before.id)
                .unwrap()
                .is_empty()
        );
        assert!(events.try_recv().is_err());
    }

    fn relations(db: &DbPool, issue: &Issue) -> Issue {
        let conn = db.write().unwrap();
        let hidden = queries::create_project(
            &conn,
            &crate::db::models::CreateProject {
                identifier: "HIDE".into(),
                name: "Hidden relations".into(),
                ..Default::default()
            },
        )
        .unwrap();
        for (relation, reverse) in [
            ("blocks", false),
            ("blocks", true),
            ("relates_to", false),
            ("duplicate", false),
            ("duplicate", true),
        ] {
            for project_id in [issue.project_id, hidden.id] {
                let neighbor = queries::create_issue(
                    &conn,
                    &CreateIssue {
                        project_id,
                        title: format!("{relation}/{reverse}"),
                        ..Default::default()
                    },
                )
                .unwrap();
                let (source, target) = if reverse {
                    (neighbor.id, issue.id)
                } else {
                    (issue.id, neighbor.id)
                };
                queries::link_issues(&conn, source, target, relation).unwrap();
            }
        }
        queries::get_issue(&conn, issue.id).unwrap()
    }

    fn assert_scoped(issue: &Issue) {
        for relations in [
            &issue.blocks,
            &issue.blocked_by,
            &issue.relates_to,
            &issue.duplicates,
            &issue.duplicated_by,
        ] {
            assert_eq!(relations.len(), 1);
            assert!(!relations[0].starts_with("HIDE-"));
        }
    }

    #[test]
    fn native_issue_write_success_scopes_all_private_relation_directions() {
        let (db, identity, before) = fixture();
        let before = relations(&db, &before);
        let saved = commit_issue_update(
            &db,
            &RealtimeHub::new(),
            &Some(identity),
            before.id,
            UpdateIssue {
                title: Some("Scoped success".into()),
                expected_seq: Some(before.seq),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(saved.title, "Scoped success");
        assert_scoped(&saved);
        let raw = queries::get_issue(&db.read().unwrap(), before.id).unwrap();
        assert_eq!(
            raw.blocks.len(),
            2,
            "projection must not delete stored relations"
        );
        let admin = queries::users::get_user_by_username(&db.read().unwrap(), "admin").unwrap();
        let admin = crate::auth::fresh_identity(&admin, Transport::Web);
        let saved = commit_issue_update(
            &db,
            &RealtimeHub::new(),
            &Some(admin),
            before.id,
            UpdateIssue {
                title: Some("Admin success".into()),
                expected_seq: Some(saved.seq),
                ..Default::default()
            },
        )
        .unwrap();
        for relations in [
            &saved.blocks,
            &saved.blocked_by,
            &saved.relates_to,
            &saved.duplicates,
            &saved.duplicated_by,
        ] {
            assert_eq!(relations.len(), 2);
        }
    }

    #[test]
    fn native_issue_write_conflict_scopes_captured_snapshot_without_mutation_or_publication() {
        let (db, identity, before) = fixture();
        let before = relations(&db, &before);
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        let result = commit_issue_update(
            &db,
            &realtime,
            &Some(identity),
            before.id,
            UpdateIssue {
                title: Some("Losing draft".into()),
                expected_seq: Some(before.seq - 1),
                ..Default::default()
            },
        );
        let Err(LificError::UpdateConflict { message, current }) = result else {
            panic!("expected captured conflict")
        };
        assert!(message.contains(&format!("current seq {}", before.seq)));
        let current: Issue = serde_json::from_value(*current).unwrap();
        assert_eq!(current.seq, before.seq);
        assert_eq!(current.title, before.title);
        assert_scoped(&current);
        assert_eq!(
            queries::get_issue(&db.read().unwrap(), before.id)
                .unwrap()
                .seq,
            before.seq
        );
        assert!(events.try_recv().is_err());
        let admin = queries::users::get_user_by_username(&db.read().unwrap(), "admin").unwrap();
        let admin = crate::auth::fresh_identity(&admin, Transport::Web);
        let Err(LificError::UpdateConflict { current, .. }) = commit_issue_update(
            &db,
            &realtime,
            &Some(admin),
            before.id,
            UpdateIssue {
                expected_seq: Some(before.seq - 1),
                title: Some("Admin losing draft".into()),
                ..Default::default()
            },
        ) else {
            panic!("expected admin conflict")
        };
        let current: Issue = serde_json::from_value(*current).unwrap();
        for relations in [
            &current.blocks,
            &current.blocked_by,
            &current.relates_to,
            &current.duplicates,
            &current.duplicated_by,
        ] {
            assert_eq!(relations.len(), 2);
        }
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn label_intents_preserve_current_labels_and_unrelated_issue_fields() {
        let (db, identity, before) = fixture();
        for name in ["first", "concurrent", "second"] {
            queries::create_label(
                &db.write().unwrap(),
                &CreateLabel {
                    project_id: before.project_id,
                    name: name.into(),
                    color: "#123abc".into(),
                },
            )
            .unwrap();
        }
        let concurrent = queries::update_issue(
            &db.write().unwrap(),
            before.id,
            &UpdateIssue {
                title: Some("Concurrent confirmed title".into()),
                description: Some("Concurrent confirmed body".into()),
                labels: Some(vec!["first".into(), "concurrent".into()]),
                ..Default::default()
            },
        )
        .unwrap();
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        let attached = commit_issue_label_change(
            &db,
            &realtime,
            &Some(identity.clone()),
            before.id,
            IssueLabelChange::Attach("second"),
        )
        .unwrap();
        let mut attached_labels = attached.labels.clone();
        attached_labels.sort();
        assert_eq!(attached_labels, ["concurrent", "first", "second"]);
        assert_eq!(attached.title, concurrent.title);
        assert_eq!(attached.description, concurrent.description);
        assert!(attached.seq > concurrent.seq);
        let event = events.try_recv().unwrap();
        assert_eq!(
            event.event,
            RealtimeEvent::IssueUpdated {
                project_id: attached.project_id,
                issue_id: attached.id,
            }
        );
        let envelope: serde_json::Value =
            serde_json::from_str(event.message.to_text().unwrap()).unwrap();
        assert_eq!(envelope["seq"], attached.seq);
        assert!(events.try_recv().is_err());

        let removed = commit_issue_label_change(
            &db,
            &realtime,
            &Some(identity.clone()),
            before.id,
            IssueLabelChange::Remove("first"),
        )
        .unwrap();
        let mut removed_labels = removed.labels.clone();
        removed_labels.sort();
        assert_eq!(removed_labels, ["concurrent", "second"]);
        assert_eq!(removed.title, concurrent.title);
        assert_eq!(removed.description, concurrent.description);
        let repeated = commit_issue_label_change(
            &db,
            &realtime,
            &Some(identity),
            before.id,
            IssueLabelChange::Attach("second"),
        )
        .unwrap();
        let mut repeated_labels = repeated.labels;
        repeated_labels.sort();
        assert_eq!(repeated_labels, ["concurrent", "second"]);
    }

    #[test]
    fn label_intents_recheck_current_role_and_do_not_publish_failed_writes() {
        let (db, identity, before) = fixture();
        for name in ["denied", "failed"] {
            queries::create_label(
                &db.write().unwrap(),
                &CreateLabel {
                    project_id: before.project_id,
                    name: name.into(),
                    color: "#123abc".into(),
                },
            )
            .unwrap();
        }
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        queries::members::upsert_member(
            &db.write().unwrap(),
            before.project_id,
            identity.user.id,
            Role::Viewer,
        )
        .unwrap();
        assert!(matches!(
            commit_issue_label_change(
                &db,
                &realtime,
                &Some(identity.clone()),
                before.id,
                IssueLabelChange::Attach("denied")
            ),
            Err(LificError::Forbidden(_))
        ));
        assert!(events.try_recv().is_err());
        assert_eq!(
            queries::get_issue(&db.read().unwrap(), before.id)
                .unwrap()
                .seq,
            before.seq
        );

        queries::members::upsert_member(
            &db.write().unwrap(),
            before.project_id,
            identity.user.id,
            Role::Maintainer,
        )
        .unwrap();
        db.write().unwrap().execute_batch(
            "CREATE TRIGGER fail_label_intent BEFORE INSERT ON issue_labels BEGIN SELECT RAISE(ABORT, 'forced label failure'); END;",
        ).unwrap();
        assert!(matches!(
            commit_issue_label_change(
                &db,
                &realtime,
                &Some(identity),
                before.id,
                IssueLabelChange::Attach("failed")
            ),
            Err(LificError::Database(_))
        ));
        let persisted = queries::get_issue(&db.read().unwrap(), before.id).unwrap();
        assert_eq!(persisted.labels, before.labels);
        assert_eq!(persisted.seq, before.seq);
        assert!(events.try_recv().is_err());
    }
}

#[cfg(test)]
mod delete_tests {
    use super::*;
    use crate::{
        actor::{ActorCtx, Transport},
        db::{
            models::{AttachmentEntity, CreateIssue},
            queries,
        },
    };

    fn fixture() -> (DbPool, ResolvedIdentity, Issue) {
        let (db, _, _, maintainer, _, _, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let issue = queries::create_issue(
            &db.write().unwrap(),
            &CreateIssue {
                project_id,
                title: "Deferred native deletion".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let identity = crate::auth::fresh_identity(&maintainer, Transport::Web);
        (db, identity, issue)
    }

    #[tokio::test]
    async fn native_issue_delete_commits_web_audit_tombstone_and_one_event() {
        let (db, identity, before) = fixture();
        let attachment = {
            let conn = db.write().unwrap();
            let attachment = queries::attachments::create_attachment(
                &conn,
                &"4".repeat(64),
                "kept.png",
                "image/png",
                1,
                Some(identity.user.id),
            )
            .unwrap();
            queries::attachments::link_attachment(
                &conn,
                attachment.id,
                AttachmentEntity::Issue,
                before.id,
            )
            .unwrap();
            // The cursor is global: unrelated mutations can leave gaps.
            queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id: before.project_id,
                    title: "Cursor gap".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            attachment
        };
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        let deleted = crate::actor::scope(
            ActorCtx {
                user_id: Some(identity.user.id),
                transport: Transport::Web,
            },
            async { commit_issue_delete(&db, &realtime, &Some(identity.clone()), before.id) },
        )
        .await
        .unwrap();
        let conn = db.read().unwrap();
        assert!(matches!(
            queries::get_issue(&conn, before.id),
            Err(LificError::NotFound(_))
        ));
        let seq = queries::issue_seq(&conn, before.id).unwrap();
        assert!(seq > before.seq);
        assert_eq!(deleted.issue_id, before.id);
        assert_eq!(deleted.project_id, before.project_id);
        assert_eq!(deleted.tombstone_seq, seq);
        let audits: Vec<(Option<i64>, String)> = conn.prepare("SELECT actor_user_id, transport FROM audit_log WHERE entity_type = 'issue' AND entity_id = ?1 AND action = 'delete'").unwrap()
            .query_map([before.id], |row| Ok((row.get(0)?, row.get(1)?))).unwrap().collect::<Result<_, _>>().unwrap();
        assert_eq!(audits, vec![(Some(identity.user.id), "web".into())]);
        assert_eq!(
            queries::attachments::list_for_entity(&conn, AttachmentEntity::Issue, before.id)
                .unwrap()[0]
                .id,
            attachment.id
        );
        assert!(queries::attachments::get_attachment(&conn, attachment.id).is_ok());
        drop(conn);
        let event = events.try_recv().unwrap();
        assert_eq!(
            event.event,
            RealtimeEvent::IssueDeleted {
                project_id: before.project_id,
                issue_id: before.id
            }
        );
        let envelope: serde_json::Value =
            serde_json::from_str(event.message.to_text().unwrap()).unwrap();
        assert_eq!(envelope["seq"], seq);
        assert!(matches!(
            commit_issue_delete(&db, &realtime, &Some(identity), before.id),
            Err(LificError::NotFound(_))
        ));
        assert_eq!(
            queries::issue_seq(&db.read().unwrap(), before.id).unwrap(),
            seq
        );
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn native_issue_delete_rechecks_demotion_and_removal() {
        for removed in [false, true] {
            let (db, identity, before) = fixture();
            if removed {
                queries::members::remove_member(
                    &db.write().unwrap(),
                    before.project_id,
                    identity.user.id,
                )
                .unwrap();
            } else {
                queries::members::upsert_member(
                    &db.write().unwrap(),
                    before.project_id,
                    identity.user.id,
                    Role::Viewer,
                )
                .unwrap();
            }
            let realtime = RealtimeHub::new();
            let mut events = realtime.subscribe();
            assert!(matches!(
                commit_issue_delete(&db, &realtime, &Some(identity), before.id),
                Err(LificError::Forbidden(_))
            ));
            assert_eq!(
                queries::get_issue(&db.read().unwrap(), before.id)
                    .unwrap()
                    .seq,
                before.seq
            );
            assert!(events.try_recv().is_err());
        }
    }

    #[test]
    fn native_issue_delete_preserves_error_order_and_legacy_authority() {
        let (db, _, before) = fixture();
        let realtime = RealtimeHub::new();
        assert!(matches!(
            commit_issue_delete(&db, &realtime, &None, i64::MAX),
            Err(LificError::NotFound(_))
        ));
        assert!(matches!(
            commit_issue_delete(&db, &realtime, &None, before.id),
            Err(LificError::Forbidden(_))
        ));
        queries::settings::update(
            &db.write().unwrap(),
            queries::settings::InstanceSettingsPatch {
                authz_enforced: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(commit_issue_delete(&db, &realtime, &None, before.id).is_ok());
    }

    #[test]
    fn native_issue_delete_database_failure_leaves_live_row_and_no_event() {
        let (db, identity, before) = fixture();
        db.write().unwrap().execute_batch("CREATE TEMP TRIGGER reject_native_delete BEFORE UPDATE OF deleted_at ON issues WHEN NEW.deleted_at IS NOT NULL BEGIN SELECT RAISE(ABORT, 'delete blocked'); END").unwrap();
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        assert!(matches!(
            commit_issue_delete(&db, &realtime, &Some(identity), before.id),
            Err(LificError::Database(_))
        ));
        assert_eq!(
            queries::get_issue(&db.read().unwrap(), before.id)
                .unwrap()
                .seq,
            before.seq
        );
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn native_issue_delete_failed_cursor_read_rolls_back_tombstone_and_audit() {
        let (db, identity, before) = fixture();
        // The delete query succeeds, but its post-write cursor read cannot.
        // This probes transaction rollback beyond SQLite statement atomicity.
        db.write().unwrap().execute_batch("CREATE TEMP TRIGGER remove_native_tombstone AFTER UPDATE OF deleted_at ON issues WHEN OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL BEGIN DELETE FROM issues WHERE id = NEW.id; END").unwrap();
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        assert!(matches!(
            commit_issue_delete(&db, &realtime, &Some(identity), before.id),
            Err(LificError::NotFound(_))
        ));
        let conn = db.read().unwrap();
        assert_eq!(
            queries::get_issue(&conn, before.id).unwrap().seq,
            before.seq
        );
        let deletes: i64 = conn.query_row("SELECT count(*) FROM audit_log WHERE entity_type = 'issue' AND entity_id = ?1 AND action = 'delete'", [before.id], |row| row.get(0)).unwrap();
        assert_eq!(deletes, 0);
        assert!(events.try_recv().is_err());
    }
}

#[cfg(test)]
mod stale_identity_tests {
    use super::*;

    #[tokio::test]
    async fn issue_writer_rechecks_stale_admin_and_publishes_nothing_after_demotion() {
        let (db, admin, _, _, _, _, project) = crate::api::test_helpers::setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(
            &admin,
            crate::actor::Transport::Web,
        ));
        let issue = {
            let conn = db.write().unwrap();
            let issue = crate::db::queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id: project,
                    title: "Keep the confirmed title".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            conn.execute("UPDATE users SET is_admin = 0 WHERE id = ?1", [admin.id])
                .unwrap();
            crate::db::queries::members::upsert_member(&conn, project, admin.id, Role::Viewer)
                .unwrap();
            issue
        };
        let hub = RealtimeHub::new();
        let mut events = hub.subscribe();
        let result = commit_issue_update(
            &db,
            &hub,
            &identity,
            issue.id,
            UpdateIssue {
                title: Some("Stale privilege must not write".into()),
                expected_seq: Some(issue.seq),
                ..Default::default()
            },
        );
        assert!(
            matches!(result, Err(LificError::Forbidden(_))),
            "stale administrator unexpectedly wrote: {result:?}"
        );
        assert_eq!(
            crate::db::queries::get_issue(&db.read().unwrap(), issue.id)
                .unwrap()
                .title,
            issue.title
        );
        assert!(events.try_recv().is_err());
    }
}
