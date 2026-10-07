//! Authorized read models for the native project Files manager.

use crate::{
    db::{
        DbPool,
        models::{
            AuthUser, LinkedEntity, PendingOrphanList, Project, ProjectAttachmentPage,
            ProjectAttachmentQuery, Role,
        },
        queries,
    },
    error::LificError,
    realtime::{RealtimeEvent, RealtimeHub},
    resolve_caller::ResolvedIdentity,
    storage::AttachmentStore,
};

#[derive(Debug)]
pub(crate) struct Snapshot {
    pub(crate) user: AuthUser,
    pub(crate) projects: Vec<Project>,
    pub(crate) project: Project,
    pub(crate) authority: crate::services::project_authority::Snapshot,
    pub(crate) page: ProjectAttachmentPage,
    pub(crate) orphans: PendingOrphanList,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct DuplicateFile {
    pub(crate) attachment_id: i64,
    pub(crate) filename: String,
    pub(crate) entities: Vec<LinkedEntity>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct WhereUsed {
    pub(crate) entities: Vec<LinkedEntity>,
    pub(crate) duplicates: Vec<DuplicateFile>,
}

pub(crate) fn download_response(
    db: &DbPool,
    store: &AttachmentStore,
    identity: &Option<ResolvedIdentity>,
    attachment_id: i64,
    headers: &axum::http::HeaderMap,
) -> Result<axum::response::Response, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let current = crate::auth::refresh_identity(&tx, identity.as_ref())?;
    let attachment = queries::attachments::get_attachment(&tx, attachment_id)?;
    authorize_read_conn(&tx, &current, &attachment)?;
    tx.commit()?;
    crate::api::attachments::attachment_bytes_response(&attachment, store, headers)
}

pub(crate) fn delete(
    db: &DbPool,
    hub: &RealtimeHub,
    store: &AttachmentStore,
    identity: &Option<ResolvedIdentity>,
    attachment_id: i64,
) -> Result<(), LificError> {
    let events = store
        .try_with_lock(|store| {
            let (sha256, events) = db.transaction(|conn| {
                let current = crate::auth::refresh_identity(conn, identity.as_ref())?;
                let user = crate::api::require_user(&current)?;
                let attachment = queries::attachments::get_attachment(conn, attachment_id)?;
                authorize_delete_conn(conn, &current, &user, &attachment)?;
                let events = linked_events_conn(conn, attachment_id)?;
                queries::attachments::delete_attachment(conn, attachment_id)?;
                Ok((attachment.sha256, events))
            })?;
            let remaining = {
                let conn = db.read()?;
                queries::attachments::count_rows_for_sha(&conn, &sha256)?
            };
            if remaining == 0 {
                store.delete_unlocked(&sha256)?;
            }
            Ok(events)
        })?
        .ok_or_else(AttachmentStore::busy_error)?;
    for event in events {
        hub.send(event);
    }
    Ok(())
}

fn authorize_delete_conn(
    conn: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    user: &AuthUser,
    attachment: &crate::db::models::Attachment,
) -> Result<(), LificError> {
    if user.is_admin || Some(user.id) == attachment.uploader_id {
        return Ok(());
    }
    for project_id in owning_project_ids_conn(conn, attachment.id)? {
        if crate::authz::require_role_conn(conn, identity, project_id, Role::Maintainer).is_ok() {
            return Ok(());
        }
    }
    Err(LificError::Forbidden(
        "only the uploader, a project maintainer, or an admin can delete this attachment".into(),
    ))
}

fn linked_events_conn(
    conn: &rusqlite::Connection,
    attachment_id: i64,
) -> Result<Vec<RealtimeEvent>, LificError> {
    let links = queries::attachments::links_for_attachment(conn, attachment_id)?;
    let mut events = Vec::new();
    for (raw_entity, entity_id) in links {
        let Ok(entity) = raw_entity.parse::<crate::db::models::AttachmentEntity>() else {
            continue;
        };
        let event = crate::api::attachments::linked_entity_event(conn, entity, entity_id)?;
        if let Some(event) = event
            && !events.contains(&event)
        {
            events.push(event);
        }
    }
    Ok(events)
}

pub(crate) fn snapshot(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    identifier: &str,
    query: &ProjectAttachmentQuery,
) -> Result<Snapshot, LificError> {
    let supplied = crate::api::require_user(identity)?;
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let current = crate::auth::refresh_identity(&tx, identity.as_ref())?;
    let user = crate::api::require_user(&current)?;
    if supplied.id != user.id {
        return Err(LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        ));
    }
    let visible = super::projects::sidebar_visibility(&tx, user.id)?;
    let projects = queries::list_projects_for_user(&tx, user.id)?;
    let projects = crate::authz::filter_visible(projects, &visible, |project| Some(project.id));
    let project = projects
        .iter()
        .find(|project| project.identifier.eq_ignore_ascii_case(identifier))
        .cloned()
        .ok_or_else(|| LificError::NotFound(format!("Project {identifier} not found")))?;
    let authority = crate::services::project_authority::load_conn(&tx, &current, project.id)?;
    let page = queries::attachments::list_project_attachments(&tx, project.id, query)?;
    let orphans = orphan_list_conn(&tx, project.id)?;
    let user = crate::auth::fresh_auth_user(&crate::auth::fresh_caller(&tx, user.id)?);
    tx.commit()?;
    Ok(Snapshot {
        user,
        projects,
        project,
        authority,
        page,
        orphans,
    })
}

