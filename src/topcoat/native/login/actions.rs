//! HTTP procedures reuse the established Argon2, rate-limit, and session path.
use std::sync::Arc;

use super::super::{context, transport};
use crate::{
    config::{AuthConfig, Config},
    error::LificError,
};
use axum::{
    Extension,
    extract::{ConnectInfo, Json, State},
    response::IntoResponse,
};
use topcoat::{
    context::{Cx, app_context},
    router::{
        header,
        request::{headers, remote_addr},
        response::response_headers,
    },
    runtime::procedure,
};

fn finish(cx: &Cx, result: Result<impl IntoResponse, LificError>) -> (bool, String) {
    match result {
        Ok(result) => {
            let response = result.into_response();
            // The existing adapter owns the cookie flags. Its JSON token never
            // crosses the native procedure boundary or enters browser state.
            for cookie in response.headers().get_all(header::SET_COOKIE) {
                response_headers(cx).append(header::SET_COOKIE, cookie.clone());
            }
            (true, transport::mounted_url(cx, "/"))
        }
        Err(LificError::BadRequest(message) | LificError::Forbidden(message)) => (false, message),
        Err(error) => {
            tracing::error!(error=%error, "native login failed");
            (false, "Unable to sign in. Try again.".into())
        }
    }
}

#[procedure("/__native_login/sign_in")]
pub(super) async fn sign_in(
    cx: &Cx,
    identity: String,
    password: String,
) -> topcoat::Result<(bool, String)> {
    let Some(peer) = remote_addr(cx) else {
        return Ok((false, "Unable to sign in. Try again.".into()));
    };
    let cfg = app_context::<Config>(cx);
    let result = crate::api::auth::auth_login(
        State(context::db(cx).clone()),
        Extension(AuthConfig::from_server(
            &cfg.auth,
            cfg.server.public_url.as_deref(),
        )),
        ConnectInfo(peer),
        Extension(app_context::<Arc<[crate::ratelimit::IpNetwork]>>(cx).clone()),
        Some(Extension(
            app_context::<Arc<crate::ratelimit::RateLimiter>>(cx).clone(),
        )),
        headers(cx).clone(),
        Json(crate::db::models::LoginRequest { identity, password }),
    )
    .await;
    Ok(finish(cx, result))
}

#[procedure("/__native_login/automatic")]
pub(super) async fn automatic(cx: &Cx) -> topcoat::Result<(bool, String)> {
    let cfg = app_context::<Config>(cx);
    let result = crate::api::auth::auth_auto_login(
        State(context::db(cx).clone()),
        Extension(AuthConfig::from_server(
            &cfg.auth,
            cfg.server.public_url.as_deref(),
        )),
    )
    .await;
    Ok(finish(cx, result))
}
