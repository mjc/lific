//! Signup uses the same trusted peer, limiter, policy and transaction as REST.
use crate::config::{AuthConfig, Config};
use axum::{
    Extension,
    extract::{ConnectInfo, Json, State},
};
use std::sync::Arc;
use topcoat::{
    context::{Cx, app_context},
    router::request::{headers, remote_addr},
    runtime::procedure,
};

#[procedure("/__native_signup/sign_up")]
pub(super) async fn sign_up(
    cx: &Cx,
    username: String,
    email: String,
    password: String,
) -> topcoat::Result<(bool, String)> {
    let peer = remote_addr(cx)
        .ok_or_else(|| crate::error::LificError::Unavailable("missing request peer".into()))?;
    let cfg = app_context::<Config>(cx);
    let result = crate::api::auth::auth_signup(
        State(super::super::context::db(cx).clone()),
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
        Json(crate::api::auth::SignupRequest {
            username,
            email,
            password,
            display_name: None,
        }),
    )
    .await;
    Ok(super::super::auth_actions::finish(
        cx,
        result,
        "Unable to create your account. Try again.",
    ))
}
