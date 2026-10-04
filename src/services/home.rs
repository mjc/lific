//! Authorized reads used by the original Home landing page.
//! Secondary read failures stay explicit so the renderer can preserve Home's
//! partial results without treating a database failure as an empty response.

use crate::db::{
    DbPool,
    models::{ActivityFeed, Issue, ListIssuesQuery, Page, Project, Role, Status},
    queries,
};
use crate::error::LificError;
use crate::resolve_caller::ResolvedIdentity;

#[derive(Debug)]
pub(crate) struct HomeReads {
    pub(crate) projects: Result<Vec<Project>, LificError>,
    pub(crate) active_issues: Result<Vec<Issue>, LificError>,
    pub(crate) todo_issues: Result<Vec<Issue>, LificError>,
    pub(crate) pages: Result<Vec<Page>, LificError>,
}

pub(crate) fn load(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
) -> Result<HomeReads, LificError> {
    let user = crate::api::require_user(identity)?;
    // Master's me() failure aborts Home loading before its secondary reads.
    // Session validation remains the native caller's responsibility.
    {
        let conn = db.read()?;
        crate::auth::fresh_caller(&conn, user.id)?;
    }
    Ok(HomeReads {
        projects: super::projects::list_visible_projects(db, identity),
        active_issues: read_issues(db, user.id, Status::Active),
        todo_issues: read_issues(db, user.id, Status::Todo),
        pages: read_pages(db, user.id),
    })
}

fn read_issues(db: &DbPool, user_id: i64, status: Status) -> Result<Vec<Issue>, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let visible = super::projects::sidebar_visibility(&tx, user_id)?;
    let mut issues = queries::list_issues(
        &tx,
        &ListIssuesQuery {
            status: Some(status),
            limit: Some(200),
            ..Default::default()
        },
    )?;
    // Preserve REST's limit-before-visibility-filter placement, including
    // its absence of an assignee filter or internal triage ordering.
    queries::retain_visible_relations(&tx, &mut issues, visible.as_ref());
    let issues = crate::authz::filter_visible(issues, &visible, |i| Some(i.project_id));
    tx.commit()?;
    Ok(issues)
}

fn read_pages(db: &DbPool, user_id: i64) -> Result<Vec<Page>, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let visible = super::projects::sidebar_visibility(&tx, user_id)?;
    // Original listAllPages() has no project_id: the existing query returns
    // workspace pages, despite the frontend's cross-project comment. Keep
    // this discrepancy explicit rather than silently changing Home's data.
    let pages = queries::list_pages(&tx, None, None, None, None, None, None, None, None)?;
    let pages = crate::authz::filter_visible(pages, &visible, |p| p.project_id);
    tx.commit()?;
    Ok(pages)
}

