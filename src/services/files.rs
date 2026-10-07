//! Authorized read models for the native project Files manager.

use crate::{
    db::{
        DbPool,
        models::{PendingOrphanList, ProjectAttachmentPage, ProjectAttachmentQuery, Role},
        queries,
    },
    error::LificError,
    resolve_caller::ResolvedIdentity,
};

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
    let items = queries::attachments::list_project_orphans(
        &tx,
        project_id,
        crate::storage::ORPHAN_GRACE_SECONDS,
    )?;
    let total_bytes = items
        .iter()
        .fold(0_i64, |total, item| total.saturating_add(item.size_bytes));
    tx.commit()?;
    Ok(PendingOrphanList {
        items,
        grace_seconds: crate::storage::ORPHAN_GRACE_SECONDS,
        total_bytes,
    })
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
