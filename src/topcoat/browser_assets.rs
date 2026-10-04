//! Embedded icons and install metadata shared by every browser route.

use topcoat::router::{Body, response::Response, route};

fn png(bytes: &'static [u8]) -> topcoat::Result<Response> {
    Ok(Response::builder()
        .header("content-type", "image/png")
        .header("cache-control", "public, max-age=86400")
        .body(Body::from(bytes))?)
}

#[route(GET "/logo.webp")]
async fn logo() -> topcoat::Result<Response> {
    Ok(Response::builder()
        .header("content-type", "image/webp")
        .header("cache-control", "public, max-age=86400")
        .body(Body::from(include_bytes!("assets/logo.webp").as_slice()))?)
}

#[route(GET "/favicon.png")]
async fn favicon() -> topcoat::Result<Response> {
    png(include_bytes!("assets/favicon.png"))
}

#[route(GET "/apple-touch-icon.png")]
async fn apple_touch_icon() -> topcoat::Result<Response> {
    png(include_bytes!("assets/apple-touch-icon.png"))
}

#[route(GET "/icon-192.png")]
async fn icon_192() -> topcoat::Result<Response> {
    png(include_bytes!("assets/icon-192.png"))
}

#[route(GET "/icon-512.png")]
async fn icon_512() -> topcoat::Result<Response> {
    png(include_bytes!("assets/icon-512.png"))
}

#[route(GET "/icon-maskable-512.png")]
async fn icon_maskable_512() -> topcoat::Result<Response> {
    png(include_bytes!("assets/icon-maskable-512.png"))
}

#[route(GET "/manifest.webmanifest")]
async fn manifest() -> topcoat::Result<Response> {
    Ok(Response::builder()
        .header("content-type", "application/manifest+json")
        .header("cache-control", "no-cache")
        .body(Body::from(include_str!("assets/manifest.webmanifest")))?)
}

#[cfg(test)]
mod tests {
    use tower::ServiceExt;

    #[tokio::test]
    async fn original_logo_is_an_embedded_webp_asset() {
        let response =
            topcoat::router::tower::TowerService::new(crate::server::topcoat_app::router())
                .oneshot(
                    axum::http::Request::builder()
                        .uri("/logo.webp")
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "image/webp");
        let bytes = axum::body::to_bytes(axum::body::Body::new(response.into_body()), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");
    }
}
