//! Embedded icons and install metadata shared by every browser route.

use topcoat::router::{Body, response::Response, route};

fn png(bytes: &'static [u8]) -> topcoat::Result<Response> {
    Ok(Response::builder()
        .header("content-type", "image/png")
        .header("cache-control", "public, max-age=86400")
        .body(Body::from(bytes))?)
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
