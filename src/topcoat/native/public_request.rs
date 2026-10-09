//! Discard private credentials before capturing a published request context.

use axum::{
    body::Body,
    http::{Request, header},
    middleware::Next,
    response::Response,
};

pub(crate) fn strip_credentials<B>(request: &mut Request<B>) {
    let path = request.uri().path();
    if path == "/public" || path.starts_with("/public/") {
        let headers = request.headers_mut();
        headers.remove(header::COOKIE);
        headers.remove(header::AUTHORIZATION);
    }
}

pub(crate) async fn layer(mut request: Request<Body>, next: Next) -> Response {
    strip_credentials(&mut request);
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_scope_preserves_transport_metadata_and_private_path_credentials() {
        for (path, public) in [
            ("/public", true),
            ("/public/ACC/issues?q=needle", true),
            ("/public/__native/comments", true),
            ("/public/api/projects/ACC/attachments/1", true),
            ("/publicity/issues", false),
            ("/ACC/issues", false),
            ("/api/projects", false),
        ] {
            let mut request = Request::builder()
                .method("POST")
                .uri(path)
                .header(header::COOKIE, "lific_token=private")
                .header(header::AUTHORIZATION, "Bearer private")
                .header(header::HOST, "reader.test")
                .header(header::ORIGIN, "https://reader.test")
                .header("x-forwarded-prefix", "/team/lific")
                .header("x-forwarded-for", "192.0.2.3")
                .extension(42_usize)
                .body("unchanged request body")
                .unwrap();
            let original_headers = request.headers().clone();
            strip_credentials(&mut request);
            let mut expected = original_headers;
            if public {
                expected.remove(header::COOKIE);
                expected.remove(header::AUTHORIZATION);
            }
            assert_eq!(request.headers(), &expected, "{path}");
            assert_eq!(request.uri().to_string(), path);
            assert_eq!(request.method(), "POST");
            assert_eq!(request.body(), &"unchanged request body");
            assert_eq!(request.extensions().get::<usize>(), Some(&42));
        }
    }
}
