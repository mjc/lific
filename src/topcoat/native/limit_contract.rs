//! Test-only routes exercise the production app context and REST budgets together.

#![cfg(test)]

use std::{net::SocketAddr, sync::Arc};

use axum::{
    body::Body as AxumBody,
    http::{Request, StatusCode},
};
use topcoat::{
    context::{Cx, app_context},
    router::{Body, response::Response, route},
};
use tower::ServiceExt;

use super::home_fixture::{Fixture, fixture};
use crate::{
    api::{AttachmentConfig, AttachmentUploadLimiter},
    ratelimit::RateLimiter,
};

const LOGIN_IDENTITY: &str = "shared_limit_probe";
const LOGIN_KEY: &str = "login_id:shared_limit_probe";
const MULTIPART_TYPE: &str = "multipart/form-data; boundary=native-limit-contract";
const EMPTY_MULTIPART: &str = "--native-limit-contract--\r\n";

fn admission_response(admitted: bool) -> topcoat::Result<Response> {
    Ok(Response::builder()
        .status(if admitted { 200 } else { 429 })
        .body(Body::from(if admitted { "admitted" } else { "refused" }))?)
}

#[route(GET "/__native_limit_auth")]
async fn native_limit_auth(cx: &Cx) -> topcoat::Result<Response> {
    let caller = super::session::read(cx, super::context::caller(cx))?;
    super::session::read(cx, crate::api::require_user(&caller.identity))?;
    let limiter = app_context::<Arc<RateLimiter>>(cx);
    admission_response(limiter.check(LOGIN_KEY))
}

#[route(GET "/__native_limit_upload")]
async fn native_limit_upload(cx: &Cx) -> topcoat::Result<Response> {
    let caller = super::session::read(cx, super::context::caller(cx))?;
    let user = super::session::read(cx, crate::api::require_user(&caller.identity))?;
    let limiter = app_context::<Arc<AttachmentUploadLimiter>>(cx);
    admission_response(limiter.0.check(&format!("user:{}", user.id)))
}

#[route(GET "/__native_limit_upload_config")]
async fn native_limit_upload_config(cx: &Cx) -> topcoat::Result<Response> {
    let caller = super::session::read(cx, super::context::caller(cx))?;
    super::session::read(cx, crate::api::require_user(&caller.identity))?;
    let config = app_context::<AttachmentConfig>(cx);
    Ok(Response::builder().body(Body::from(config.max_bytes.to_string()))?)
}

async fn request(
    fixture: &Fixture,
    method: &str,
    path: &str,
    content_type: Option<&str>,
    body: impl Into<AxumBody>,
) -> axum::response::Response {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("cookie", format!("lific_token={}", fixture.token));
    if path.starts_with("/api/") {
        request = request.header("authorization", format!("Bearer {}", fixture.token));
    }
    if let Some(content_type) = content_type {
        request = request.header("content-type", content_type);
    }
    let mut request = request.body(body.into()).unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<SocketAddr>().unwrap(),
    ));
    fixture.app.clone().oneshot(request).await.unwrap()
}

async fn native_check(fixture: &Fixture, path: &str) -> StatusCode {
    request(fixture, "GET", path, None, AxumBody::empty())
        .await
        .status()
}

async fn body(response: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn login(fixture: &Fixture, password: &str) -> axum::response::Response {
    request(
        fixture,
        "POST",
        "/api/auth/login",
        Some("application/json"),
        serde_json::json!({ "identity": LOGIN_IDENTITY, "password": password }).to_string(),
    )
    .await
}

async fn upload(fixture: &Fixture) -> axum::response::Response {
    request(
        fixture,
        "POST",
        "/api/attachments",
        Some(MULTIPART_TYPE),
        EMPTY_MULTIPART,
    )
    .await
}

#[tokio::test]
async fn native_auth_budget_is_the_rest_login_budget() {
    let fixture = fixture();
    for _ in 0..5 {
        assert_eq!(
            native_check(&fixture, "/__native_limit_auth").await,
            StatusCode::OK
        );
    }
    assert_eq!(
        native_check(&fixture, "/__native_limit_auth").await,
        StatusCode::TOO_MANY_REQUESTS
    );
    let response = login(&fixture, "unused").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let error: serde_json::Value = serde_json::from_str(&body(response).await).unwrap();
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .starts_with("too many login attempts — try again in "),
        "{error}"
    );
}

#[tokio::test]
async fn rest_login_spending_is_seen_by_the_native_auth_handle() {
    let fixture = fixture();
    // Login reserves budget before rejecting oversized passwords, avoiding Argon2 here.
    let password = "x".repeat(1025);
    for _ in 0..5 {
        let response = login(&fixture, &password).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(body(response).await.contains("password"));
    }
    assert_eq!(
        native_check(&fixture, "/__native_limit_auth").await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn native_upload_budget_is_the_rest_upload_budget() {
    let fixture = fixture();
    for _ in 0..30 {
        assert_eq!(
            native_check(&fixture, "/__native_limit_upload").await,
            StatusCode::OK
        );
    }
    assert_eq!(
        native_check(&fixture, "/__native_limit_upload").await,
        StatusCode::TOO_MANY_REQUESTS
    );
    let response = upload(&fixture).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let error: serde_json::Value = serde_json::from_str(&body(response).await).unwrap();
    assert_eq!(
        error["error"],
        "upload rate limit exceeded — try again shortly"
    );
}

#[tokio::test]
async fn rest_upload_spending_is_seen_by_the_native_upload_handle() {
    let fixture = fixture();
    // The real handler spends budget before rejecting multipart without a file.
    for _ in 0..30 {
        let response = upload(&fixture).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let error: serde_json::Value = serde_json::from_str(&body(response).await).unwrap();
        assert_eq!(error["error"], "no 'file' field in upload");
    }
    assert_eq!(
        native_check(&fixture, "/__native_limit_upload").await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn native_upload_reads_the_production_attachment_config() {
    let fixture = fixture();
    let response = request(
        &fixture,
        "GET",
        "/__native_limit_upload_config",
        None,
        AxumBody::empty(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body(response).await.parse::<usize>().unwrap(),
        AttachmentConfig::default().max_bytes
    );
}
