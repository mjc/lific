//! Native issue save outcomes. Browser drafts are not server authority.

use topcoat::context::{Cx, app_context};

use super::model::Field;
use crate::{
    db::models::{Issue, Priority, Role, Status},
    error::LificError,
    realtime::RealtimeHub,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub(crate) identifier: String,
    pub(crate) seq: i64,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) status: Status,
    pub(crate) priority: Priority,
    pub(crate) blocks: Vec<String>,
    pub(crate) blocked_by: Vec<String>,
    pub(crate) relates_to: Vec<String>,
    pub(crate) duplicates: Vec<String>,
    pub(crate) duplicated_by: Vec<String>,
    pub(crate) labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SaveOutcome {
    Saved(Snapshot),
    Unchanged(Snapshot),
    Conflict(Snapshot),
    Reauth,
    Forbidden,
    Invalid(String),
}

pub(crate) async fn save(
    cx: &Cx,
    identifier: &str,
    field: Field,
    value: &str,
    observed_seq: i64,
) -> topcoat::Result<SaveOutcome> {
    let caller = match super::super::context::caller(cx) {
        Ok(caller) => caller,
        Err(LificError::Forbidden(_)) => return Ok(SaveOutcome::Reauth),
        Err(error) => return Err(error.into()),
    };
    match crate::api::require_user(&caller.identity) {
        Ok(_) => {}
        Err(LificError::Forbidden(_)) => return Ok(SaveOutcome::Reauth),
        Err(error) => return Err(error.into()),
    }
    let db = super::super::context::db(cx);
    let saved = match crate::services::issues::resolve_issue(db, &caller.identity, identifier) {
        Ok(issue) => issue,
        Err(LificError::Forbidden(_)) => return Ok(SaveOutcome::Forbidden),
        Err(LificError::BadRequest(message)) => return Ok(SaveOutcome::Invalid(message)),
        Err(LificError::NotFound(_)) => return Err(topcoat::router::error::not_found().into()),
        Err(error) => return Err(error.into()),
    };
    // A no-op or invalid patch must not bypass the same Maintainer gate as REST.
    match crate::authz::require_role(db, &caller.identity, saved.project_id, Role::Maintainer) {
        Ok(()) => {}
        Err(LificError::Forbidden(_)) => return Ok(SaveOutcome::Forbidden),
        Err(error) => return Err(error.into()),
    }
    let patch = match super::model::edit_patch(field, value, &saved, observed_seq) {
        Ok(Some(patch)) => patch,
        Ok(None) => return Ok(SaveOutcome::Unchanged(snapshot(saved))),
        Err(LificError::BadRequest(message)) => return Ok(SaveOutcome::Invalid(message)),
        Err(error) => return Err(error.into()),
    };
    let result = caller
        .scope(async {
            crate::services::issues::commit_issue_update(
                db,
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                saved.id,
                patch,
            )
        })
        .await;
    match result {
        Ok(issue) => Ok(SaveOutcome::Saved(snapshot(issue))),
        Err(LificError::UpdateConflict { current, .. }) => {
            // The shared writer projected this exact losing-write snapshot.
            // A later read could return a different winner and sequence.
            let current = serde_json::from_value(*current).map_err(|error| {
                LificError::Internal(format!("failed to read conflicting issue: {error}"))
            })?;
            Ok(SaveOutcome::Conflict(snapshot(current)))
        }
        Err(LificError::Forbidden(_)) => Ok(SaveOutcome::Forbidden),
        Err(LificError::BadRequest(message)) => Ok(SaveOutcome::Invalid(message)),
        Err(LificError::NotFound(_)) => Err(topcoat::router::error::not_found().into()),
        Err(error) => Err(error.into()),
    }
}

