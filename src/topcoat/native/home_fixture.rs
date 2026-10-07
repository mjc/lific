//! Shared authenticated production-server fixtures for native page tests.

use crate::{
    config::Config,
    db::{
        self,
        models::{CreateIssue, CreateProject, Status},
        queries,
    },
    ratelimit::IpNetwork,
    realtime::RealtimeHub,
    server::{build_app_with_store_and_frontend, topcoat_app},
    storage::AttachmentStore,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use std::{net::SocketAddr, sync::Arc};
use tower::ServiceExt;

#[derive(Clone, Default)]
pub(crate) struct HomeSnapshotReads(Arc<std::sync::atomic::AtomicUsize>);

impl HomeSnapshotReads {
    pub(crate) fn record(&self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn count(&self) -> usize {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

pub(crate) struct Fixture {
    pub(crate) app: Router,
    pub(crate) db: db::DbPool,
    pub(crate) token: String,
    pub(crate) realtime: RealtimeHub,
    pub(crate) home_snapshot_reads: HomeSnapshotReads,
    pub(crate) sidebar_writes: super::project_sidebar::SidebarWriteStore,
    _store: tempfile::TempDir,
}

pub(super) async fn document(
    fixture: &Fixture,
    mount: &str,
    path: &str,
    authenticated: bool,
    signals: Option<serde_json::Map<String, serde_json::Value>>,
) -> (StatusCode, String) {
    let mut request = Request::builder()
        .method(if signals.is_some() { "POST" } else { "GET" })
        .uri(format!("{mount}{path}"))
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("x-forwarded-prefix", mount);
    if authenticated {
        request = request.header("cookie", format!("lific_token={}", fixture.token));
    }
    let runtime = signals.is_some();
    let body = if let Some(signals) = signals {
        request = request
            .header("content-type", "application/json")
            .header("x-topcoat-runtime", "true")
            .header("accept", "application/x-ndjson");
        Body::from(serde_json::json!({"signals": signals}).to_string())
    } else {
        Body::empty()
    };
    let mut request = request.body(body).unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let app = if mount.is_empty() {
        fixture.app.clone()
    } else {
        super::admission_contract::mounted(fixture.app.clone())
    };
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    let html =
        if runtime && status == StatusCode::OK {
            body.lines().map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .find(|frame| frame["t"] == "snapshot")
            .unwrap_or_else(|| panic!("missing native snapshot at {mount}{path}: {body}"))
            ["html"].as_str().unwrap().to_owned()
        } else {
            body
        };
    (status, html)
}

pub(super) async fn procedure(
    fixture: &Fixture,
    path: &str,
    arguments: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("content-type", "application/json")
        .header("cookie", format!("lific_token={}", fixture.token))
        .body(Body::from(arguments.to_string()))
        .unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = fixture.app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        serde_json::Value::String(String::from_utf8_lossy(&bytes).into_owned())
    });
    (status, value)
}

pub(super) fn page_signals(html: &str) -> serde_json::Map<String, serde_json::Value> {
    let document = scraper::Html::parse_document(html);
    let mut signals = serde_json::Map::new();
    for node in document.tree.nodes() {
        if let scraper::Node::Comment(comment) = node.value()
            && let Some(value) = comment
                .strip_prefix("::topcoat::signal(")
                .and_then(|value| value.strip_suffix(')'))
        {
            let text = scraper::Html::parse_fragment(&value.replace('<', "&lt;"))
                .root_element()
                .text()
                .collect::<String>();
            let declaration: serde_json::Value = serde_json::from_str(&text).unwrap();
            signals.insert(
                declaration["id"].as_str().unwrap().to_owned(),
                declaration["v"].clone(),
            );
        }
    }
    signals
}

pub(crate) fn fixture() -> Fixture {
    fixture_with_auth(true)
}

pub(super) fn fixture_with_auth(required: bool) -> Fixture {
    let (db, _, _, _, viewer, _, project_id) = crate::api::test_helpers::setup_membership_test();
    let token = {
        let conn = db.write().unwrap();
        conn.execute(
            "UPDATE projects SET identifier = 'ACC', name = 'Visible project' WHERE id = ?1",
            [project_id],
        )
        .unwrap();
        let hidden = queries::create_project(
            &conn,
            &CreateProject {
                identifier: "HIDE".into(),
                name: "Private hidden project".into(),
                ..Default::default()
            },
        )
        .unwrap();
        for (project_id, status, title) in [
            (project_id, Status::Active, "Visible active initial work"),
            (project_id, Status::Todo, "Visible todo initial work"),
            (hidden.id, Status::Active, "Private hidden initial work"),
        ] {
            queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id,
                    title: title.into(),
                    status,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        queries::users::create_session(&conn, viewer.id, None)
            .unwrap()
            .token
    };
    let mut cfg = Config::default();
    cfg.auth.required = required;
    let store = tempfile::tempdir().unwrap();
    let proxies: Arc<[IpNetwork]> = vec![IpNetwork::parse("127.0.0.1").unwrap()].into();
    let realtime = RealtimeHub::new();
    let home_snapshot_reads = HomeSnapshotReads::default();
    let sidebar_writes = super::project_sidebar::SidebarWriteStore::default();
    let app = build_app_with_store_and_frontend(
        &cfg,
        db.clone(),
        realtime.clone(),
        proxies,
        AttachmentStore::new(store.path().to_owned()),
        topcoat_app::router_builder()
            .app_context(home_snapshot_reads.clone())
            .app_context(sidebar_writes.clone()),
    );
    Fixture {
        app,
        db,
        token,
        realtime,
        home_snapshot_reads,
        sidebar_writes,
        _store: store,
    }
}

pub(super) async fn serve(fixture: &Fixture) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = fixture.app.clone();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (format!("http://{address}"), task)
}

pub(crate) fn browser_command(script: &str, origin: &str, token: &str) -> tokio::process::Command {
    let mut command = if cfg!(windows) || std::env::var_os("PLAYWRIGHT_EXECUTABLE_PATH").is_some() {
        tokio::process::Command::new("node")
    } else {
        let mut command = tokio::process::Command::new("devenv");
        command.args(["--profile", "topcoat-e2e", "shell", "node"]);
        command
    };
    command.args([script, origin, token]);
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .kill_on_drop(true);
    command
}

#[tokio::test]
async fn native_browser_discovery_io_contract() {
    let output = browser_command("src/topcoat/native/browser_fixture.io.test.cjs", "", "")
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "browser discovery IO contracts failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[tokio::test]
async fn native_original_source_checkout_io_contract() {
    let output = browser_command(
        "src/topcoat/native/original_source_fixture.test.cjs",
        "",
        "",
    )
    .output()
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "original reference checkout contracts failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
