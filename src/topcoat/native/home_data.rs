//! Initial Home reads. Refresh and retention of a previous snapshot are separate.

use topcoat::context::Cx;

use crate::{
    db::models::{Activity, AuthUser, Issue, Page, Project},
    error::LificError,
};

/// Display data for this render, never reusable authority for a later action.
#[derive(Debug)]
pub(crate) struct Snapshot {
    pub(crate) user: AuthUser,
    pub(crate) projects: Vec<Project>,
    pub(crate) issues: Vec<Issue>,
    pub(crate) pinned_pages: Vec<Page>,
    pub(crate) activity: Vec<Activity>,
}

pub(crate) fn snapshot(cx: &Cx) -> Result<Snapshot, LificError> {
    let caller = super::context::caller(cx)?;
    let user = crate::api::require_user(&caller.identity)?;
    let db = super::context::db(cx);
    #[cfg(test)]
    if let Some(reads) =
        topcoat::context::try_app_context::<super::home_fixture::HomeSnapshotReads>(cx)
    {
        reads.record();
    }
    let reads = crate::services::home::load(db, &caller.identity)?;

    let projects = section(reads.projects, "projects");
    let mut issues = section(reads.active_issues, "active issues");
    issues.extend(section(reads.todo_issues, "todo issues"));
    // Preserve original Home's filter, including the documented discrepancy:
    // the shared no-project read returns workspace pages, not project pages.
    let mut pinned_pages: Vec<_> = section(reads.pages, "pages")
        .into_iter()
        .filter(|page| page.pinned && page.project_id.is_some())
        .collect();
    pinned_pages.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    pinned_pages.truncate(8);

    let digest_project_ids = super::home_model::derive_home(&projects, &issues).digest_project_ids;
    let mut activity = Vec::new();
    for project_id in digest_project_ids {
        let feed = crate::services::home::project_activity(db, &caller.identity, project_id);
        activity.extend(section(feed.map(|feed| feed.items), "activity"));
    }
    // Stable sorting retains selected-project/feed order for equal times.
    activity.sort_by(|a, b| b.ts.cmp(&a.ts));
    activity.truncate(10);

    Ok(Snapshot {
        user,
        projects,
        issues,
        pinned_pages,
        activity,
    })
}

