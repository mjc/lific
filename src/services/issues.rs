use crate::authz;
use crate::db::{
    DbPool,
    models::{AttachmentActor, CommentActor, Issue, Role, UpdateIssue},
};
use crate::error::LificError;
use crate::realtime::{RealtimeEvent, RealtimeHub};
use crate::resolve_caller::ResolvedIdentity;

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
