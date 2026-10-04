//! Real server integration for the native context and framework transport.

use std::{net::SocketAddr, sync::Arc};

use topcoat::runtime::{RouterBuilderRuntimeExt, Surrogated};

use super::probe::{self, ProbeState};
use crate::{
    config::Config,
    db::{
        self,
        models::{CreateIssue, CreateProject, UpdateIssue},
        queries,
    },
    ratelimit::IpNetwork,
    realtime::RealtimeHub,
    server::{build_app_with_store_and_frontend, topcoat_app},
    storage::AttachmentStore,
};

struct Fixture {
    origin: String,
    token: String,
    db: db::DbPool,
    issue_id: i64,
    seq: i64,
    state: Arc<ProbeState>,
    task: tokio::task::JoinHandle<()>,
    _store: tempfile::TempDir,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn deploy() -> Fixture {
    let db = db::open_memory().unwrap();
    let (token, issue_id, seq) = {
        let conn = db.write().unwrap();
        conn.execute(
            "INSERT INTO users (username, email, password_hash, is_admin, is_bot)
             VALUES ('native_owner', 'native@test.local', 'fixture', 1, 0)",
            [],
        )
        .unwrap();
        let project = queries::create_project(
            &conn,
            &CreateProject {
                name: "Native fixture".into(),
                identifier: "ACC".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let issue = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: project.id,
                title: "Native initial title".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let token = queries::users::create_session(&conn, 1, None)
            .unwrap()
            .token;
        (token, issue.id, issue.seq)
    };
    let state = Arc::new(ProbeState::new(issue_id));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut cfg = Config::default();
    cfg.auth.required = true;
    cfg.server.host = "127.0.0.1".into();
    cfg.server.port = address.port();
    let proxies: Arc<[IpNetwork]> = vec![IpNetwork::parse("127.0.0.1").unwrap()].into();
    let store = tempfile::tempdir().unwrap();
    let app = build_app_with_store_and_frontend(
        &cfg,
        db.clone(),
        RealtimeHub::new(),
        proxies,
        AttachmentStore::new(store.path().to_owned()),
        probe::router_builder()
            .route(topcoat_app::runtime_script)
            .runtime()
            .app_context(state.clone()),
    );
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    Fixture {
        origin: format!("http://{address}"),
        token,
        db,
        issue_id,
        seq,
        state,
        task,
        _store: store,
    }
}

async fn save(
    fixture: &Fixture,
    title: &str,
    seq: i64,
    token: Option<&str>,
    origin: &str,
) -> reqwest::Response {
    let request = reqwest::Client::new()
        .post(format!("{}/__native_probe/save", fixture.origin))
        .header("origin", origin)
        .json(&(title.to_owned(), seq).into_surrogate());
    let request = match token {
        Some(token) => request.header("cookie", format!("lific_token={token}")),
        None => request,
    };
    request.send().await.unwrap()
}

#[tokio::test]
async fn native_assembled_initial_html_and_procedure_share_the_real_session_and_store() {
    let fixture = deploy().await;
    let response = reqwest::Client::new()
        .get(format!("{}/ACC/__native_probe", fixture.origin))
        .header("cookie", format!("lific_token={}", fixture.token))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(response.headers()["x-frame-options"], "DENY");
    let html = response.text().await.unwrap();
    assert!(html.contains("Native initial title"));
    assert!(!html.contains("__topcoat-session.js"));
    let response = save(
        &fixture,
        "Native persisted title",
        fixture.seq,
        Some(&fixture.token),
        &fixture.origin,
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let outcome: serde_json::Value = response.json().await.unwrap();
    assert_eq!(outcome[0]["ok"], "Native persisted title");
    assert_eq!(outcome[2]["v"]["v"], "1");
    assert_eq!(outcome[4], "web");
    assert_eq!(outcome[5]["v"], "1");
    let issue = queries::get_issue(&fixture.db.read().unwrap(), fixture.issue_id).unwrap();
    assert_eq!(issue.title, "Native persisted title");
    assert!(issue.seq > fixture.seq);
    assert_eq!(outcome[1]["v"]["v"], issue.seq.to_string());
}

#[tokio::test]
async fn native_assembled_conflicts_and_revoked_sessions_are_inspectable_without_writes() {
    let fixture = deploy().await;
    let current = fixture
        .db
        .transaction(|conn| {
            queries::update_issue(
                conn,
                fixture.issue_id,
                &UpdateIssue {
                    title: Some("Other writer".into()),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let response = save(
        &fixture,
        "Stale draft",
        fixture.seq,
        Some(&fixture.token),
        &fixture.origin,
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let outcome: serde_json::Value = response.json().await.unwrap();
    assert_eq!(outcome[0]["err"], "conflict");
    assert_eq!(outcome[1]["v"]["v"], current.seq.to_string());
    assert_eq!(outcome[3]["v"], "Other writer");
    {
        let conn = fixture.db.write().unwrap();
        queries::users::delete_session(&conn, &fixture.token).unwrap();
        assert!(queries::users::validate_session(&conn, &fixture.token).is_err());
    }
    let response = save(
        &fixture,
        "Revoked draft",
        current.seq,
        Some(&fixture.token),
        &fixture.origin,
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let outcome: serde_json::Value = response.json().await.unwrap();
    assert_eq!(outcome[0]["err"], "reauth");
    let issue = queries::get_issue(&fixture.db.read().unwrap(), fixture.issue_id).unwrap();
    assert_eq!(issue.title, current.title);
    assert_eq!(issue.seq, current.seq);
    assert_eq!(
        fixture
            .state
            .calls
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[tokio::test]
async fn native_assembled_http_shard_renders_authorized_data() {
    let fixture = deploy().await;
    let response = reqwest::Client::new()
        .post(format!("{}/__native_probe/issue", fixture.origin))
        .header("accept", "application/x-ndjson")
        .header("origin", &fixture.origin)
        .header("cookie", format!("lific_token={}", fixture.token))
        .json(&serde_json::json!({"args": (0usize,).into_surrogate()}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("application/x-ndjson")
    );
    let body = response.text().await.unwrap();
    let frames: Vec<serde_json::Value> = body
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let snapshot = frames
        .iter()
        .find(|frame| frame["t"] == "snapshot")
        .expect("HTTP shard must return a framed snapshot");
    let html = snapshot["html"].as_str().unwrap();
    assert!(html.contains("Native initial title"));
    assert!(html.contains("native-probe-sequence"));
}

#[tokio::test]
async fn native_assembled_cross_origin_procedures_are_refused_before_the_action() {
    let fixture = deploy().await;
    let response = save(
        &fixture,
        "Cross-origin draft",
        fixture.seq,
        Some(&fixture.token),
        "https://other.example",
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
    assert_eq!(
        queries::get_issue(&fixture.db.read().unwrap(), fixture.issue_id)
            .unwrap()
            .title,
        "Native initial title"
    );
    assert_eq!(
        fixture
            .state
            .calls
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[tokio::test]
async fn native_assembled_cross_origin_socket_upgrades_are_refused() {
    use tokio_tungstenite::tungstenite::{Error, client::IntoClientRequest};

    let fixture = deploy().await;
    let url = format!(
        "{}/__native_probe/issue",
        fixture.origin.replacen("http://", "ws://", 1)
    );
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("origin", "https://other.example".parse().unwrap());
    request.headers_mut().insert(
        "cookie",
        format!("lific_token={}", fixture.token).parse().unwrap(),
    );
    request
        .headers_mut()
        .insert("sec-websocket-protocol", "topcoat-runtime".parse().unwrap());
    match tokio_tungstenite::connect_async(request).await {
        Err(Error::Http(response)) => assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN),
        result => panic!("cross-origin socket was not rejected with HTTP 403: {result:?}"),
    }
}

#[tokio::test]
async fn native_assembled_page_and_shard_classify_session_denials_and_missing_issues() {
    let fixture = deploy().await;
    let client = reqwest::Client::new();
    let page_url = format!("{}/ACC/__native_probe", fixture.origin);
    for cookie in [None, Some("lific_token=not-a-session")] {
        let request = client.get(&page_url);
        let request = match cookie {
            Some(cookie) => request.header("cookie", cookie),
            None => request,
        };
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
        assert!(
            !response
                .text()
                .await
                .unwrap()
                .contains("Native initial title")
        );
    }
    let response = client
        .post(format!("{}/__native_probe/issue", fixture.origin))
        .header("origin", &fixture.origin)
        .header("accept", "application/x-ndjson")
        .json(&serde_json::json!({"args": (0usize,).into_surrogate()}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    fixture
        .db
        .write()
        .unwrap()
        .execute("DELETE FROM issues WHERE id = ?1", [fixture.issue_id])
        .unwrap();
    let response = client
        .get(&page_url)
        .header("cookie", format!("lific_token={}", fixture.token))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn native_assembled_browser_page_save_shard_and_socket_never_call_rest() {
    let fixture = deploy().await;
    let mut command = super::home_fixture::browser_command(
        "src/topcoat/native/probe.browser.test.cjs",
        &fixture.origin,
        &fixture.token,
    );
    let output = tokio::time::timeout(std::time::Duration::from_secs(90), command.output())
        .await
        .expect("native browser test timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
