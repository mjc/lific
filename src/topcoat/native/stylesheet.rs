//! Production stylesheet cache identity and actual browser application.

use std::net::SocketAddr;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

use super::home_fixture::{self, Fixture};

async fn get(fixture: &Fixture, uri: &str, prefix: &str) -> axum::response::Response {
    let mut request = Request::builder()
        .uri(uri)
        .header("cookie", format!("lific_token={}", fixture.token));
    if !prefix.is_empty() {
        request = request.header("x-forwarded-prefix", prefix);
    }
    let mut request = request.body(Body::empty()).unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<SocketAddr>().unwrap(),
    ));
    fixture.app.clone().oneshot(request).await.unwrap()
}

#[tokio::test]
async fn native_stylesheet_document_url_fingerprints_the_exact_uncached_production_css() {
    let fixture = home_fixture::fixture();
    for prefix in ["", "/app", "/ACC"] {
        let response = get(&fixture, "/", prefix).await;
        assert_eq!(response.status(), StatusCode::OK);
        let html = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8(html.to_vec()).unwrap();
        let stylesheets = html
            .split("<link")
            .filter(|tag| {
                tag.split('>')
                    .next()
                    .unwrap()
                    .contains("rel=\"stylesheet\"")
            })
            .map(|tag| {
                tag.split("href=\"")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            stylesheets.len(),
            1,
            "native document stylesheet contract: {stylesheets:?}"
        );
        let href = stylesheets[0];
        assert!(
            href.starts_with(&format!("{prefix}/__topcoat-app.css?v=")),
            "unversioned stylesheet: {href}"
        );
        let response = get(&fixture, href.strip_prefix(prefix).unwrap(), prefix).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["content-type"],
            "text/css; charset=utf-8"
        );
        assert_eq!(response.headers()["cache-control"], "no-cache");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        let css = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            href,
            format!("{prefix}/__topcoat-app.css?v={:x}", Sha256::digest(&css))
        );
        let css = String::from_utf8(css.to_vec()).unwrap();
        for selector in [
            ":root",
            ".tc-button",
            ".tc-shell__skip",
            ".native-home-shell",
            ".tc-home-active",
            ".tc-home-sections",
            ".tc-native-home__page",
        ] {
            assert!(
                css.contains(selector),
                "missing bundled section: {selector}"
            );
        }
    }
}

#[tokio::test]
async fn native_runtime_document_url_fingerprints_the_shipped_framework_runtime() {
    let fixture = home_fixture::fixture();
    for prefix in ["", "/app", "/ACC"] {
        let response = get(&fixture, "/", prefix).await;
        assert_eq!(response.status(), StatusCode::OK);
        let html = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8(html.to_vec()).unwrap();
        let scripts = html
            .split("<script")
            .filter_map(|tag| tag.split('>').next().unwrap().split("src=\"").nth(1))
            .map(|source| source.split('"').next().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            scripts.len(),
            1,
            "Home must load only its framework runtime: {scripts:?}"
        );
        let src = scripts[0];
        assert!(
            src.starts_with(&format!("{prefix}/__topcoat-runtime.js?v=")),
            "unversioned runtime: {src}"
        );
        let response = get(&fixture, src.strip_prefix(prefix).unwrap(), prefix).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["content-type"],
            "text/javascript; charset=utf-8"
        );
        assert_eq!(response.headers()["cache-control"], "no-cache");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        let runtime = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(runtime.as_ref(), include_bytes!("../assets/runtime.js"));
        assert_eq!(
            src,
            format!(
                "{prefix}/__topcoat-runtime.js?v={:x}",
                Sha256::digest(&runtime)
            )
        );
    }
}

#[tokio::test]
async fn native_stylesheet_browser_escapes_stale_bare_css_and_applies_the_mounted_bundle() {
    let fixture = home_fixture::fixture();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        "src/topcoat/native/stylesheet.browser.test.cjs",
        &origin,
        &fixture.token,
    );
    let result = tokio::time::timeout(std::time::Duration::from_secs(150), command.output()).await;
    server.abort();
    let output = result
        .expect("production stylesheet browser timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
