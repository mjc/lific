//! Home is exercised through the same factory as the running server.

use std::{net::SocketAddr, sync::Arc};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

use super::home_fixture::{self, Fixture, fixture, fixture_with_auth};

use crate::{
    config::Config,
    db::{self, queries},
    realtime::RealtimeHub,
    server::build_app_with_store,
    storage::AttachmentStore,
};

async fn get(
    fixture: &Fixture,
    cookie: Option<&str>,
    prefix: Option<&str>,
) -> axum::response::Response {
    get_path(fixture, "/", cookie, prefix).await
}

async fn get_path(
    fixture: &Fixture,
    path: &str,
    cookie: Option<&str>,
    prefix: Option<&str>,
) -> axum::response::Response {
    let mut request = Request::builder().uri(path);
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
async fn native_home_icons_use_selected_immutable_assets_at_every_mount() {
    let fixture = fixture();
    let cookie = format!("lific_token={}", fixture.token);
    for prefix in ["", "/app", "/ACC"] {
        let response = get(&fixture, Some(&cookie), Some(prefix)).await;
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = std::str::from_utf8(&body).unwrap();
        let href = html
            .split("<use href=\"")
            .nth(1)
            .expect("Home uses shared SVG geometry")
            .split('"')
            .next()
            .unwrap();
        assert!(href.starts_with(&format!("{prefix}/__native_icons/")));
        let path = href
            .strip_prefix(prefix)
            .unwrap()
            .split('#')
            .next()
            .unwrap();
        // Static, allowlisted geometry contains no private account data.
        let response = get_path(&fixture, path, None, Some(prefix)).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "image/svg+xml");
        assert_eq!(
            response.headers()["cache-control"],
            "public, max-age=31536000, immutable"
        );
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        let svg = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(
            svg.len() < 2_000,
            "Serve only the selected icon, not the catalog"
        );
        let svg = std::str::from_utf8(&svg).unwrap();
        assert!(svg.contains("xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains("id=\"icon\""));
        assert!(!svg.contains("<script") && !svg.contains("<use"));
        let base = path.rsplit_once('/').unwrap().0;
        for invalid in [
            format!("{base}/Unknown.svg"),
            "/__native_icons/stale/Circle.svg".to_owned(),
        ] {
            assert_eq!(
                get_path(&fixture, &invalid, None, Some(prefix))
                    .await
                    .status(),
                StatusCode::NOT_FOUND
            );
        }
    }
}

