//! HTTP procedures reuse the established Argon2, rate-limit, and session path.
use std::sync::Arc;

use super::super::context;
use crate::config::{AuthConfig, Config};
use axum::{
    Extension,
    extract::{ConnectInfo, Json, State},
};
use topcoat::{
    context::{Cx, app_context},
    router::request::{headers, remote_addr},
    runtime::procedure,
};

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
    Ok(super::super::auth_actions::finish(
        cx,
        result,
        "Unable to sign in. Try again.",
    ))
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
    Ok(super::super::auth_actions::finish(
        cx,
        result,
        "Unable to sign in. Try again.",
    ))
}
