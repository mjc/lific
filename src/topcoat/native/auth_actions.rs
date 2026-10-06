//! Shared authentication procedure completion; session secrets stay server-side.
use crate::error::LificError;
use axum::response::IntoResponse;
use topcoat::{
    context::Cx,
    router::{header, response::response_headers},
};

pub(super) fn finish(
    cx: &Cx,
    result: Result<impl IntoResponse, LificError>,
    failure: &str,
) -> (bool, String) {
    match result {
        Ok(result) => {
            let response = result.into_response();
            for cookie in response.headers().get_all(header::SET_COOKIE) {
                response_headers(cx).append(header::SET_COOKIE, cookie.clone());
            }
            (true, super::transport::mounted_url(cx, "/"))
        }
        Err(LificError::BadRequest(message) | LificError::Forbidden(message)) => (false, message),
        Err(error) => {
            tracing::error!(error=%error, "native authentication failed");
            (false, failure.into())
        }
    }
}