#[tokio::test]
async fn native_home_initial_html_serializes_the_sidebar_catalog_once() {
    let fixture = fixture();
    let cookie = format!("lific_token={}", fixture.token);
    let response = get(&fixture, Some(&cookie), None).await;
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    assert_eq!(
        html.matches("\\&quot;catalog\\&quot;").count(),
        1,
        "Only the retained model value should contain the catalog; stale mount checks use its revision"
    );
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
        assert!(html.contains(&format!(
            "src=\"{prefix}{}\"",
            super::super::assets::runtime_url()
        )));
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
async fn native_home_initial_mount_handlers_bind_shared_signal_handles_once() {
    let fixture = fixture();
    let cookie = format!("lific_token={}", fixture.token);
    let response = get(&fixture, Some(&cookie), None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    let document = scraper::Html::parse_document(html);
    let largest = document
        .select(&scraper::Selector::parse("*").unwrap())
        .filter_map(|element| element.value().attr("data-topcoat-on:mount"))
        .map(str::len)
        .max()
        .expect("Home installs native mount handlers");
    assert!(
        largest < 2_500,
        "Home must load shared generated handlers rather than inline their bodies: {largest} bytes"
    );
}

#[tokio::test]
async fn native_home_generated_handler_is_shared_immutable_and_free_of_account_data() {
    let fixture = fixture();
    let cookie = format!("lific_token={}", fixture.token);
    for filename in [
        "__native-home-shell.js",
        "__native-workspace.js",
        "__native-sidebar.js",
    ] {
        let mut first = None;
        for prefix in ["", "/app", "/ACC"] {
            let response = get(&fixture, Some(&cookie), Some(prefix)).await;
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let html = std::str::from_utf8(&body).unwrap();
            let document = scraper::Html::parse_document(html);
            let marker = format!("/{filename}?v=");
            let mount = document
                .select(&scraper::Selector::parse("*").unwrap())
                .filter_map(|element| element.value().attr("data-topcoat-on:mount"))
                .find(|handler| handler.contains(&marker))
                .expect("Home loads its generated handler from a fingerprinted shared asset");
            assert!(mount.len() < 8_000, "Home bootstrap: {} bytes", mount.len());
            let digest: String = mount
                .split(&marker)
                .nth(1)
                .unwrap()
                .chars()
                .take(64)
                .collect();
            assert_eq!(digest.len(), 64);
            assert!(
                digest
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
            );
            let path = format!("/{filename}?v={digest}");
            let response = get_path(&fixture, &path, None, Some(prefix)).await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.headers()["content-type"],
                "text/javascript; charset=utf-8"
            );
            assert_eq!(
                response.headers()["cache-control"],
                "public, max-age=31536000, immutable"
            );
            assert_eq!(response.headers()["x-content-type-options"], "nosniff");
            let asset = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let source = std::str::from_utf8(&asset).unwrap();
            assert!(source.contains("export"));
            for private in [
                fixture.token.as_str(),
                "Visible active initial work",
                "Private hidden",
                "Project 1",
            ] {
                assert!(
                    !source.contains(private),
                    "Shared handler leaked request data"
                );
            }
            if let Some((previous_path, previous_asset)) = &first {
                assert_eq!(&path, previous_path);
                assert_eq!(&asset, previous_asset);
            } else {
                first = Some((path, asset));
            }
            for invalid in [format!("/{filename}"), format!("/{filename}?v=stale")] {
                assert_eq!(
                    get_path(&fixture, &invalid, None, Some(prefix))
                        .await
                        .status(),
                    StatusCode::NOT_FOUND
                );
            }
        }
    }
}

#[tokio::test]
async fn native_home_runtime_loads_shared_handlers_before_hydration() {
    use sha2::{Digest, Sha256};
    let fixture = fixture();
    let url = super::super::assets::runtime_url();
    let response = get_path(&fixture, url, None, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let source = std::str::from_utf8(&body).unwrap();
    for (line, filename) in source.lines().take(3).zip([
        "__native-home-shell.js",
        "__native-workspace.js",
        "__native-sidebar.js",
    ]) {
        assert!(
            line.starts_with("import ") && line.contains(&format!("./{filename}?v=")),
            "Runtime must finish loading generated handlers before it hydrates controls: {line}"
        );
    }
    assert_eq!(
        url,
        format!("/__topcoat-runtime.js?v={:x}", Sha256::digest(&body))
    );
}

#[tokio::test]
async fn native_home_initial_html_defers_phone_chrome_and_closed_palette_result_mount() {
    let fixture = fixture();
    let cookie = format!("lific_token={}", fixture.token);
    let response = get(&fixture, Some(&cookie), None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    assert!(html.contains("id=\"native-home-mobile-open\""));
    assert!(
        !html.contains("<section data-native-mobile-nav="),
        "Unopened phone chrome must not serialize its dialog"
    );
    assert!(!html.contains("aria-label=\"Phone workspace\""));
    assert!(!html.contains("data-native-mobile-unavailable=\"\""));
    let document = scraper::Html::parse_document(html);
    let results = scraper::Selector::parse("nav.native-home-palette-results").unwrap();
    let palette_results = document.select(&results).next().unwrap();
    assert!(
        palette_results
            .value()
            .attr("data-topcoat-on:mount")
            .is_none(),
        "Closed palette results must not serialize their inactive result workflow"
    );
}

#[tokio::test]
async fn native_home_initial_html_keeps_navigation_handlers_shared_as_catalog_grows() {
    let fixture = fixture();
    let cookie = format!("lific_token={}", fixture.token);
    let response = get(&fixture, Some(&cookie), None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let initial = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    {
        let conn = fixture.db.write().unwrap();
        let owner = queries::users::validate_session(&conn, &fixture.token).unwrap();
        for number in 2..=45 {
            queries::create_project(
                &conn,
                &crate::db::models::CreateProject {
                    identifier: format!("P{number}"),
                    name: format!("Project {number}"),
                    lead_user_id: Some(owner.id),
                    ..Default::default()
                },
            )
            .unwrap();
        }
    }
    let response = get(&fixture, Some(&cookie), None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let expanded = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = std::str::from_utf8(&expanded).unwrap();
    for number in 2..=45 {
        assert!(html.contains(&format!("title=\"Project {number}\"")));
        assert!(
            !html.contains(&format!("aria-label=\"Open Project {number} navigation\"")),
            "Unopened phone navigation must not duplicate the project catalog"
        );
    }
    assert!(html.contains("Visible active initial work"));
    assert!(!html.contains("Private hidden"));
    assert!(
        !html.contains("class=\"sidebar-destination native-sidebar-destination\""),
        "Collapsed project panels must not serialize their destination trees"
    );
    println!(
        "GET / initial HTML: {} bytes for one project; {} bytes for 45 projects",
        initial.len(),
        expanded.len()
    );
    assert!(
        expanded.len() < 135_000,
        "45-project GET / must not repeat full navigation controllers: {} bytes",
        expanded.len()
    );
    assert!(
        expanded.len() < initial.len() + 44 * 1_450,
        "Catalog growth must add markup and action arguments, not controller bodies: {} -> {} bytes",
        initial.len(),
        expanded.len()
    );
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
    let (origin, task) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        "src/topcoat/native/home.browser.test.cjs",
        &origin,
        &fixture.token,
    );
    // The existing visual capture option supplies the installed, pinned master.
    // Native browser coverage runs without this optional screenshot reference.
    if let Some(snapshot) = std::env::var_os("LIFIC_SVELTE_SNAPSHOT") {
        command.arg(snapshot);
    }
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
