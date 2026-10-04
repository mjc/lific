//! Home is exercised through the same factory as the running server.

use std::{net::SocketAddr, sync::Arc};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

use crate::{
    config::Config,
    db::{
        self,
        models::{CreateIssue, CreateProject, Status},
        queries,
    },
    ratelimit::IpNetwork,
    realtime::RealtimeHub,
    server::build_app_with_store,
    storage::AttachmentStore,
};

struct Fixture {
    app: Router,
    db: db::DbPool,
    token: String,
    _store: tempfile::TempDir,
}

fn fixture() -> Fixture {
    fixture_with_auth(true)
}

fn fixture_with_auth(required: bool) -> Fixture {
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
    let app = build_app_with_store(
        &cfg,
        db.clone(),
        RealtimeHub::new(),
        proxies,
        AttachmentStore::new(store.path().to_owned()),
    );
    Fixture {
        app,
        db,
        token,
        _store: store,
    }
}

async fn get(
    fixture: &Fixture,
    cookie: Option<&str>,
    prefix: Option<&str>,
) -> axum::response::Response {
    let mut request = Request::builder().uri("/");
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    if let Some(prefix) = prefix {
        request = request.header("x-forwarded-prefix", prefix);
    }
    let mut request = request.body(Body::empty()).unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<SocketAddr>().unwrap(),
    ));
    fixture.app.clone().oneshot(request).await.unwrap()
}

#[tokio::test]
async fn native_home_production_initial_html_contains_visible_work_without_hybrid_scripts() {
    let fixture = fixture();
    for prefix in [None, Some("/app"), Some("/ACC")] {
        let response = get(
            &fixture,
            Some(&format!("lific_token={}", fixture.token)),
            prefix,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8(bytes.to_vec()).unwrap();
        for text in [
            "viewer",
            "Visible active initial work",
            "Visible todo initial work",
            "Visible project",
            "data-native-home",
            "My active issues",
        ] {
            assert!(html.contains(text), "missing {text}: {html}");
        }
        assert!(!html.contains("Private hidden"));
        assert!(!html.contains("Loading your dashboard"));
        let prefix = prefix.unwrap_or("");
        assert!(html.contains(&format!("href=\"{prefix}/ACC/issues/ACC-1\"")));
        assert!(html.contains(&format!("src=\"{prefix}/__topcoat-runtime.js\"")));
        for script in html.split("<script").skip(1) {
            let tag = script.split('>').next().unwrap();
            if tag.contains(" src=") {
                assert!(
                    tag.contains("/__topcoat-runtime.js"),
                    "Home loaded a legacy controller: {tag}"
                );
            }
        }
    }
}

#[tokio::test]
async fn native_home_production_guest_and_revoked_credentials_redirect_without_private_html() {
    let fixture = fixture();
    {
        let conn = fixture.db.write().unwrap();
        queries::users::delete_session(&conn, &fixture.token).unwrap();
        assert!(queries::users::validate_session(&conn, &fixture.token).is_err());
    }
    for prefix in [None, Some("/app"), Some("/ACC")] {
        for cookie in [
            None,
            Some("lific_token=invalid".to_owned()),
            Some(format!("lific_token={}", fixture.token)),
        ] {
            let response = get(&fixture, cookie.as_deref(), prefix).await;
            assert!(
                response.status().is_redirection(),
                "expected login redirect, got {}",
                response.status()
            );
            assert_eq!(
                response.headers()["location"],
                format!("{}/login", prefix.unwrap_or(""))
            );
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains("Visible active initial work"));
        }
    }
}

#[tokio::test]
async fn native_home_production_optional_auth_preserves_operator_and_rejects_bad_credentials() {
    let fixture = fixture_with_auth(false);
    let response = get(&fixture, None, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Welcome, admin"));
    assert!(html.contains("Private hidden initial work"));
    let response = get(&fixture, Some("lific_token=invalid"), None).await;
    assert!(response.status().is_redirection());
    assert_eq!(response.headers()["location"], "/login");
}

#[tokio::test]
async fn native_home_production_missing_identity_redirects_instead_of_rendering_an_error() {
    let db = db::open_memory().unwrap();
    let store = tempfile::tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.auth.required = false;
    let app = build_app_with_store(
        &cfg,
        db,
        RealtimeHub::new(),
        Arc::from([]),
        AttachmentStore::new(store.path().to_owned()),
    );
    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(
        response.status().is_redirection(),
        "missing identity must redirect, got {}",
        response.status()
    );
    assert_eq!(response.headers()["location"], "/login");
}

#[tokio::test]
async fn native_home_production_browser_reads_and_interacts_without_rest() {
    let fixture = fixture();
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
    let origin = format!("http://{address}");
    let script = "src/topcoat/native/home.browser.test.cjs";
    let mut command = if std::env::var_os("PLAYWRIGHT_EXECUTABLE_PATH").is_some() {
        let mut command = tokio::process::Command::new("node");
        command.args([script, &origin, &fixture.token]);
        command
    } else {
        let mut command = tokio::process::Command::new("devenv");
        command.args([
            "--profile",
            "topcoat-e2e",
            "shell",
            "bash",
            "--noprofile",
            "--norc",
            "-c",
            "node \"$1\" \"$2\" \"$3\" \"$4\"",
            "lific-native-home",
            script,
            &origin,
            &fixture.token,
        ]);
        command
    };
    // The existing visual capture option supplies the installed, pinned master.
    // Native browser coverage runs without this optional screenshot reference.
    if let Some(snapshot) = std::env::var_os("LIFIC_SVELTE_SNAPSHOT") {
        command.arg(snapshot);
    }
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .kill_on_drop(true);
    let result = tokio::time::timeout(std::time::Duration::from_secs(240), command.output()).await;
    task.abort();
    let output = result.expect("production Home browser timed out").unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