/// The native Home model selects at most three project IDs. Each feed uses
/// the original limit of eight and offset of zero; the renderer merges ten.
pub(crate) fn project_activity(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
) -> Result<ActivityFeed, LificError> {
    let user = crate::api::require_user(identity)?;
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let fresh = crate::auth::fresh_caller(&tx, user.id)?;
    let current_identity = identity
        .as_ref()
        .map(|caller| crate::auth::fresh_identity(&fresh, caller.transport));
    crate::authz::require_role_conn(&tx, &current_identity, project_id, Role::Viewer)?;
    let feed = queries::activity::list_activity(
        &tx,
        queries::activity::ActivityScope::Project(project_id),
        Some(8),
        Some(0),
    )?;
    tx.commit()?;
    Ok(feed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::Transport;
    use crate::db::{
        models::{CreateIssue, CreatePage, CreateProject, Role, Status, User},
        queries,
    };

    fn identity(user: &User) -> ResolvedIdentity {
        ResolvedIdentity {
            user: crate::auth::fresh_auth_user(user),
            transport: Transport::Web,
        }
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

    #[test]
    fn native_home_reads_keep_personal_order_visible_issues_and_safe_relations() {
        let (db, _, _, _, viewer, _, first_id) = crate::api::test_helpers::setup_membership_test();
        let (second, hidden, active, todo) = {
            let conn = db.write().unwrap();
            let second = queries::create_project(
                &conn,
                &CreateProject {
                    identifier: "SEC".into(),
                    name: "Second".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let hidden = queries::create_project(
                &conn,
                &CreateProject {
                    identifier: "HIDE".into(),
                    name: "Hidden".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            queries::members::upsert_member(&conn, second.id, viewer.id, Role::Viewer).unwrap();
            let visible = Some([first_id, second.id].into_iter().collect());
            queries::reorder_projects(&conn, viewer.id, &[second.id, first_id], &visible).unwrap();
            let active = issue(&conn, first_id, Status::Active, "Active without assignee");
            let todo = issue(&conn, second.id, Status::Todo, "Todo without assignee");
            let hidden_issue = issue(&conn, hidden.id, Status::Active, "Hidden work");
            queries::link_issues(&conn, active.id, todo.id, "relates_to").unwrap();
            queries::link_issues(&conn, active.id, hidden_issue.id, "blocks").unwrap();
            let stored = queries::get_issue(&conn, active.id).unwrap();
            assert_eq!(stored.relates_to, std::slice::from_ref(&todo.identifier));
            assert_eq!(stored.blocks, [hidden_issue.identifier]);
            (second, hidden, active, todo)
        };

        let reads = load(&db, &Some(identity(&viewer))).unwrap();
        let projects = reads.projects.unwrap();
        assert_eq!(
            projects.iter().map(|p| p.id).collect::<Vec<_>>(),
            [second.id, first_id]
        );
        assert_eq!(
            projects.iter().map(|p| p.sort_order).collect::<Vec<_>>(),
            [0, 1]
        );
        assert!(!projects.iter().any(|p| p.id == hidden.id));
        let active_issues = reads.active_issues.unwrap();
        assert_eq!(active_issues.len(), 1);
        assert_eq!(active_issues[0].id, active.id);
        let baseline = {
            let conn = db.read().unwrap();
            queries::list_issues(
                &conn,
                &ListIssuesQuery {
                    status: Some(Status::Active),
                    limit: Some(200),
                    ..Default::default()
                },
            )
            .unwrap()
            .into_iter()
            .find(|i| i.id == active.id)
            .unwrap()
        };
        // Home's status-list query does not hydrate relations. Preserve its
        // payload instead of replacing it with per-row single-issue reads.
        assert_eq!(active_issues[0].blocks, baseline.blocks);
        assert_eq!(active_issues[0].blocked_by, baseline.blocked_by);
        assert_eq!(active_issues[0].relates_to, baseline.relates_to);
        assert_eq!(active_issues[0].duplicates, baseline.duplicates);
        assert_eq!(active_issues[0].duplicated_by, baseline.duplicated_by);
        assert!(
            [
                &active_issues[0].blocks,
                &active_issues[0].blocked_by,
                &active_issues[0].relates_to,
                &active_issues[0].duplicates,
                &active_issues[0].duplicated_by,
            ]
            .into_iter()
            .flatten()
            .all(|identifier| !identifier.starts_with("HIDE-"))
        );
        assert_eq!(reads.todo_issues.unwrap()[0].id, todo.id);
    }

    #[test]
    fn native_home_reads_cap_each_status_at_two_hundred_and_report_page_faults() {
        let (db, _, _, _, viewer, _, project_id) =
            crate::api::test_helpers::setup_membership_test();
        {
            let conn = db.write().unwrap();
            for status in [Status::Active, Status::Todo] {
                for index in 0..201 {
                    issue(&conn, project_id, status, &format!("{status} {index}"));
                }
            }
            issue(&conn, project_id, Status::Backlog, "Not active work");
            issue(&conn, project_id, Status::Done, "Finished work");
            conn.execute("ALTER TABLE pages RENAME TO unavailable_pages", [])
                .unwrap();
        }
        let reads = load(&db, &Some(identity(&viewer))).unwrap();
        assert_eq!(reads.active_issues.unwrap().len(), 200);
        assert_eq!(reads.todo_issues.unwrap().len(), 200);
        assert!(reads.projects.is_ok());
        assert!(matches!(reads.pages, Err(LificError::Database(_))));
    }

    #[test]
    fn native_home_reads_preserve_original_pages_query_discrepancy() {
        let (db, admin, _, _, viewer, _, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let workspace = {
            let conn = db.write().unwrap();
            for project_id in [Some(project_id), None] {
                let page = queries::create_page(
                    &conn,
                    &CreatePage {
                        project_id,
                        title: "Pinned page".into(),
                        ..Default::default()
                    },
                )
                .unwrap();
                conn.execute("UPDATE pages SET pinned = 1 WHERE id = ?1", [page.id])
                    .unwrap();
            }
            queries::list_pages(&conn, None, None, None, None, None, None, None, None)
                .unwrap()
                .remove(0)
        };
        // Master's listAllPages() sends /pages without project_id. Despite
        // its comment, that query selects only workspace pages. Home then
        // requires project_id != null when deriving pinned pages.
        assert!(
            load(&db, &Some(identity(&viewer)))
                .unwrap()
                .pages
                .unwrap()
                .is_empty()
        );
        let pages = load(&db, &Some(identity(&admin))).unwrap().pages.unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].id, workspace.id);
        assert!(!pages.iter().any(|p| p.pinned && p.project_id.is_some()));
    }

    #[test]
    fn native_home_activity_reads_eight_rows_and_rechecks_project_membership() {
        let (db, _, _, _, viewer, _, project_id) =
            crate::api::test_helpers::setup_membership_test();
        {
            let conn = db.write().unwrap();
            for index in 0..10 {
                issue(
                    &conn,
                    project_id,
                    Status::Active,
                    &format!("Activity {index}"),
                );
            }
        }
        let caller = Some(identity(&viewer));
        let feed = project_activity(&db, &caller, project_id).unwrap();
        assert_eq!(feed.items.len(), 8);
        assert!(feed.has_more);
        assert!(feed.items.iter().all(|a| a.project_id == Some(project_id)));
        queries::members::remove_member(&db.write().unwrap(), project_id, viewer.id).unwrap();
        assert!(matches!(
            project_activity(&db, &caller, project_id),
            Err(LificError::Forbidden(_))
        ));
    }

    #[test]
    fn native_home_reads_reject_missing_or_disabled_callers() {
        let (db, _, _, _, viewer, _, _) = crate::api::test_helpers::setup_membership_test();
        assert!(matches!(load(&db, &None), Err(LificError::Forbidden(_))));
        let caller = Some(identity(&viewer));
        queries::users::set_active(&db.write().unwrap(), viewer.id, false).unwrap();
        assert!(matches!(load(&db, &caller), Err(LificError::Forbidden(_))));
    }

    #[test]
    fn native_home_hidden_rows_consume_each_original_two_hundred_row_budget() {
        let (db, _, _, _, viewer, _, visible_id) =
            crate::api::test_helpers::setup_membership_test();
        let hidden_id = {
            let conn = db.write().unwrap();
            let hidden = queries::create_project(
                &conn,
                &CreateProject {
                    identifier: "HIDE".into(),
                    name: "Hidden budget".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            for status in [Status::Active, Status::Todo] {
                for index in 0..200 {
                    issue(
                        &conn,
                        hidden.id,
                        status,
                        &format!("Hidden {status} {index}"),
                    );
                }
                issue(&conn, visible_id, status, &format!("Visible {status}"));
            }
            // Home uses the existing default sort_order query. Put visible
            // work after the hidden rows deterministically in both statuses.
            conn.execute(
                "UPDATE issues SET sort_order = 10000 WHERE project_id = ?1",
                [visible_id],
            )
            .unwrap();
            hidden.id
        };
        {
            let conn = db.read().unwrap();
            for status in [Status::Active, Status::Todo] {
                let baseline = queries::list_issues(
                    &conn,
                    &ListIssuesQuery {
                        status: Some(status),
                        limit: Some(200),
                        ..Default::default()
                    },
                )
                .unwrap();
                assert_eq!(baseline.len(), 200);
                assert!(baseline.iter().all(|i| i.project_id == hidden_id));
            }
        }
        let reads = load(&db, &Some(identity(&viewer))).unwrap();
        assert_eq!(reads.projects.unwrap()[0].id, visible_id);
        // Preserve master's limit-before-filter behavior, even when it
        // leaves visible work beyond the original response budget.
        assert!(reads.active_issues.unwrap().is_empty());
        assert!(reads.todo_issues.unwrap().is_empty());
    }

    #[test]
    fn native_home_stale_admin_uses_current_membership_after_demotion() {
        let (db, admin, _, _, _, _, visible_id) = crate::api::test_helpers::setup_membership_test();
        let caller = Some(identity(&admin));
        let (hidden_id, visible_issue) = {
            let conn = db.write().unwrap();
            queries::members::upsert_member(&conn, visible_id, admin.id, Role::Viewer).unwrap();
            let hidden = queries::create_project(
                &conn,
                &CreateProject {
                    identifier: "HIDE".into(),
                    name: "Admin-only work".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let visible = issue(
                &conn,
                visible_id,
                Status::Active,
                "Membership remains visible",
            );
            issue(&conn, hidden.id, Status::Todo, "Admin-only issue");
            queries::create_page(
                &conn,
                &CreatePage {
                    title: "Admin-only workspace page".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            (hidden.id, visible)
        };
        let before = load(&db, &caller).unwrap();
        assert_eq!(before.projects.unwrap().len(), 2);
        assert_eq!(before.todo_issues.unwrap().len(), 1);
        assert_eq!(before.pages.unwrap().len(), 1);
        queries::users::set_admin(&db.write().unwrap(), &admin.username, false).unwrap();
        assert!(caller.as_ref().unwrap().user.is_admin);

        let after = load(&db, &caller).unwrap();
        let projects = after.projects.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, visible_id);
        let active = after.active_issues.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, visible_issue.id);
        assert!(after.todo_issues.unwrap().is_empty());
        assert!(after.pages.unwrap().is_empty());
        assert!(project_activity(&db, &caller, visible_id).is_ok());
        assert!(matches!(
            project_activity(&db, &caller, hidden_id),
            Err(LificError::Forbidden(_))
        ));
    }
}
