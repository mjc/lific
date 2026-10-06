//! Issue-reference search through the actual production Home dialog.

use super::home_fixture::{self, Fixture};
use crate::db::{
    models::{CreateIssue, CreateProject, Role, Status},
    queries,
};

fn fixture() -> (Fixture, String) {
    let fixture = home_fixture::fixture();
    let other_token = {
        let conn = fixture.db.write().unwrap();
        let viewer = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let first_id: i64 = conn
            .query_row(
                "SELECT id FROM projects WHERE identifier = 'ACC'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let hidden_id: i64 = conn
            .query_row(
                "SELECT id FROM projects WHERE identifier = 'HIDE'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let other_id: i64 = conn
            .query_row(
                "SELECT id FROM users WHERE username = 'non_member'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let second = queries::create_project(
            &conn,
            &CreateProject {
                identifier: "SEC".into(),
                name: "Second visible project".into(),
                ..Default::default()
            },
        )
        .unwrap();
        queries::members::upsert_member(&conn, second.id, viewer.id, Role::Viewer).unwrap();
        queries::members::upsert_member(&conn, hidden_id, other_id, Role::Viewer).unwrap();
        queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: second.id,
                title: "Second visible reference".into(),
                status: Status::Active,
                ..Default::default()
            },
        )
        .unwrap();
        let visible = Some([first_id, second.id].into_iter().collect());
        queries::reorder_projects(&conn, viewer.id, &[second.id, first_id], &visible).unwrap();
        queries::users::create_session(&conn, other_id, None)
            .unwrap()
            .token
    };
    (fixture, other_token)
}

async fn browser(scenario: &str) {
    let (fixture, other_token) = fixture();
    let (origin, task) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        "src/topcoat/native/palette_reference.browser.test.cjs",
        &origin,
        &fixture.token,
    );
    command.arg(scenario).arg(other_token);
    let result = tokio::time::timeout(std::time::Duration::from_secs(180), command.output()).await;
    task.abort();
    let output = result
        .unwrap_or_else(|_| panic!("native palette reference {scenario} browser timed out"))
        .unwrap();
    assert!(
        output.status.success(),
        "native palette reference {scenario}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[tokio::test]
async fn native_palette_reference_production_resolves_variants_and_bare_keyboard_selection() {
    browser("references").await;
}

#[tokio::test]
async fn native_palette_reference_production_omits_hidden_missing_and_page_references() {
    browser("denied").await;
}

#[tokio::test]
async fn native_palette_reference_production_escape_clears_query_before_reopening() {
    browser("escape").await;
}

#[tokio::test]
async fn native_palette_reference_production_rechecks_cookie_owner_and_session_denial() {
    browser("cookie").await;
}

#[tokio::test]
async fn native_palette_reference_modifier_enter_opens_new_tab_without_replacing_home() {
    browser("modified-ready").await;
}

#[tokio::test]
async fn native_palette_reference_modifier_enter_keeps_new_tab_intent_until_actual_result() {
    browser("modified-pending").await;
}

#[tokio::test]
async fn native_palette_reference_late_actual_results_cannot_replace_query_or_closed_palette() {
    browser("stale").await;
}

#[tokio::test]
async fn native_palette_reference_disposed_account_cannot_consume_pending_modified_enter() {
    browser("disposed").await;
}
