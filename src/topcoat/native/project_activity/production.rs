use super::super::home_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn native_project_activity_initial_document_has_feed_actors_and_hydration() {
    let fixture = home_fixture::fixture();
    let response = fixture
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/ACC/activity")
                .header("cookie", format!("lific_token={}", fixture.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    for text in [
        "Activity",
        "Actors",
        "created issue",
        "ACC-1",
        "data-topcoat-bind:",
    ] {
        assert!(
            html.contains(text),
            "missing initial Activity content: {text}"
        );
    }
    assert!(!html.contains("/api/projects/"));
    assert!(!html.contains("Private hidden project"));
}

#[tokio::test]
async fn native_project_activity_real_browser_matches_main_and_keeps_loaded_history() {
    let fixture = super::browser_fixture::fixture();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        "src/topcoat/native/project_activity/activity.browser.test.cjs",
        &origin,
        &fixture.token,
    );
    command.arg(std::env::var_os("LIFIC_SVELTE_SNAPSHOT").unwrap_or_default());
    let output = tokio::time::timeout(std::time::Duration::from_secs(300), command.output()).await;
    server.abort();
    let output = output.expect("Project Activity browser timed out").unwrap();
    assert!(
        output.status.success(),
        "Activity browser:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn native_project_activity_auth_and_revocation_remove_private_feed() {
    let fixture = home_fixture::fixture();
    for (path, token) in [
        ("/ACC/activity", None),
        ("/HIDE/activity", Some(fixture.token.as_str())),
        ("/MISSING/activity", Some(fixture.token.as_str())),
    ] {
        let mut request = Request::builder().uri(path);
        if let Some(token) = token {
            request = request.header("cookie", format!("lific_token={token}"));
        }
        let response = fixture
            .app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let html = String::from_utf8(
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        if token.is_none() {
            assert!(status.is_redirection());
        } else {
            assert!(html.contains("Couldn't load activity"));
        }
        assert!(!html.contains("created issue"));
        assert!(!html.contains("Private hidden project"));
    }
    {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("DELETE FROM project_members WHERE user_id=?1", [user.id])
            .unwrap();
    }
    let response = fixture
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/ACC/activity")
                .header("cookie", format!("lific_token={}", fixture.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = String::from_utf8(
        axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(html.contains("Couldn't load activity"));
    assert!(!html.contains("created issue"));
}
