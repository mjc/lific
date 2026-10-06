//! Original independent project disclosures through real native Overview links.
use super::super::home_fixture;
use crate::db::{models::CreateProject, queries};
use std::time::Duration;

async fn browser(required: bool) {
    let fixture = home_fixture::fixture_with_auth(required);
    let (one, two, before) = {
        let conn = fixture.db.write().unwrap();
        let actor = queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        let one = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        conn.execute("UPDATE projects SET name='One' WHERE id=?1", [one])
            .unwrap();
        let two = queries::create_project(
            &conn,
            &CreateProject {
                identifier: "TWO".into(),
                name: "Two".into(),
                lead_user_id: Some(actor),
                ..Default::default()
            },
        )
        .unwrap()
        .id;
        (
            one,
            two,
            serde_json::to_value(queries::list_projects(&conn).unwrap()).unwrap(),
        )
    };
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/project_sidebar/disclosure.browser.test.cjs"
        ),
        &origin,
        &fixture.token,
    );
    command.arg(serde_json::json!({"one":one,"two":two,"auth_required":required}).to_string());
    let result = tokio::time::timeout(Duration::from_secs(120), command.output()).await;
    server.abort();
    let output = result
        .expect("Sidebar disclosure browser timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "Original independent disclosure assertions; auth required={required}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::to_value(queries::list_projects(&fixture.db.read().unwrap()).unwrap()).unwrap(),
        before,
        "Disclosure/navigation do not edit project records."
    );
}

#[tokio::test]
async fn native_sidebar_adapts_master_independent_disclosure_and_overview_navigation() {
    browser(true).await;
}

#[tokio::test]
async fn native_sidebar_adapts_master_independent_disclosure_and_overview_navigation_auth_optional()
{
    browser(false).await;
}
