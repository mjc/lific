//! Native instance-name writes reuse the canonical authenticated settings route.

use std::sync::Arc;

use axum::{
    Extension,
    extract::{Json, State},
    http::header,
};
use topcoat::{
    context::{Cx, app_context},
    router::{
        request::{headers, remote_addr},
        response::response_headers,
    },
    runtime::procedure,
};

use super::super::context::{self, Caller};
use crate::{config::Config, error::LificError};

fn same_account(cx: &Cx, expected: i64) -> Result<Caller, LificError> {
    let caller = context::caller(cx)?;
    if crate::api::require_user(&caller.identity)?.id != expected {
        return Err(LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        ));
    }
    Ok(caller)
}

fn account_admin(caller: &Caller, expected: i64) -> Result<(), LificError> {
    let user = crate::api::require_user(&caller.identity)?;
    if user.id != expected {
        return Err(LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        ));
    }
    if !user.is_admin {
        return Err(LificError::Forbidden("only an admin can do this".into()));
    }
    Ok(())
}

async fn save_name(
    cx: &Cx,
    caller: &Caller,
    account: i64,
    value: String,
) -> Result<String, LificError> {
    account_admin(caller, account)?;
    let headers = caller.session_headers()?;
    let response = caller
        .scope(crate::api::auth::instance_settings_patch(
            State(context::db(cx).clone()),
            Extension(app_context::<crate::realtime::RealtimeHub>(cx).clone()),
            Extension(caller.identity.clone()),
            None,
            headers,
            Json(crate::api::auth::InstanceSettingsPatchReq {
                instance_name: Some(value),
                ..Default::default()
            }),
        ))
        .await?;
    Ok(response.0["instance_name"]
        .as_str()
        .unwrap_or_default()
        .to_owned())
}

async fn rotate_and_save(
    cx: &Cx,
    caller: &Caller,
    account: i64,
    value: String,
    password: Option<String>,
) -> Result<String, LificError> {
    account_admin(caller, account)?;
    let Some(peer) = remote_addr(cx) else {
        return Err(LificError::Unavailable(
            "Unable to refresh your session. Try again.".into(),
        ));
    };
    let config = app_context::<Config>(cx);
    let auth =
        crate::config::AuthConfig::from_server(&config.auth, config.server.public_url.as_deref());
    let mut session_headers = headers(cx).clone();
    session_headers.extend(caller.session_headers()?);
    let refreshed = caller
        .scope(crate::services::sessions::refresh_session(
            context::db(cx),
            &auth,
            &caller.identity,
            peer,
            app_context::<Arc<[crate::ratelimit::IpNetwork]>>(cx),
            Some(app_context::<Arc<crate::ratelimit::RateLimiter>>(cx)),
            &session_headers,
            password,
        ))
        .await?;
    let cookie = crate::services::sessions::session_cookie(
        &refreshed.session.token,
        &refreshed.session.expires_at,
        auth.secure_cookies,
    );
    response_headers(cx).append(
        header::SET_COOKIE,
        cookie
            .parse()
            .expect("shared session cookie is a valid header"),
    );
    let fresh = Caller {
        identity: Some(crate::auth::fresh_identity(
            &refreshed.user,
            crate::actor::Transport::Web,
        )),
        session_token: Some(refreshed.session.token),
    };
    save_name(cx, &fresh, account, value).await
}

fn message(error: LificError) -> String {
    match error {
        LificError::BadRequest(message) | LificError::Forbidden(message) => message,
        LificError::Unavailable(message) => message,
        error => {
            tracing::error!(error=%error, "native instance name update failed");
            "Couldn't save the instance name. Try again.".into()
        }
    }
}

#[procedure("/__native_instance_settings/save_text")]
pub(super) async fn save_text(
    cx: &Cx,
    account: i64,
    field: String,
    value: String,
) -> topcoat::Result<(bool, String)> {
    if field != "name" {
        return Ok((false, "unknown instance setting".into()));
    }
    let caller = match same_account(cx, account) {
        Ok(caller) => caller,
        Err(error) => return Ok((false, message(error))),
    };
    match save_name(cx, &caller, account, value.clone()).await {
        Ok(name) => Ok((true, name)),
        Err(LificError::Forbidden(ref refusal_message))
            if refusal_message == crate::auth::RECENT_AUTH_REQUIRED_MESSAGE =>
        {
            let settings = crate::db::queries::settings::get(&*context::db(cx).read()?)?;
            let auth = app_context::<crate::auth::AuthState>(cx);
            if settings.web_auto_login || !auth.required {
                match rotate_and_save(cx, &caller, account, value, None).await {
                    Ok(name) => Ok((true, name)),
                    Err(LificError::BadRequest(ref message))
                        if message == "your password is required to confirm this" =>
                    {
                        Ok((false, crate::auth::RECENT_AUTH_REQUIRED_MESSAGE.into()))
                    }
                    Err(error) => Ok((false, message(error))),
                }
            } else {
                Ok((false, crate::auth::RECENT_AUTH_REQUIRED_MESSAGE.into()))
            }
        }
        Err(error) => Ok((false, message(error))),
    }
}

#[procedure("/__native_instance_settings/confirm_name")]
pub(super) async fn confirm_name(
    cx: &Cx,
    account: i64,
    value: String,
    password: String,
) -> topcoat::Result<(bool, String)> {
    let caller = match same_account(cx, account) {
        Ok(caller) => caller,
        Err(error) => return Ok((false, message(error))),
    };
    if caller.session_token.is_none() {
        return Ok((
            false,
            "Sign in with your account to confirm this change.".into(),
        ));
    }
    match rotate_and_save(cx, &caller, account, value, Some(password)).await {
        Ok(name) => Ok((true, name)),
        Err(error) => Ok((false, message(error))),
    }
}