/// Project an issue already authorized and relation-scoped by the shared service.
pub(crate) fn snapshot(issue: Issue) -> Snapshot {
    Snapshot {
        identifier: issue.identifier,
        seq: issue.seq,
        title: issue.title,
        description: issue.description,
        status: issue.status,
        priority: issue.priority,
        blocks: issue.blocks,
        blocked_by: issue.blocked_by,
        relates_to: issue.relates_to,
        duplicates: issue.duplicates,
        duplicated_by: issue.duplicated_by,
        labels: issue.labels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actor::Transport,
        auth::AuthState,
        db::{
            self,
            models::{
                AttachmentEntity, CreateIssue, CreateProject, Issue, Role, UpdateIssue, User,
            },
            queries,
        },
        realtime::{RealtimeEvent, RealtimeHub},
    };
    use topcoat::context::CxTestBuilder;

    struct Fixture {
        db: db::DbPool,
        actor: User,
        outsider: User,
        issue: Issue,
        visible: Issue,
        hidden: Issue,
        token: String,
        realtime: RealtimeHub,
    }

    fn fixture() -> Fixture {
        let (db, _, _, actor, _, outsider, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let (issue, visible, hidden, token) = {
            let conn = db.write().unwrap();
            let create = |project_id, title: &str| {
                queries::create_issue(
                    &conn,
                    &CreateIssue {
                        project_id,
                        title: title.to_owned(),
                        description: "Saved body".into(),
                        status: Status::Active,
                        priority: Priority::Medium,
                        ..Default::default()
                    },
                )
                .unwrap()
            };
            let issue = create(project_id, "Saved title");
            let visible = create(project_id, "Visible relation");
            let hidden_project = queries::create_project(
                &conn,
                &CreateProject {
                    identifier: "HIDE".into(),
                    name: "Hidden project".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let hidden = create(hidden_project.id, "Hidden relation");
            for related in [&visible, &hidden] {
                queries::link_issues(&conn, issue.id, related.id, "relates_to").unwrap();
            }
            let issue = queries::get_issue(&conn, issue.id).unwrap();
            assert_eq!(
                issue.relates_to.len(),
                2,
                "fixture must prove the raw hidden relationship exists"
            );
            let token = queries::users::create_session(&conn, actor.id, None)
                .unwrap()
                .token;
            (issue, visible, hidden, token)
        };
        Fixture {
            db,
            actor,
            outsider,
            issue,
            visible,
            hidden,
            token,
            realtime: RealtimeHub::new(),
        }
    }

    fn context(fixture: &Fixture) -> Cx {
        let (parts, ()) = axum::http::Request::builder()
            .header("cookie", format!("lific_token={}", fixture.token))
            .body(())
            .unwrap()
            .into_parts();
        CxTestBuilder::new()
            .app_context(AuthState {
                db: fixture.db.clone(),
                public_url: "https://test.local".into(),
                required: true,
            })
            .app_context(fixture.realtime.clone())
            .request_context(parts)
            .build()
    }

    fn assert_scoped(fixture: &Fixture, snapshot: &Snapshot) {
        assert_eq!(snapshot.identifier, fixture.issue.identifier);
        assert_eq!(
            snapshot.relates_to,
            std::slice::from_ref(&fixture.visible.identifier)
        );
        for relations in [
            &snapshot.blocks,
            &snapshot.blocked_by,
            &snapshot.relates_to,
            &snapshot.duplicates,
            &snapshot.duplicated_by,
        ] {
            assert!(!relations.contains(&fixture.hidden.identifier));
        }
    }

    #[tokio::test]
    async fn native_issue_edit_action_commits_fresh_cookie_actor_and_scoped_snapshot() {
        let fixture = fixture();
        let cx = context(&fixture);
        let mut events = fixture.realtime.subscribe();
        let result = save(
            &cx,
            &fixture.issue.identifier,
            Field::Title,
            "  Native saved title\n",
            fixture.issue.seq,
        )
        .await
        .unwrap();
        let SaveOutcome::Saved(saved) = result else {
            panic!("valid action did not save: {result:?}")
        };
        assert_scoped(&fixture, &saved);
        assert_eq!(saved.title, "Native saved title");
        assert!(saved.seq > fixture.issue.seq);
        let conn = fixture.db.read().unwrap();
        let stored = queries::get_issue(&conn, fixture.issue.id).unwrap();
        assert_eq!(stored.title, saved.title);
        assert_eq!(stored.seq, saved.seq);
        let actor: (Option<i64>, String) = conn.query_row(
            "SELECT actor_user_id, transport FROM audit_log WHERE entity_type = 'issue' AND entity_id = ?1 AND field = 'title' ORDER BY id DESC LIMIT 1",
            [fixture.issue.id], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(actor, (Some(fixture.actor.id), "web".into()));
        let event = events.try_recv().unwrap();
        assert_eq!(
            event.event,
            RealtimeEvent::IssueUpdated {
                project_id: fixture.issue.project_id,
                issue_id: fixture.issue.id
            }
        );
        let envelope: serde_json::Value =
            serde_json::from_str(event.message.to_text().unwrap()).unwrap();
        assert_eq!(envelope["seq"], saved.seq);
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn native_issue_edit_action_returns_captured_scoped_conflict_without_overwriting_winner()
    {
        let fixture = fixture();
        let identity = Some(crate::auth::fresh_identity(&fixture.actor, Transport::Web));
        let winner = crate::services::issues::commit_issue_update(
            &fixture.db,
            &fixture.realtime,
            &identity,
            fixture.issue.id,
            UpdateIssue {
                description: Some("Winner **body**".into()),
                expected_seq: Some(fixture.issue.seq),
                ..Default::default()
            },
        )
        .unwrap();
        let mut events = fixture.realtime.subscribe();
        let result = save(
            &context(&fixture),
            &fixture.issue.identifier,
            Field::Description,
            "Losing **dirty draft**",
            fixture.issue.seq,
        )
        .await
        .unwrap();
        let SaveOutcome::Conflict(current) = result else {
            panic!("stale edit did not conflict: {result:?}")
        };
        assert_scoped(&fixture, &current);
        assert_eq!(current.seq, winner.seq);
        assert_eq!(current.description, "Winner **body**");
        let stored = queries::get_issue(&fixture.db.read().unwrap(), fixture.issue.id).unwrap();
        assert_eq!(stored.seq, winner.seq);
        assert_eq!(stored.description, winner.description);
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn native_issue_edit_action_rejects_revoked_cookie_on_a_retained_context() {
        let fixture = fixture();
        let cx = context(&fixture);
        queries::users::delete_session(&fixture.db.write().unwrap(), &fixture.token).unwrap();
        assert!(
            queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token).is_err()
        );
        let mut events = fixture.realtime.subscribe();
        let result = save(
            &cx,
            &fixture.issue.identifier,
            Field::Title,
            "Revoked draft",
            fixture.issue.seq,
        )
        .await
        .unwrap();
        assert_eq!(result, SaveOutcome::Reauth);
        let stored = queries::get_issue(&fixture.db.read().unwrap(), fixture.issue.id).unwrap();
        assert_eq!(stored.title, fixture.issue.title);
        assert_eq!(stored.seq, fixture.issue.seq);
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn native_issue_edit_action_observes_current_permission_after_demotion() {
        let fixture = fixture();
        let cx = context(&fixture);
        queries::members::upsert_member(
            &fixture.db.write().unwrap(),
            fixture.issue.project_id,
            fixture.actor.id,
            Role::Viewer,
        )
        .unwrap();
        let mut events = fixture.realtime.subscribe();
        for (field, value) in [
            (Field::Title, "Demoted draft"),
            (Field::Title, ""),
            (Field::Title, fixture.issue.title.as_str()),
            (Field::Status, "not-a-status"),
        ] {
            let result = save(
                &cx,
                &fixture.issue.identifier,
                field,
                value,
                fixture.issue.seq,
            )
            .await
            .unwrap();
            assert_eq!(result, SaveOutcome::Forbidden);
        }
        let stored = queries::get_issue(&fixture.db.read().unwrap(), fixture.issue.id).unwrap();
        assert_eq!(stored.title, fixture.issue.title);
        assert_eq!(stored.seq, fixture.issue.seq);
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn native_issue_edit_action_description_reconciles_only_the_callers_attachment() {
        let fixture = fixture();
        let (mine, theirs) = {
            let conn = fixture.db.write().unwrap();
            let mine = queries::attachments::create_attachment(
                &conn,
                &"5".repeat(64),
                "mine.png",
                "image/png",
                1,
                Some(fixture.actor.id),
            )
            .unwrap();
            let theirs = queries::attachments::create_attachment(
                &conn,
                &"6".repeat(64),
                "theirs.png",
                "image/png",
                1,
                Some(fixture.outsider.id),
            )
            .unwrap();
            (mine, theirs)
        };
        let description = format!(
            "![mine](/api/attachments/{}) ![theirs](/api/attachments/{})",
            mine.id, theirs.id
        );
        let result = save(
            &context(&fixture),
            &fixture.issue.identifier,
            Field::Description,
            &description,
            fixture.issue.seq,
        )
        .await
        .unwrap();
        let SaveOutcome::Saved(saved) = result else {
            panic!("description action did not save: {result:?}")
        };
        assert_scoped(&fixture, &saved);
        assert_eq!(saved.description, description);
        let attachments = queries::attachments::list_for_entity(
            &fixture.db.read().unwrap(),
            AttachmentEntity::Issue,
            fixture.issue.id,
        )
        .unwrap();
        assert_eq!(
            attachments
                .iter()
                .map(|attachment| attachment.id)
                .collect::<Vec<_>>(),
            vec![mine.id]
        );
    }
}
