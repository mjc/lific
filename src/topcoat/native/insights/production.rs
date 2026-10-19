use super::super::home_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn native_insights_initial_document_is_populated_and_hydrated() {
    let fixture = home_fixture::fixture();
    let response = fixture
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/ACC/insights")
                .header("cookie", format!("lific_token={}", fixture.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    for text in [
        "Created vs. closed",
        "last 12 weeks",
        "Backlog",
        "Urgent",
        "No module",
        "Top actors",
        "Issues created vs closed per week",
    ] {
        assert!(
            html.contains(text),
            "missing initial Insights content: {text}"
        );
    }
    assert!(
        html.contains("data-topcoat-bind:"),
        "initial hydration bindings missing"
    );
    assert!(!html.contains("/api/projects/"));
    assert!(!html.contains("Private hidden project"));
}

async fn document(
    fixture: &home_fixture::Fixture,
    path: &str,
    token: Option<&str>,
    prefix: &str,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let mut request = Request::builder()
        .uri(path)
        .header("x-forwarded-prefix", prefix);
    if let Some(token) = token {
        request = request.header("cookie", format!("lific_token={token}"));
    }
    let mut request = request.body(Body::empty()).unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = fixture.app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn native_insights_auth_mount_empty_state_and_revocation_keep_private_metrics_absent() {
    let fixture = home_fixture::fixture();
    let (status, headers, _) = document(&fixture, "/ACC/insights", None, "/app").await;
    assert!(status.is_redirection());
    assert_eq!(headers["location"], "/app/login");
    let (_, _, hidden) = document(&fixture, "/HIDE/insights", Some(&fixture.token), "").await;
    assert!(hidden.contains("Couldn't load insights"));
    assert!(!hidden.contains("Created vs. closed"));
    assert!(!hidden.contains("Private hidden project"));
    {
        let conn = fixture.db.write().unwrap();
        conn.execute("UPDATE issues SET deleted_at=datetime('now') WHERE project_id=(SELECT id FROM projects WHERE identifier='ACC')",[]).unwrap();
    }
    let (status, _, empty) =
        document(&fixture, "/ACC/insights", Some(&fixture.token), "/app").await;
    assert_eq!(status, StatusCode::OK);
    assert!(empty.contains("Nothing to chart yet"));
    assert!(empty.contains("/app/__native_home/mascot.png"));
    assert!(empty.contains("/app/ACC/overview"));
    assert!(!empty.contains("Created vs. closed"));
    {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("DELETE FROM project_members WHERE user_id=?1", [user.id])
            .unwrap();
    }
    let (_, _, revoked) = document(&fixture, "/ACC/insights", Some(&fixture.token), "").await;
    assert!(revoked.contains("Couldn't load insights"));
    assert!(!revoked.contains("Created vs. closed"));
}

#[tokio::test]
async fn native_insights_real_browser_week_selection_hover_and_pinned_main_geometry() {
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        let project = crate::db::queries::create_project(
            &conn,
            &crate::db::models::CreateProject {
                identifier: "EMP".into(),
                name: "Empty project".into(),
                ..Default::default()
            },
        )
        .unwrap();
        conn.execute(
            "INSERT INTO project_members(project_id,user_id,role) VALUES(?1,?2,'viewer')",
            rusqlite::params![project.id, user.id],
        )
        .unwrap();
    }
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        "src/topcoat/native/insights/insights.browser.test.cjs",
        &origin,
        &fixture.token,
    );
    command.arg(std::env::var_os("LIFIC_SVELTE_SNAPSHOT").unwrap_or_default());
    let output = tokio::time::timeout(std::time::Duration::from_secs(180), command.output()).await;
    server.abort();
    let output = output.expect("Insights browser timed out").unwrap();
    assert!(
        output.status.success(),
        "Insights browser:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn native_insights_navigation_admission_reuses_the_shared_document_owner() {
    let fixture = home_fixture::fixture();
    let (_, _, initial) = document(&fixture, "/ACC/insights", Some(&fixture.token), "").await;
    let signals = home_fixture::page_signals(&initial);
    let mut request = Request::builder()
        .method("POST")
        .uri("/ACC/insights")
        .header("origin", "http://127.0.0.1:3000")
        .header("host", "127.0.0.1:3000")
        .header("cookie", format!("lific_token={}", fixture.token))
        .header("content-type", "application/json")
        .header("x-topcoat-runtime", "true")
        .header("accept", "application/x-ndjson")
        .body(Body::from(
            serde_json::json!({ "signals": signals }).to_string(),
        ))
        .unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = fixture.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let frames = String::from_utf8(body.to_vec()).unwrap();
    let snapshot = frames
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|frame| frame["t"] == "snapshot")
        .expect("actual Topcoat destination snapshot");
    assert!(snapshot["html"].as_str().unwrap().contains("ACC  Insights"));
}

// Test-only control of genuine SQLite failures and membership, not a product API.
#[topcoat::runtime::procedure("/__native_insights_test/fault")]
async fn native_insights_test_fault(
    cx: &topcoat::context::Cx,
    action: String,
) -> topcoat::Result<bool> {
    let caller = super::super::context::caller(cx)?;
    let user = crate::api::require_user(&caller.identity)?;
    let conn = super::super::context::db(cx).write()?;
    match action.as_str() {
        "transient" => {
            conn.execute(
                "ALTER TABLE audit_log RENAME TO insights_held_audit_log",
                [],
            )?;
        }
        "restore" => {
            conn.execute(
                "ALTER TABLE insights_held_audit_log RENAME TO audit_log",
                [],
            )?;
        }
        "revoke" => {
            conn.execute("DELETE FROM project_members WHERE project_id=(SELECT id FROM projects WHERE identifier='ACC') AND user_id=?1",[user.id])?;
        }
        _ => return Err(topcoat::router::error::bad_request("invalid fixture action").into()),
    }
    Ok(true)
}