fn orphan_list_conn(
    conn: &rusqlite::Connection,
    project_id: i64,
) -> Result<PendingOrphanList, LificError> {
    let items = queries::attachments::list_project_orphans(
        conn,
        project_id,
        crate::storage::ORPHAN_GRACE_SECONDS,
    )?;
    let total_bytes = items
        .iter()
        .fold(0_i64, |total, item| total.saturating_add(item.size_bytes));
    Ok(PendingOrphanList {
        items,
        grace_seconds: crate::storage::ORPHAN_GRACE_SECONDS,
        total_bytes,
    })
}

pub(crate) fn where_used(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    attachment_id: i64,
) -> Result<WhereUsed, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let current = crate::auth::refresh_identity(&tx, identity.as_ref())?;
    let attachment = queries::attachments::get_attachment(&tx, attachment_id)?;
    authorize_read_conn(&tx, &current, &attachment)?;
    let entities = visible_links_conn(&tx, &current, attachment_id)?;
    let siblings = queries::attachments::duplicates_of(&tx, attachment_id, &attachment.sha256)?;
    let mut duplicates = Vec::with_capacity(siblings.len());
    for sibling in siblings {
        duplicates.push(DuplicateFile {
            attachment_id: sibling.id,
            filename: sibling.filename,
            entities: visible_links_conn(&tx, &current, sibling.id)?,
        });
    }
    tx.commit()?;
    Ok(WhereUsed {
        entities,
        duplicates,
    })
}

pub(crate) fn authorize_read(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    attachment_id: i64,
) -> Result<(), LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let current = crate::auth::refresh_identity(&tx, identity.as_ref())?;
    let attachment = queries::attachments::get_attachment(&tx, attachment_id)?;
    authorize_read_conn(&tx, &current, &attachment)?;
    tx.commit()?;
    Ok(())
}

fn authorize_read_conn(
    conn: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    attachment: &crate::db::models::Attachment,
) -> Result<(), LificError> {
    let project_ids = owning_project_ids_conn(conn, attachment.id)?;
    if project_ids.is_empty() {
        if matches!(identity.as_ref(), Some(caller) if caller.user.is_admin || Some(caller.user.id) == attachment.uploader_id)
        {
            return Ok(());
        }
        return Err(LificError::Forbidden(
            "not authorized to read this attachment".into(),
        ));
    }
    let mut last_error = None;
    for project_id in project_ids {
        match crate::authz::require_role_conn(conn, identity, project_id, Role::Viewer) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error
        .unwrap_or_else(|| LificError::Forbidden("not authorized to read this attachment".into())))
}