fn section<T>(read: Result<Vec<T>, LificError>, section: &str) -> Vec<T> {
    match read {
        Ok(items) => items,
        Err(error) => {
            tracing::warn!(section, error = %error, "native Home read failed");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use topcoat::{context::CxTestBuilder, router::request::Request};

    use super::*;
    use crate::{
        auth::AuthState,
        db::{
            DbPool,
            models::{CreateIssue, CreateProject, Role, Status, User},
            queries,
        },
    };

    fn cx(db: &DbPool, cookie: &str) -> Cx {
        let mut request = Request::new(());
        request
            .headers_mut()
            .insert("cookie", cookie.parse().unwrap());
        let (parts, ()) = request.into_parts();
        CxTestBuilder::new()
            .app_context(AuthState {
                db: db.clone(),
                public_url: "https://test.local".into(),
                // A bad credential must not fall back to the first admin.
                required: false,
            })
            .request_context(parts)
            .build()
    }

    fn issue(conn: &rusqlite::Connection, project_id: i64, status: Status, title: &str) -> Issue {
        queries::create_issue(
            conn,
            &CreateIssue {
                project_id,
                title: title.into(),
                status,
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn pin_issue_time(conn: &rusqlite::Connection, issue_id: i64, time: &str) {
        let tx = conn.unchecked_transaction().unwrap();
        // Allocate the same instance sequence as the stamp layer. Supplying
        // the new seq keeps issues_updated's ordinary-edit clock from
        // overwriting this explicit history timestamp; all triggers remain.
        tx.execute("UPDATE sync_seq SET value = value + 1 WHERE id = 1", [])
            .unwrap();
        tx.execute(
            "UPDATE issues SET updated_at = ?1,
             seq = (SELECT value FROM sync_seq WHERE id = 1) WHERE id = ?2",
            (time, issue_id),
        )
        .unwrap();
        tx.commit().unwrap();
        assert_eq!(queries::get_issue(conn, issue_id).unwrap().updated_at, time);
    }

    fn fixture() -> (DbPool, User, String, i64, i64) {
        let (db, _, _, _, viewer, _, first_id) = crate::api::test_helpers::setup_membership_test();
        let (cookie, second_id) = {
            let conn = db.write().unwrap();
            let session = queries::users::create_session(&conn, viewer.id, None).unwrap();
            let second = queries::create_project(
                &conn,
                &CreateProject {
                    name: "Second".into(),
                    identifier: "SEC".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            queries::members::upsert_member(&conn, second.id, viewer.id, Role::Viewer).unwrap();
            let hidden = queries::create_project(
                &conn,
                &CreateProject {
                    name: "Hidden".into(),
                    identifier: "HIDE".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let active = issue(&conn, first_id, Status::Active, "Visible active work");
            let todo = issue(&conn, second.id, Status::Todo, "Visible todo work");
            issue(&conn, hidden.id, Status::Active, "Hidden work");
            pin_issue_time(&conn, active.id, "2026-10-01 12:00:00");
            pin_issue_time(&conn, todo.id, "2026-10-02 12:00:00");
            (
                format!("other=value; lific_token={}", session.token),
                second.id,
            )
        };
        (db, viewer, cookie, first_id, second_id)
    }

    #[test]
    fn home_snapshot_reads_cookie_identity_visible_work_and_selected_activity() {
        let (db, viewer, cookie, first_id, second_id) = fixture();
        let cx = cx(&db, &cookie);
        {
            let conn = db.write().unwrap();
            // The request already exists; display data must still be fresh.
            conn.execute(
                "UPDATE users SET display_name = 'Home Viewer' WHERE id = ?1",
                [viewer.id],
            )
            .unwrap();
            for project_id in [first_id, second_id] {
                for _ in 0..8 {
                    issue(&conn, project_id, Status::Backlog, "Digest history");
                }
                let time = if project_id == second_id {
                    "2026-10-02 12:00:00"
                } else {
                    "2026-10-01 12:00:00"
                };
                conn.execute(
                    "UPDATE audit_log SET ts = ?1 WHERE project_id = ?2",
                    (time, project_id),
                )
                .unwrap();
            }
        }
        let data = snapshot(&cx).unwrap();
        assert_eq!(data.user.id, viewer.id);
        assert_eq!(data.user.username, "viewer");
        assert_eq!(data.user.display_name, "Home Viewer");
        assert!(!data.user.is_admin);
        assert_eq!(data.projects.len(), 2);
        assert!(
            data.projects
                .iter()
                .all(|p| [first_id, second_id].contains(&p.id))
        );
        assert_eq!(
            data.issues
                .iter()
                .map(|i| i.title.as_str())
                .collect::<Vec<_>>(),
            ["Visible active work", "Visible todo work"]
        );
        let model = super::super::home_model::derive_home(&data.projects, &data.issues);
        assert_eq!(model.digest_project_ids, [second_id, first_id]);
        assert!(data.pinned_pages.is_empty());
        assert_eq!(data.activity.len(), 10);
        assert!(
            data.activity[..8]
                .iter()
                .all(|a| a.project_id == Some(second_id))
        );
        assert!(
            data.activity[8..]
                .iter()
                .all(|a| a.project_id == Some(first_id))
        );
        let feed = crate::services::home::project_activity(
            &db,
            &Some(crate::resolve_caller::ResolvedIdentity {
                user: data.user.clone(),
                transport: crate::actor::Transport::Web,
            }),
            second_id,
        )
        .unwrap();
        assert_eq!(
            data.activity[..8].iter().map(|a| a.id).collect::<Vec<_>>(),
            feed.items.iter().map(|a| a.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn home_snapshot_rejects_malformed_and_revoked_cookies_without_operator_fallback() {
        let (db, viewer, cookie, _, _) = fixture();
        assert!(matches!(
            snapshot(&cx(&db, "lific_token=invalid")),
            Err(LificError::Forbidden(_))
        ));
        let cx = cx(&db, &cookie);
        assert_eq!(snapshot(&cx).unwrap().user.id, viewer.id);
        db.write()
            .unwrap()
            .execute("DELETE FROM sessions WHERE user_id = ?1", [viewer.id])
            .unwrap();
        assert!(matches!(snapshot(&cx), Err(LificError::Forbidden(_))));
    }

    #[test]
    fn home_snapshot_keeps_projects_and_active_work_when_pages_read_fails() {
        let (db, viewer, cookie, _, _) = fixture();
        db.write()
            .unwrap()
            .execute("ALTER TABLE pages RENAME TO unavailable_pages", [])
            .unwrap();
        let data = snapshot(&cx(&db, &cookie)).unwrap();
        assert_eq!(data.user.id, viewer.id);
        assert_eq!(data.projects.len(), 2);
        assert_eq!(data.issues.len(), 2);
        assert!(data.pinned_pages.is_empty());
        assert!(!data.activity.is_empty());
    }

    #[test]
    fn native_activity_rate_baseline_rechecks_visible_membership() {
        let (db, _, cookie, first_id, second_id) = fixture();
        let cx = cx(&db, &cookie);
        let user = snapshot(&cx).unwrap().user;
        let count = || match crate::realtime::activity_baseline(&db, &user).unwrap() {
            crate::realtime::RealtimeEvent::ActivityBaseline { day_count } => day_count,
            event => panic!("unexpected baseline {event:?}"),
        };
        db.write()
            .unwrap()
            .execute("DELETE FROM audit_log", [])
            .unwrap();
        {
            let conn = db.write().unwrap();
            issue(&conn, first_id, Status::Active, "Visible rate update");
            let hidden = queries::resolve_project_identifier(&conn, "HIDE").unwrap();
            issue(&conn, hidden, Status::Active, "Hidden rate update");
        }
        assert_eq!(count(), 1);
        let mut rate =
            super::super::home_activity_rate::State::restore("", user.id, user.is_admin, 1_000);
        rate.admit_connection("baseline-fixture");
        assert!(rate.seed(&cx, &user, 1_000));
        let label = rate.presentation(1_000);
        assert!(rate.seed(&cx, &user, 3_601_000));
        assert_eq!(rate.presentation(3_601_000), label);
        let stored = serde_json::to_string(&rate).unwrap();
        let restored = super::super::home_activity_rate::State::restore(
            &stored,
            user.id,
            user.is_admin,
            3_601_001,
        );
        assert!(
            !restored.baseline_due(3_601_001),
            "An unchanged successful hourly baseline retains its renewed timestamp across content renders."
        );
        db.write()
            .unwrap()
            .execute(
                "DELETE FROM project_members WHERE project_id IN (?1, ?2) AND user_id = ?3",
                (first_id, second_id, user.id),
            )
            .unwrap();
        assert_eq!(count(), 0);
    }
}
