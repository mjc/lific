use crate::authz;
use crate::db::{
    DbPool,
    models::{AttachmentActor, CommentActor, Issue, Role, UpdateIssue},
};
use crate::error::LificError;
use crate::realtime::{RealtimeEvent, RealtimeHub};
use crate::resolve_caller::ResolvedIdentity;

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
    let mut issue = {
        let conn = db.read()?;
        let id = crate::db::queries::resolve_identifier(&conn, identifier)?;
        crate::db::queries::get_issue(&conn, id)?
    };
    authz::require_role(db, identity, issue.project_id, Role::Viewer)?;
    retain_visible_relations(db, identity, std::slice::from_mut(&mut issue))?;
    Ok(issue)
}

/// One relation-visibility policy shared by every private issue response.
pub(crate) fn retain_visible_relations(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    issues: &mut [Issue],
) -> Result<(), LificError> {
    let visible = authz::visible_project_ids(db, identity)?;
    let conn = db.read()?;
    crate::db::queries::retain_visible_relations(&conn, issues, visible.as_ref());
    Ok(())
}

/// Commit an authenticated issue edit through the shared domain transaction.
/// The caller supplies its actor scope; this service preserves that transport.
pub(crate) fn commit_issue_update(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
    mut input: UpdateIssue,
) -> Result<Issue, LificError> {
    let user = crate::api::require_user(identity)?;
    input.attachments = AttachmentActor::Authenticated(CommentActor::from(&user));
    let issue = db.transaction(|conn| {
        // Same recheck as the create path, against the issue's project as it
        // stands inside this transaction rather than as it read a moment ago.
        // An update cannot move an issue between projects, so reading it here
        // and writing below are the same project by construction.
        let project_id = crate::db::queries::get_issue(conn, id)?.project_id;
        authz::require_role_conn(conn, identity, project_id, Role::Maintainer)?;
        // LIF-262: `update_issue` re-scans the stored description and
        // reconciles links in the same savepoint as the edit.
        crate::db::queries::update_issue(conn, id, &input)
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
}
