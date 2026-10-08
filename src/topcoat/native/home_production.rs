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

fn assert_native_logo_preload(headers: &axum::http::HeaderMap, prefix: &str) {
    let links = headers
        .get_all("link")
        .iter()
        .flat_map(|value| value.to_str().unwrap().split(", ").map(str::to_owned))
        .collect::<Vec<_>>();
    assert_eq!(
        links,
        [format!("<{prefix}/logo.webp>; rel=preload; as=image")],
        "icons embedded in the stylesheet need no preload hints"
    );
}

#[tokio::test]
async fn native_home_keeps_dashboard_outside_notification_owner_before_and_after_hydration() {
    let fixture = fixture();
    for mount in ["", "/app", "/ACC"] {
        let (status, initial) = home_fixture::document(&fixture, mount, "/", true, None).await;
        assert_eq!(status, StatusCode::OK);
        let signals = home_fixture::page_signals(&initial);
        let (status, hydrated) =
            home_fixture::document(&fixture, mount, "/", true, Some(signals)).await;
        assert_eq!(status, StatusCode::OK);
        for html in [&initial, &hydrated] {
            let document = scraper::Html::parse_document(html);
            let selector = scraper::Selector::parse("#native-deferred-delete-owner").unwrap();
            let owner = document.select(&selector).next().unwrap();
            assert_eq!(
                owner
                    .children()
                    .filter(|node| node.value().is_element())
                    .count(),
                0,
                "dormant notifications must not contain the page at {mount}"
            );
            for selector in [
                ".native-home-body > .native-home-topbar",
                ".native-home-body > .native-home-panel-wrap > #main-content > [data-native-home]",
            ] {
                assert!(
                    document
                        .select(&scraper::Selector::parse(selector).unwrap())
                        .next()
                        .is_some(),
                    "Home must retain its viewport flex layout at {mount}: {selector}"
                );
            }
        }
    }
}

#[tokio::test]
async fn native_document_preloads_logo_before_body_without_icon_hints_at_every_mount() {
    let fixture = fixture();
    let cookie = format!("lific_token={}", fixture.token);
    for prefix in ["", "/app", "/ACC"] {
        let response = get(&fixture, Some(&cookie), Some(prefix)).await;
        assert_eq!(response.status(), StatusCode::OK);
        // Headers must be ready before the caller reads any response body.
        assert_native_logo_preload(response.headers(), prefix);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = std::str::from_utf8(&body).unwrap();
        let document = scraper::Html::parse_document(html);
        let icons = scraper::Selector::parse("svg.native-icon[data-icon]").unwrap();
        for name in ["Circle", "CircleDot"] {
            assert!(
                document
                    .select(&icons)
                    .any(|node| node.value().attr("data-icon") == Some(name))
            );
        }
        assert_eq!(
            document
                .select(&scraper::Selector::parse("use").unwrap())
                .count(),
            0
        );
        assert!(!html.contains("/__native_icons/"));
    }

    for (path, token) in [("/", None), ("/api/projects", Some(cookie.as_str()))] {
        let response = get_path(&fixture, path, token, None).await;
        assert!(
            !response.headers().contains_key("link"),
            "redirects and REST responses must not preload page images"
        );
    }
}

#[tokio::test]
async fn native_document_preloads_use_one_http_field_for_selected_logo() {
    let fixture = fixture();
    let cookie = format!("lific_token={}", fixture.token);
    let response = get(&fixture, Some(&cookie), None).await;
    assert_eq!(response.headers().get_all("link").iter().count(), 1);
}

#[tokio::test]
async fn native_document_preloads_omit_embedded_icons_and_unopened_picker_choices() {
    let fixture = fixture();
    let token = {
        let conn = fixture.db.write().unwrap();
        let admin = queries::users::get_user_by_username(&conn, "admin").unwrap();
        queries::users::create_session(&conn, admin.id, None)
            .unwrap()
            .token
    };
    let cookie = format!("lific_token={token}");
    for prefix in ["", "/app", "/ACC"] {
        let response = get_path(&fixture, "/ACC/overview", Some(&cookie), Some(prefix)).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_native_logo_preload(response.headers(), prefix);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = std::str::from_utf8(&body).unwrap();
        let document = scraper::Html::parse_document(html);
        let choices = scraper::Selector::parse(".native-project-picker-choice").unwrap();
        assert_eq!(
            document.select(&choices).count(),
            80,
            "retain the original picker grid"
        );
        let picker_icons =
            scraper::Selector::parse(".native-project-picker-choice svg.native-icon[data-icon]")
                .unwrap();
        assert!(document.select(&picker_icons).count() > 0);
        assert_eq!(
            document
                .select(&scraper::Selector::parse("use").unwrap())
                .count(),
            0
        );
    }
}

