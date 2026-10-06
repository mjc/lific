//! Browser edge cases against the shared production Home fixture.

use super::home_fixture::{self, Fixture};
use crate::db::{
    models::{ListIssuesQuery, Status, UpdateIssue},
    queries,
};

async fn browser(fixture: &Fixture, scenario: &str) {
    let (origin, server) = home_fixture::serve(fixture).await;
    let mut command = home_fixture::browser_command(
        "src/topcoat/native/home_browser_edges.test.cjs",
        &origin,
        &fixture.token,
    );
    command.arg(scenario);
    let result = tokio::time::timeout(std::time::Duration::from_secs(150), command.output()).await;
    server.abort();
    let output = result
        .expect("production Home edge browser timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn native_home_browser_preloads_logo_without_external_icon_requests() {
    browser(&home_fixture::fixture(), "preloads").await;
}

#[tokio::test]
async fn native_home_browser_skip_link_focuses_main_at_every_mount() {
    browser(&home_fixture::fixture(), "accessibility").await;
}

#[tokio::test]
async fn native_home_browser_storage_denial_preserves_native_initialization_without_rest() {
    browser(&home_fixture::fixture(), "storage").await;
}

#[tokio::test]
async fn native_home_browser_quiet_work_decodes_the_actual_mounted_mascot() {
    let fixture = home_fixture::fixture();
    fixture
        .db
        .transaction(|conn| {
            let issues = queries::list_issues(conn, &ListIssuesQuery::default())?;
            assert!(
                !issues.is_empty(),
                "fixture must contain real work before closing it"
            );
            for issue in issues {
                queries::update_issue(
                    conn,
                    issue.id,
                    &UpdateIssue {
                        status: Some(Status::Done),
                        expected_seq: Some(issue.seq),
                        ..Default::default()
                    },
                )?;
            }
            for status in [Status::Active, Status::Todo] {
                assert!(
                    queries::list_issues(
                        conn,
                        &ListIssuesQuery {
                            status: Some(status),
                            ..Default::default()
                        }
                    )?
                    .is_empty(),
                    "quiet fixture still contains {status:?} work"
                );
            }
            Ok(())
        })
        .unwrap();
    browser(&fixture, "quiet").await;
}
