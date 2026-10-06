//! Native and pinned original issue pages use the same production database.
use super::super::home_fixture;
use crate::db::{
    models::{Priority, Role, UpdateIssue},
    queries,
};

#[tokio::test]
async fn native_issue_detail_matches_pinned_master_geometry_and_phone_properties() {
    compare_document(false).await;
}

#[tokio::test]
async fn native_issue_detail_matches_pinned_master_computed_typography() {
    compare_document(true).await;
}

async fn compare_document(typography_only: bool) {
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let actor = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let issue = queries::get_issue(&conn, id).unwrap();
        queries::members::upsert_member(&conn, issue.project_id, actor.id, Role::Maintainer)
            .unwrap();
        queries::update_issue(
            &conn,
            id,
            &UpdateIssue {
                title: Some("Production issue initial title".into()),
                description: Some("# Production markdown\n\nExact initial description.".into()),
                priority: Some(Priority::Medium),
                ..Default::default()
            },
        )
        .unwrap();
    }
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/issue_edit/detail.geometry.test.cjs"
        ),
        &origin,
        &fixture.token,
    );
    command.arg(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/topcoat/native/browser_fixture.cjs"),
    );
    command.arg(std::env::var_os("LIFIC_SVELTE_SNAPSHOT").expect("Pinned master required"));
    if typography_only {
        command.arg("--typography-only");
    }
    let result = tokio::time::timeout(std::time::Duration::from_secs(180), command.output()).await;
    server.abort();
    let output = result
        .expect("Issue composition browser timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
