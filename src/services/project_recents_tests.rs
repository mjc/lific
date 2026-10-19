use super::*;
use crate::db::models::{CreateIssue, CreateModule, CreatePage, CreatePlan};
fn fixture() -> (DbPool, ResolvedIdentity, i64) {
    let (db, _, _, _, viewer, _, project) = crate::api::test_helpers::setup_membership_test();
    db.write()
        .unwrap()
        .execute(
            "UPDATE projects SET identifier='LIF' WHERE id=?1",
            [project],
        )
        .unwrap();
    (
        db,
        crate::auth::fresh_identity(&viewer, Transport::Web),
        project,
    )
}
#[test]
fn recent_issues_request_the_same_five_updated_rows_and_retain_identifier_links() {
    let (db, identity, project) = fixture();
    let mut expected = Vec::new();
    {
        let conn = db.write().unwrap();
        for index in 1..=6 {
            let item = queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id: project,
                    title: format!("Issue {index}"),
                    ..Default::default()
                },
            )
            .unwrap();
            conn.execute(
                "UPDATE issues SET updated_at=?1 WHERE id=?2",
                rusqlite::params![format!("2025-01-{index:02}"), item.id],
            )
            .unwrap();
            expected.insert(0, item);
        }
    }
    let rows = load(
        &db,
        &Some(identity.clone()),
        identity.user.id,
        project,
        Section::Issues,
    )
    .unwrap();
    assert_eq!(rows.len(), 5);
    assert_eq!(
        rows.iter()
            .map(|row| row.label.as_str())
            .collect::<Vec<_>>(),
        ["Issue 6", "Issue 5", "Issue 4", "Issue 3", "Issue 2"]
    );
    let mut supplied = expected.clone();
    supplied.reverse();
    let projected = issue_rows("LIF", supplied);
    assert_eq!(projected.len(), 5);
    assert_eq!(projected[0].href, "/LIF/issues/LIF-1");
    assert_eq!(projected[0].label, "Issue 1");
    assert_eq!(projected[0].identifier.as_deref(), Some("LIF-1"));
    assert_eq!(rows[0].identifier.as_ref(), Some(&expected[0].identifier));
    assert!(
        rows[0]
            .href
            .ends_with(&format!("/issues/{}", expected[0].identifier))
    );
}
#[test]
fn modules_retain_stable_update_ordering_and_limit_the_sidebar_to_five_names() {
    let (db, identity, project) = fixture();
    let mut items = Vec::new();
    {
        let conn = db.write().unwrap();
        // This isolated read fixture needs historical timestamps. The normal
        // modules_updated trigger overwrites every UPDATE with now; use the
        // same pinning convention as the DB's timestamp-ordering tests.
        conn.execute_batch("DROP TRIGGER modules_updated;").unwrap();
        for (index, date) in [
            (1, "2025-01-01"),
            (3, "2025-03-01"),
            (2, "2025-03-01"),
            (4, "2025-02-01"),
            (5, "2025-01-05"),
            (6, "2025-01-04"),
        ] {
            let item = queries::create_module(
                &conn,
                &CreateModule {
                    project_id: project,
                    name: format!("Module {index}"),
                    description: String::new(),
                    status: "active".into(),
                    emoji: None,
                },
            )
            .unwrap();
            conn.execute(
                "UPDATE modules SET updated_at=?1 WHERE id=?2",
                rusqlite::params![date, item.id],
            )
            .unwrap();
            items.push(item);
        }
    }
    // The translated adapter assertion uses the supplied server order 3,2.
    let conn = db.read().unwrap();
    let mut supplied = Vec::new();
    for item in &items {
        let mut item = item.clone();
        item.updated_at = conn
            .query_row(
                "SELECT updated_at FROM modules WHERE id=?1",
                [item.id],
                |r| r.get(0),
            )
            .unwrap();
        supplied.push(item);
    }
    drop(conn);
    let projected = module_rows("LIF", supplied);
    assert_eq!(
        projected
            .iter()
            .map(|r| r.label.as_str())
            .collect::<Vec<_>>(),
        ["Module 3", "Module 2", "Module 4", "Module 5", "Module 6"]
    );
    assert_eq!(projected[0].href, format!("/LIF/modules/{}", items[1].id));
    // Real DB API name order is the tie source; stable sorting preserves it.
    let rows = load(
        &db,
        &Some(identity.clone()),
        identity.user.id,
        project,
        Section::Modules,
    )
    .unwrap();
    assert_eq!(
        rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
        ["Module 2", "Module 3", "Module 4", "Module 5", "Module 6"]
    );
}
#[test]
fn pages_combine_three_bounded_lifecycle_queries_by_updated_time_then_numeric_id() {
    let (db, identity, project) = fixture();
    let mut expected = Vec::new();
    {
        let conn = db.write().unwrap();
        for (status, count) in [
            ("draft", 6),
            ("active", 6),
            ("complete", 6),
            ("archived", 6),
        ] {
            for index in 0..count {
                let item = queries::create_page(
                    &conn,
                    &CreatePage {
                        project_id: Some(project),
                        title: format!("{status}{index}"),
                        status: status.into(),
                        ..Default::default()
                    },
                )
                .unwrap();
                conn.execute(
                    "UPDATE pages SET updated_at='2025-03-01',pinned=?1 WHERE id=?2",
                    rusqlite::params![i64::from(index == 0), item.id],
                )
                .unwrap();
                if status != "archived" {
                    expected.push(item.id);
                }
            }
        }
    }
    // Pinned priority does not replace the explicitly requested updated order.
    expected.sort_by(|a, b| b.cmp(a));
    expected.truncate(5);
    let rows = load(
        &db,
        &Some(identity.clone()),
        identity.user.id,
        project,
        Section::Pages,
    )
    .unwrap();
    assert_eq!(
        rows.iter()
            .map(|r| r.href.rsplit('/').next().unwrap().parse::<i64>().unwrap())
            .collect::<Vec<_>>(),
        expected
    );
    let conn = db.read().unwrap();
    let results = ["draft", "active", "complete"].map(|status| {
        queries::list_pages(
            &conn,
            Some(project),
            None,
            None,
            Some(status),
            Some("updated"),
            Some("desc"),
            Some(5),
            None,
        )
        .map_err(ReadFailure::from)
    });
    assert!(results.iter().all(|r| r.as_ref().unwrap().len() == 5));
    let template = results[0].as_ref().unwrap()[0].clone();
    assert_eq!(
        rows,
        page_rows(
            &queries::get_project(&conn, project).unwrap().identifier,
            results
        )
        .unwrap()
    );
    let resource = |id, date: &str, status: &str| {
        let mut row = template.clone();
        row.id = id;
        row.updated_at = date.into();
        row.status = status.into();
        row
    };
    let projected = page_rows(
        "LIF",
        [
            Ok(vec![
                resource(1, "2025-01-01", "draft"),
                resource(7, "2025-03-01", "draft"),
            ]),
            Ok(vec![
                resource(3, "2025-03-01", "active"),
                resource(4, "2025-02-01", "active"),
            ]),
            Ok(vec![
                resource(9, "2025-03-01", "complete"),
                resource(6, "2025-01-04", "complete"),
            ]),
        ],
    )
    .unwrap();
    assert_eq!(
        projected
            .iter()
            .map(|row| row.href.as_str())
            .collect::<Vec<_>>(),
        [
            "/LIF/pages/9",
            "/LIF/pages/7",
            "/LIF/pages/3",
            "/LIF/pages/4",
            "/LIF/pages/6"
        ]
    );
}
#[test]
fn plans_preserve_server_order_and_over_fetch_ten_candidates_before_excluding_archived() {
    let (db, identity, project) = fixture();
    let mut plans = Vec::new();
    {
        let conn = db.write().unwrap();
        for index in 1..=12 {
            let plan = queries::plans::create_plan(
                &conn,
                &CreatePlan {
                    project_id: project,
                    title: format!("Plan {index}"),
                    issue_id: None,
                    steps: vec![],
                },
            )
            .unwrap();
            conn.execute(
                "UPDATE plans SET updated_at=?1,status=?2 WHERE id=?3",
                rusqlite::params![
                    format!("2025-01-{index:02}"),
                    if index >= 7 { "archived" } else { "active" },
                    plan.id
                ],
            )
            .unwrap();
            plans.push(plan);
        }
    }
    // Ten candidates include six archived, so only four remain. Fetching all
    // plans or filtering archived in SQL would improperly return five.
    let rows = load(
        &db,
        &Some(identity.clone()),
        identity.user.id,
        project,
        Section::Plans,
    )
    .unwrap();
    assert_eq!(
        rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
        ["Plan 6", "Plan 5", "Plan 4", "Plan 3"]
    );
    let mut supplied = vec![plans[7].clone()];
    supplied[0].status = "archived".into();
    for index in [2, 4, 3, 1, 7, 6] {
        supplied.push(plans[index - 1].clone());
    }
    let projected = plan_rows("LIF", supplied);
    assert_eq!(
        projected
            .iter()
            .map(|r| r.href.as_str())
            .collect::<Vec<_>>(),
        [2, 4, 3, 1, 7].map(|id| format!("/LIF/plans/{}", plans[id - 1].id))
    );
}
#[test]
fn reads_recheck_owner_active_account_membership_and_stale_admin_role() {
    let (db, identity, project) = fixture();
    let owner = identity.user.id;
    assert!(
        load(
            &db,
            &Some(identity.clone()),
            owner + 1,
            project,
            Section::Issues
        )
        .unwrap_err()
        .access
    );
    assert!(
        load(&db, &None, owner, project, Section::Issues)
            .unwrap_err()
            .access
    );
    queries::members::remove_member(&db.write().unwrap(), project, owner).unwrap();
    assert!(
        load(
            &db,
            &Some(identity.clone()),
            owner,
            project,
            Section::Issues
        )
        .unwrap_err()
        .access
    );
    db.write()
        .unwrap()
        .execute("UPDATE users SET is_active=0 WHERE id=?1", [owner])
        .unwrap();
    assert!(
        load(&db, &Some(identity), owner, project, Section::Issues)
            .unwrap_err()
            .access
    );
    let (db, admin, _, _, _, _, project) = crate::api::test_helpers::setup_membership_test();
    let identity = crate::auth::fresh_identity(&admin, Transport::Web);
    // setup_membership_test's admin has no membership: its initial read must
    // rely on the admin bypass, then the stale identity must lose that bypass.
    assert!(
        queries::members::get_member_role(&db.read().unwrap(), project, admin.id)
            .unwrap()
            .is_none()
    );
    assert!(
        load(
            &db,
            &Some(identity.clone()),
            admin.id,
            project,
            Section::Issues
        )
        .is_ok()
    );
    db.write()
        .unwrap()
        .execute("UPDATE users SET is_admin=0 WHERE id=?1", [admin.id])
        .unwrap();
    assert!(
        load(&db, &Some(identity), admin.id, project, Section::Issues)
            .unwrap_err()
            .access
    );
}
