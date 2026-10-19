//! Native final delete action. The workspace owns delay and cancellation.
use super::super::{context, session};
use crate::{error::LificError, realtime::RealtimeHub, services::issues::IssueDelete};
use topcoat::{
    context::{Cx, app_context},
    runtime::procedure,
};

#[procedure("/__native_issue_edit/delete")]
pub(crate) async fn commit_delete(
    cx: &Cx,
    account_id: i64,
    issue_id: i64,
) -> topcoat::Result<(i64, i64, i64)> {
    let deleted = session::read(cx, commit(cx, account_id, issue_id).await)?;
    Ok((deleted.issue_id, deleted.project_id, deleted.tombstone_seq))
}

pub(crate) async fn commit(
    cx: &Cx,
    account_id: i64,
    issue_id: i64,
) -> Result<IssueDelete, LificError> {
    let caller = context::caller(cx)?;
    let user = crate::api::require_user(&caller.identity)?;
    if user.id != account_id {
        return Err(LificError::Forbidden(
            "pending deletion belongs to another account".into(),
        ));
    }
    caller
        .scope(async {
            crate::services::issues::commit_issue_delete(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                issue_id,
            )
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        auth::AuthState,
        db::{
            self,
            models::{CreateIssue, Issue, Role, User},
            queries,
        },
        realtime::RealtimeEvent,
    };
    use topcoat::context::CxTestBuilder;
    struct Fixture {
        db: db::DbPool,
        actor: User,
        admin: User,
        issue: Issue,
        token: String,
        realtime: RealtimeHub,
    }
    fn fixture() -> Fixture {
        let (db, admin, _, actor, _, _, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let (issue, token) = {
            let conn = db.write().unwrap();
            (
                queries::create_issue(
                    &conn,
                    &CreateIssue {
                        project_id,
                        title: "Native deletion".into(),
                        ..Default::default()
                    },
                )
                .unwrap(),
                queries::users::create_session(&conn, actor.id, None)
                    .unwrap()
                    .token,
            )
        };
        Fixture {
            db,
            actor,
            admin,
            issue,
            token,
            realtime: RealtimeHub::new(),
        }
    }
    fn context(f: &Fixture, token: &str) -> Cx {
        let (parts, ()) = axum::http::Request::builder()
            .header("cookie", format!("lific_token={token}"))
            .body(())
            .unwrap()
            .into_parts();
        CxTestBuilder::new()
            .app_context(AuthState {
                db: f.db.clone(),
                public_url: "https://test.local".into(),
                required: true,
            })
            .app_context(f.realtime.clone())
            .request_context(parts)
            .build()
    }
    fn live_without_delete(f: &Fixture) {
        let conn = f.db.read().unwrap();
        assert_eq!(
            queries::get_issue(&conn, f.issue.id).unwrap().seq,
            f.issue.seq
        );
        let count:i64 = conn.query_row("SELECT count(*) FROM audit_log WHERE entity_type='issue' AND entity_id=?1 AND action='delete'", [f.issue.id], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
    }
    #[tokio::test]
    async fn native_delete_action_uses_fresh_cookie_web_actor_and_publishes_once() {
        let f = fixture();
        let cx = context(&f, &f.token);
        let mut events = f.realtime.subscribe();
        let deleted = commit(&cx, f.actor.id, f.issue.id).await.unwrap();
        let conn = f.db.read().unwrap();
        assert!(matches!(
            queries::get_issue(&conn, f.issue.id),
            Err(LificError::NotFound(_))
        ));
        let seq = queries::issue_seq(&conn, f.issue.id).unwrap();
        assert!(seq > f.issue.seq);
        assert_eq!(deleted.tombstone_seq, seq);
        assert_eq!(deleted.issue_id, f.issue.id);
        assert_eq!(deleted.project_id, f.issue.project_id);
        let rows:Vec<(Option<i64>,String)>=conn.prepare("SELECT actor_user_id,transport FROM audit_log WHERE entity_type='issue' AND entity_id=?1 AND action='delete'").unwrap()
            .query_map([f.issue.id],|row|Ok((row.get(0)?,row.get(1)?))).unwrap().collect::<Result<_,_>>().unwrap();
        assert_eq!(rows, vec![(Some(f.actor.id), "web".into())]);
        drop(conn);
        let event = events.try_recv().unwrap();
        assert_eq!(
            event.event,
            RealtimeEvent::IssueDeleted {
                project_id: f.issue.project_id,
                issue_id: f.issue.id
            }
        );
        let envelope: serde_json::Value =
            serde_json::from_str(event.message.to_text().unwrap()).unwrap();
        assert_eq!(envelope["seq"], seq);
        assert!(matches!(
            commit(&cx, f.actor.id, f.issue.id).await,
            Err(LificError::NotFound(_))
        ));
        assert_eq!(
            queries::issue_seq(&f.db.read().unwrap(), f.issue.id).unwrap(),
            seq
        );
        assert!(events.try_recv().is_err());
    }
    #[tokio::test]
    async fn native_delete_action_retained_context_observes_demotion_removal_expiry_and_revocation()
    {
        for state in ["viewer", "removed", "expired", "revoked"] {
            let f = fixture();
            let cx = context(&f, &f.token);
            let mut events = f.realtime.subscribe();
            // Resolve once before changing authority: retained Cx must not cache it.
            assert_eq!(
                crate::api::require_user(&super::context::caller(&cx).unwrap().identity)
                    .unwrap()
                    .id,
                f.actor.id
            );
            {
                let conn = f.db.write().unwrap();
                match state {
                    "viewer" => {
                        queries::members::upsert_member(
                            &conn,
                            f.issue.project_id,
                            f.actor.id,
                            Role::Viewer,
                        )
                        .unwrap();
                    }
                    "removed" => {
                        queries::members::remove_member(&conn, f.issue.project_id, f.actor.id)
                            .unwrap();
                    }
                    "expired" => {
                        conn.execute(
                            "UPDATE sessions SET expires_at='2000-01-01 00:00:00' WHERE user_id=?1",
                            [f.actor.id],
                        )
                        .unwrap();
                    }
                    "revoked" => {
                        conn.execute("DELETE FROM sessions WHERE user_id=?1", [f.actor.id])
                            .unwrap();
                    }
                    _ => unreachable!(),
                }
            }
            assert!(
                matches!(
                    commit(&cx, f.actor.id, f.issue.id).await,
                    Err(LificError::Forbidden(_))
                ),
                "{state}"
            );
            live_without_delete(&f);
            assert!(events.try_recv().is_err());
        }
    }
    #[tokio::test]
    async fn native_delete_action_rejects_replacement_account_even_when_admin() {
        let f = fixture();
        let token = queries::users::create_session(&f.db.write().unwrap(), f.admin.id, None)
            .unwrap()
            .token;
        let cx = context(&f, &token);
        let mut events = f.realtime.subscribe();
        assert!(matches!(
            commit(&cx, f.actor.id, f.issue.id).await,
            Err(LificError::Forbidden(_))
        ));
        live_without_delete(&f);
        assert!(events.try_recv().is_err());
    }
}