#[tokio::test]
async fn native_home_icons_are_inline_at_every_mount_without_icon_requests() {
    let stylesheet = super::super::assets::app_stylesheet();
    assert!(!stylesheet.contains("__native_icons/"));
    assert!(!stylesheet.contains("data:image/svg+xml,"));
    assert!(!stylesheet.contains("mask-image"));
    assert!(
        super::icons::stylesheet().len() < 512,
        "inline icons need only shared SVG paint defaults: {} bytes",
        super::icons::stylesheet().len()
    );

    let fixture = fixture();
    let cookie = format!("lific_token={}", fixture.token);
    for prefix in ["", "/app", "/ACC"] {
        let response = get(&fixture, Some(&cookie), Some(prefix)).await;
        assert_native_logo_preload(response.headers(), prefix);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = std::str::from_utf8(&body).unwrap();
        let document = scraper::Html::parse_document(html);
        let icons = scraper::Selector::parse("svg.native-icon[data-icon]").unwrap();
        let rendered = document.select(&icons).collect::<Vec<_>>();
        assert!(!rendered.is_empty());
        for icon in rendered {
            assert_eq!(icon.value().attr("viewBox"), Some("0 0 24 24"));
            assert_eq!(icon.value().attr("aria-hidden"), Some("true"));
            assert!(icon.children().any(|child| child.value().is_element()));
        }
        assert_eq!(
            document
                .select(&scraper::Selector::parse("use").unwrap())
                .count(),
            0
        );
        assert!(!html.contains("__native_icons/") && !html.contains("data:image/svg+xml"));
        let css_url = document
            .select(&scraper::Selector::parse("link[rel=stylesheet]").unwrap())
            .next()
            .unwrap()
            .value()
            .attr("href")
            .unwrap();
        assert!(css_url.starts_with(&format!("{prefix}/__topcoat-app.css")));
        let response = get_path(
            &fixture,
            css_url.strip_prefix(prefix).unwrap(),
            None,
            Some(prefix),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let css = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(css.as_ref(), stylesheet.as_bytes());
        for obsolete in [
            format!("/__native_icons/{}/ui.svg", "0".repeat(64)),
            format!("/__native_icons/{}/Fan.svg", "0".repeat(64)),
            "/__native_icons/stale/ChevronRight.mask.svg".to_owned(),
        ] {
            assert_eq!(
                get_path(&fixture, &obsolete, Some(&cookie), Some(prefix))
                    .await
                    .status(),
                StatusCode::NOT_FOUND
            );
        }
    }
}

#[tokio::test]
async fn native_home_sidebar_actions_use_compact_scalar_event_metadata() {
    let fixture = fixture();
    let response = get(
        &fixture,
        Some(&format!("lific_token={}", fixture.token)),
        None,
    )
    .await;
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = std::str::from_utf8(&bytes).unwrap();
    for event in ["click", "contextmenu", "keydown"] {
        assert!(
            html.contains(&format!("data-ns-{event}=\"")),
            "Missing compact {event} action metadata"
        );
    }
    assert!(!html.contains("data-native-sidebar-event-"));
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
                assert!(
                    tag.contains(&format!("data-topcoat-usize-bits=\"{}\"", usize::BITS)),
                    "the runtime must use the server's usize width: {tag}"
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
    for filename in ["__native-home-shell.js", "__native-sidebar.js"] {
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
    for (line, filename) in source
        .lines()
        .take(2)
        .zip(["__native-home-shell.js", "__native-sidebar.js"])
    {
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
        expanded.len() < 128_000,
        "45-project GET / must stay below the 128,000-byte response budget: {} bytes",
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
