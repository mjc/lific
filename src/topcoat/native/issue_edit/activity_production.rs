//! Activity acceptance through the genuine private route and browser engine.

use super::super::home_fixture;
use crate::db::{
    models::{Priority, Role, UpdateIssue},
    queries,
};

const TITLE: &str = "Activity history title 4";
const DESCRIPTION: &str =
    "# Actual activity description\n\nExact old/new body, with author spaces  \n";
const TIMESTAMP: &str = "2026-10-03 15:59:20";
const CLOCK: &str = "2026-10-03T16:00:00Z";

#[tokio::test]
async fn native_issue_activity_history_expands_and_preserves_long_change_state() {
    browser("history").await;
}

#[tokio::test]
async fn native_issue_activity_clock_ticks_hides_and_disposes_with_real_owner() {
    browser("clock").await;
}

async fn browser(scenario: &str) {
    let fixture = home_fixture::fixture();
    let ids = {
        let conn = fixture.db.write().unwrap();
        let actor = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let issue = queries::get_issue(&conn, id).unwrap();
        queries::members::upsert_member(&conn, issue.project_id, actor.id, Role::Maintainer)
            .unwrap();
        // One genuine creation and three field audits, then four title audits.
        queries::update_issue(
            &conn,
            id,
            &UpdateIssue {
                title: Some("Activity history initial title".into()),
                description: Some(DESCRIPTION.into()),
                priority: Some(Priority::Medium),
                ..Default::default()
            },
        )
        .unwrap();
        for index in 1..=4 {
            queries::update_issue(
                &conn,
                id,
                &UpdateIssue {
                    title: Some(format!("Activity history title {index}")),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        // Control only the real audit clock input, never manufacture audit rows.
        assert_eq!(
            conn.execute(
                "UPDATE audit_log SET ts=?1 WHERE issue_id=?2",
                rusqlite::params![TIMESTAMP, id]
            )
            .unwrap(),
            8
        );
        let items = queries::activity::list_activity(
            &conn,
            queries::activity::ActivityScope::Issue(id),
            None,
            None,
        )
        .unwrap()
        .items;
        assert_eq!(items.len(), 8);
        items.into_iter().map(|item| item.id).collect::<Vec<_>>()
    };
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/issue_edit/activity.browser.test.cjs"
        ),
        &origin,
        &fixture.token,
    );
    command
        .arg(
            serde_json::json!({
                "identifier":"ACC-1", "title":TITLE, "description":DESCRIPTION,
                "history_ids":ids, "timestamp":TIMESTAMP, "clock_time":CLOCK,
            })
            .to_string(),
        )
        .arg(scenario);
    let result = tokio::time::timeout(std::time::Duration::from_secs(180), command.output()).await;
    server.abort();
    let output = result
        .expect("Activity production browser timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