fn owning_project_ids_conn(
    conn: &rusqlite::Connection,
    attachment_id: i64,
) -> Result<Vec<i64>, LificError> {
    let links = queries::attachments::links_for_attachment(conn, attachment_id)?;
    let mut project_ids = Vec::new();
    for (raw_entity, entity_id) in links {
        let Ok(entity) = raw_entity.parse::<crate::db::models::AttachmentEntity>() else {
            continue;
        };
        let project_id = match resolve_entity_project_conn(conn, entity, entity_id) {
            Ok(project_id) => project_id,
            Err(LificError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        if let Some(project_id) = project_id
            && !project_ids.contains(&project_id)
        {
            project_ids.push(project_id);
        }
    }
    Ok(project_ids)
}

fn visible_links_conn(
    conn: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    attachment_id: i64,
) -> Result<Vec<LinkedEntity>, LificError> {
    let links = queries::attachments::links_for_attachment(conn, attachment_id)?;
    let mut out = Vec::new();
    for (raw_entity, entity_id) in links {
        let Ok(entity) = raw_entity.parse::<crate::db::models::AttachmentEntity>() else {
            continue;
        };
        let project_id = match resolve_entity_project_conn(conn, entity, entity_id) {
            Ok(project_id) => project_id,
            Err(LificError::NotFound(_)) => continue,
            Err(error) => return Err(error),
        };
        let visible = match project_id {
            Some(project_id) => {
                crate::authz::require_role_conn(conn, identity, project_id, Role::Viewer).is_ok()
            }
            None => crate::authz::require_workspace_admin_conn(conn, identity).is_ok(),
        };
        if visible && let Some(entity) = describe_entity_conn(conn, entity, entity_id) {
            out.push(entity);
        }
    }
    Ok(out)
}

fn resolve_entity_project_conn(
    conn: &rusqlite::Connection,
    entity: crate::db::models::AttachmentEntity,
    entity_id: i64,
) -> Result<Option<i64>, LificError> {
    match entity {
        crate::db::models::AttachmentEntity::Issue => {
            queries::get_issue(conn, entity_id).map(|issue| Some(issue.project_id))
        }
        crate::db::models::AttachmentEntity::Page => {
            queries::get_page(conn, entity_id).map(|page| page.project_id)
        }
        crate::db::models::AttachmentEntity::Comment => {
            let comment = queries::comments::get_comment(conn, entity_id)?;
            if let Some(issue_id) = comment.issue_id {
                queries::get_issue(conn, issue_id).map(|issue| Some(issue.project_id))
            } else if let Some(page_id) = comment.page_id {
                queries::get_page(conn, page_id).map(|page| page.project_id)
            } else {
                Ok(None)
            }
        }
    }
}

fn describe_entity_conn(
    conn: &rusqlite::Connection,
    entity: crate::db::models::AttachmentEntity,
    entity_id: i64,
) -> Option<LinkedEntity> {
    match entity {
        crate::db::models::AttachmentEntity::Issue => {
            queries::get_issue(conn, entity_id)
                .ok()
                .map(|issue| LinkedEntity {
                    entity_type: "issue".into(),
                    entity_id,
                    identifier: Some(issue.identifier),
                    title: issue.title,
                    page_id: None,
                })
        }
        crate::db::models::AttachmentEntity::Page => {
            queries::get_page(conn, entity_id)
                .ok()
                .map(|page| LinkedEntity {
                    entity_type: "page".into(),
                    entity_id,
                    identifier: Some(page.identifier),
                    title: page.title,
                    page_id: Some(page.id),
                })
        }
        crate::db::models::AttachmentEntity::Comment => {
            queries::comments::get_comment(conn, entity_id)
                .ok()
                .map(|comment| {
                    let parent = if let Some(issue_id) = comment.issue_id {
                        queries::get_issue(conn, issue_id)
                            .ok()
                            .map(|issue| (Some(issue.identifier), None))
                    } else if let Some(page_id) = comment.page_id {
                        queries::get_page(conn, page_id)
                            .ok()
                            .map(|page| (Some(page.identifier), Some(page.id)))
                    } else {
                        None
                    };
                    let (identifier, page_id) = parent.unwrap_or((None, None));
                    LinkedEntity {
                        entity_type: "comment".into(),
                        entity_id,
                        identifier,
                        title: comment_title(&comment.content),
                        page_id,
                    }
                })
        }
    }
}

const COMMENT_TITLE_CHARS: usize = 80;

fn comment_title(content: &str) -> String {
    let line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    if line.chars().count() > COMMENT_TITLE_CHARS {
        let head = line.chars().take(COMMENT_TITLE_CHARS).collect::<String>();
        format!("{head}...")
    } else {
        line.to_owned()
    }
}

pub(crate) fn list_project_files(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
    query: &ProjectAttachmentQuery,
) -> Result<ProjectAttachmentPage, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let current = crate::auth::refresh_identity(&tx, identity.as_ref())?;
    crate::authz::require_role_conn(&tx, &current, project_id, Role::Viewer)?;
    let page = queries::attachments::list_project_attachments(&tx, project_id, query)?;
    tx.commit()?;
    Ok(page)
}

pub(crate) fn list_project_orphans(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
) -> Result<PendingOrphanList, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let current = crate::auth::refresh_identity(&tx, identity.as_ref())?;
    crate::authz::require_role_conn(&tx, &current, project_id, Role::Viewer)?;
    let result = orphan_list_conn(&tx, project_id)?;
    tx.commit()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actor::Transport,
        db::{
            models::{AttachmentEntity, CreateIssue, User},
            queries,
        },
    };

    fn identity(user: &User) -> ResolvedIdentity {
        ResolvedIdentity {
            user: crate::auth::fresh_auth_user(user),
            transport: Transport::Web,
        }
    }

    #[test]
    fn project_files_page_returns_filtered_rows_and_whole_result_totals() {
        let (db, _admin, _lead, _maintainer, viewer, _outsider, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let issue = {
            let conn = db.write().unwrap();
            let issue = queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id,
                    title: "Files test issue".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let first = queries::attachments::create_attachment(
                &conn,
                &"a".repeat(64),
                "a-screen.png",
                "image/png",
                120,
                Some(viewer.id),
            )
            .unwrap();
            let second = queries::attachments::create_attachment(
                &conn,
                &"b".repeat(64),
                "b-screen.png",
                "image/png",
                80,
                Some(viewer.id),
            )
            .unwrap();
            let text_attachment = queries::attachments::create_attachment(
                &conn,
                &"c".repeat(64),
                "notes.txt",
                "text/plain",
                500,
                Some(viewer.id),
            )
            .unwrap();
            queries::attachments::link_attachment(
                &conn,
                first.id,
                AttachmentEntity::Issue,
                issue.id,
            )
            .unwrap();
            queries::attachments::link_attachment(
                &conn,
                second.id,
                AttachmentEntity::Issue,
                issue.id,
            )
            .unwrap();
            queries::attachments::link_attachment(
                &conn,
                text_attachment.id,
                AttachmentEntity::Issue,
                issue.id,
            )
            .unwrap();
            issue
        };
        let mut query = ProjectAttachmentQuery {
            mime_class: Some("image".into()),
            limit: Some(1),
            offset: Some(0),
            sort: Some("filename".into()),
            ..Default::default()
        };
        let result = list_project_files(&db, &Some(identity(&viewer)), project_id, &query).unwrap();
        assert_eq!(result.total_count, 2);
        assert_eq!(result.total_bytes, 200);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].filename, "a-screen.png");
        assert_eq!(result.items[0].entities[0].entity_id, issue.id);
        assert!(result.has_more);

        query.offset = Some(1);
        let next = list_project_files(&db, &Some(identity(&viewer)), project_id, &query).unwrap();
        assert_eq!(next.items.len(), 1);
        assert_eq!(next.items[0].filename, "b-screen.png");
        assert!(!next.has_more);
        assert_eq!(next.total_count, 2);
        assert_eq!(next.total_bytes, 200);
    }

    #[test]
    fn project_files_rejects_an_outsider() {
        let (db, _admin, _lead, _maintainer, _viewer, outsider, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let result = list_project_files(
            &db,
            &Some(identity(&outsider)),
            project_id,
            &ProjectAttachmentQuery::default(),
        );
        assert!(matches!(result, Err(LificError::Forbidden(_))));
    }

    #[test]
    fn project_files_rechecks_stale_admin_authority() {
        let (db, admin, _lead, _maintainer, _viewer, _outsider, project_id) =
            crate::api::test_helpers::setup_membership_test();
        db.write()
            .unwrap()
            .execute("UPDATE users SET is_admin = 0 WHERE id = ?1", [admin.id])
            .unwrap();
        let result = list_project_files(
            &db,
            &Some(identity(&admin)),
            project_id,
            &ProjectAttachmentQuery::default(),
        );
        assert!(matches!(result, Err(LificError::Forbidden(_))));
    }

    #[test]
    fn unlinked_attachment_read_uses_the_live_bot_identity_not_its_owner() {
        let (db, _admin, _lead, _maintainer, owner, _outsider, _project_id) =
            crate::api::test_helpers::setup_membership_test();
        let (bot, bot_upload, owner_upload) = {
            let conn = db.write().unwrap();
            let bot = queries::users::create_bot_user(
                &conn,
                owner.id,
                "files-test-bot",
                "Files test bot",
                None,
            )
            .unwrap();
            let bot_upload = queries::attachments::create_attachment(
                &conn,
                &"f".repeat(64),
                "bot-upload.txt",
                "text/plain",
                4,
                Some(bot.id),
            )
            .unwrap();
            let owner_upload = queries::attachments::create_attachment(
                &conn,
                &"g".repeat(64),
                "owner-upload.txt",
                "text/plain",
                6,
                Some(owner.id),
            )
            .unwrap();
            (bot, bot_upload, owner_upload)
        };
        let bot_identity = Some(crate::auth::fresh_identity(
            &bot,
            crate::actor::Transport::Mcp,
        ));

        assert!(authorize_read(&db, &bot_identity, bot_upload.id).is_ok());
        assert!(matches!(
            authorize_read(&db, &bot_identity, owner_upload.id),
            Err(LificError::Forbidden(_))
        ));
    }

    #[test]
    fn unlinked_attachment_read_does_not_inherit_admin_owner_for_bot() {
        let (db, admin, _lead, _maintainer, _viewer, _outsider, _project_id) =
            crate::api::test_helpers::setup_membership_test();
        let (bot, owner_upload) = {
            let conn = db.write().unwrap();
            let bot = queries::users::create_bot_user(
                &conn,
                admin.id,
                "files-admin-owner-bot",
                "Files admin owner bot",
                None,
            )
            .unwrap();
            let owner_upload = queries::attachments::create_attachment(
                &conn,
                &"h".repeat(64),
                "admin-owner-upload.txt",
                "text/plain",
                6,
                Some(admin.id),
            )
            .unwrap();
            (bot, owner_upload)
        };
        let bot_identity = Some(crate::auth::fresh_identity(
            &bot,
            crate::actor::Transport::Mcp,
        ));

        assert!(matches!(
            authorize_read(&db, &bot_identity, owner_upload.id),
            Err(LificError::Forbidden(_))
        ));
    }

    #[test]
    fn project_orphans_returns_member_uploads_with_server_grace_and_total_bytes() {
        let (db, _admin, _lead, _maintainer, viewer, outsider, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let (orphan_id, linked_id) = {
            let conn = db.write().unwrap();
            let orphan = queries::attachments::create_attachment(
                &conn,
                &"d".repeat(64),
                "pending.txt",
                "text/plain",
                96,
                Some(viewer.id),
            )
            .unwrap();
            let linked = queries::attachments::create_attachment(
                &conn,
                &"e".repeat(64),
                "linked.txt",
                "text/plain",
                1_024,
                Some(viewer.id),
            )
            .unwrap();
            let issue = queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id,
                    title: "Linked file".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            queries::attachments::link_attachment(
                &conn,
                linked.id,
                AttachmentEntity::Issue,
                issue.id,
            )
            .unwrap();
            queries::attachments::create_attachment(
                &conn,
                &"f".repeat(64),
                "outsider.txt",
                "text/plain",
                2_048,
                Some(outsider.id),
            )
            .unwrap();
            (orphan.id, linked.id)
        };
        let result = list_project_orphans(&db, &Some(identity(&viewer)), project_id).unwrap();
        assert_eq!(result.grace_seconds, crate::storage::ORPHAN_GRACE_SECONDS);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].id, orphan_id);
        assert_ne!(result.items[0].id, linked_id);
        assert_eq!(result.items[0].filename, "pending.txt");
        assert_eq!(result.total_bytes, 96);
    }
}
