//! Account settings commands resolve cookie authority again for every write.

use std::sync::Arc;

use axum::{
    Extension,
    extract::{ConnectInfo, Json, State},
    http::header,
};
use topcoat::{
    context::{Cx, app_context},
    router::{
        request::{headers, remote_addr},
        response::response_headers,
    },
    runtime::{procedure, record},
};

use super::super::{account_profile, context, transport};
use crate::{config::Config, error::LificError};

pub(super) fn same_account(cx: &Cx, expected: i64) -> Result<context::Caller, LificError> {
    let caller = context::caller(cx)?;
    if crate::api::require_user(&caller.identity)?.id != expected {
        return Err(LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        ));
    }
    Ok(caller)
}

#[record]
#[derive(Clone)]
pub(super) struct SaveOutcome {
    pub session: Option<String>,
    pub profile: Result<account_profile::Profile, String>,
}

// The browser may compare session ownership without receiving the cookie token.
fn session_fingerprint(caller: &context::Caller) -> Option<String> {
    caller.session_token.as_ref().map(|token| {
        let mut bytes = b"lific-native-profile-session\0".to_vec();
        bytes.extend_from_slice(token.as_bytes());
        crate::auth::sha256_hex(&bytes)
    })
}

#[procedure("/__native_settings/profile_session")]
pub(super) async fn profile_session(
    cx: &Cx,
    account: i64,
) -> topcoat::Result<Result<Option<String>, String>> {
    Ok(same_account(cx, account)
        .map(|caller| session_fingerprint(&caller))
        .map_err(|error| error.to_string()))
}

#[procedure("/__native_settings/save_profile")]
pub(super) async fn save_profile(
    cx: &Cx,
    account: i64,
    display_name: Option<String>,
    email: Option<String>,
) -> topcoat::Result<SaveOutcome> {
    let caller = match same_account(cx, account) {
        Ok(caller) => caller,
        Err(error) => {
            return Ok(SaveOutcome {
                session: None,
                profile: Err(error.to_string()),
            });
        }
    };
    let session = session_fingerprint(&caller);
    let profile = caller
        .scope(async {
            let user = crate::api::require_user(&caller.identity)?;
            crate::db::queries::users::update_profile(
                &*context::db(cx).write()?,
                user.id,
                display_name.as_deref().map(str::trim),
                email.as_deref().map(str::trim),
            )
            .map(account_profile::Profile::from)
        })
        .await
        .map_err(|error| error.to_string());
    Ok(SaveOutcome { session, profile })
}

#[procedure("/__native_settings/bot_action")]
pub(super) async fn bot_action(
    cx: &Cx,
    account: i64,
    id: i64,
    remove: bool,
) -> topcoat::Result<(bool, String)> {
    let caller = match same_account(cx, account) {
        Ok(caller) => caller,
        Err(error) => return Ok((false, error.to_string())),
    };
    let result = caller
        .scope(async {
            context::db(cx).transaction(|tx| {
                let fresh = crate::auth::fresh_caller(tx, account)?;
                if remove {
                    crate::db::queries::users::delete_bot(tx, id, fresh.id, fresh.is_admin)
                } else {
                    crate::db::queries::users::disconnect_bot(tx, id, fresh.id, fresh.is_admin)
                }
            })
        })
        .await;
    Ok(match result {
        Ok(()) => (true, "saved".into()),
        Err(error) => (false, error.to_string()),
    })
}

#[record]
#[derive(Clone, Debug)]
pub(super) struct ConnectFailure {
    pub message: String,
    pub requires_confirmation: bool,
    pub automatic_confirmation: bool,
}

impl ConnectFailure {
    fn terminal(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            requires_confirmation: false,
            automatic_confirmation: false,
        }
    }

    fn confirmation(message: impl Into<String>, automatic_confirmation: bool) -> Self {
        Self {
            message: message.into(),
            requires_confirmation: true,
            automatic_confirmation,
        }
    }
}

fn needs_confirmation(error: &LificError) -> bool {
    matches!(error, LificError::Forbidden(message) if message == "recent authentication required")
}

async fn mint_tool(
    cx: &Cx,
    caller: &context::Caller,
    token: &str,
    tool: String,
    display_name: String,
) -> Result<String, LificError> {
    let mut request_headers = headers(cx).clone();
    request_headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().map_err(|_| {
            LificError::BadRequest("Unable to connect this tool. Try again.".into())
        })?,
    );
    let result = caller
        .scope(crate::api::auth::create_bot(
            State(context::db(cx).clone()),
            Extension(caller.identity.clone()),
            request_headers,
            Json(crate::api::auth::CreateBotRequest {
                tool,
                display_name: if display_name.trim().is_empty() {
                    None
                } else {
                    Some(display_name)
                },
            }),
        ))
        .await?;
    result.0["key"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| LificError::Internal("tool creation returned no credential".into()))
}

#[procedure("/__native_settings/connect")]
pub(super) async fn connect(
    cx: &Cx,
    account: i64,
    tool: String,
    display_name: String,
) -> topcoat::Result<Result<String, ConnectFailure>> {
    let caller = match same_account(cx, account) {
        Ok(caller) => caller,
        Err(error) => return Ok(Err(ConnectFailure::terminal(error.client_message()))),
    };
    let Some(token) = caller.session_token.as_deref() else {
        return Ok(Err(ConnectFailure::terminal(
            "Sign in with your account to connect a tool.",
        )));
    };
    Ok(
        match mint_tool(cx, &caller, token, tool, display_name).await {
            Ok(key) => Ok(key),
            Err(error) if needs_confirmation(&error) => {
                let settings = crate::db::queries::settings::get(&*context::db(cx).read()?)?;
                Err(ConnectFailure::confirmation(
                    error.client_message(),
                    settings.web_auto_login,
                ))
            }
            Err(error) => Err(ConnectFailure::terminal(error.client_message())),
        },
    )
}

#[procedure("/__native_settings/confirm_connect")]
pub(super) async fn confirm_connect(
    cx: &Cx,
    account: i64,
    tool: String,
    display_name: String,
    password: Option<String>,
) -> topcoat::Result<Result<String, ConnectFailure>> {
    let caller = match same_account(cx, account) {
        Ok(caller) => caller,
        Err(error) => return Ok(Err(ConnectFailure::terminal(error.client_message()))),
    };
    if caller.session_token.is_none() {
        return Ok(Err(ConnectFailure::terminal(
            "Sign in with your account to connect a tool.",
        )));
    }
    let Some(peer) = remote_addr(cx) else {
        return Ok(Err(ConnectFailure::terminal(
            "Unable to refresh your session. Try again.",
        )));
    };
    let cfg = app_context::<Config>(cx);
    let auth_cfg =
        crate::config::AuthConfig::from_server(&cfg.auth, cfg.server.public_url.as_deref());
    let mut request_headers = headers(cx).clone();
    request_headers.extend(caller.session_headers()?);
    let refreshed = caller
        .scope(crate::services::sessions::refresh_session(
            context::db(cx),
            &auth_cfg,
            &caller.identity,
            peer,
            app_context::<Arc<[crate::ratelimit::IpNetwork]>>(cx),
            Some(app_context::<Arc<crate::ratelimit::RateLimiter>>(cx)),
            &request_headers,
            password,
        ))
        .await;
    let refreshed = match refreshed {
        Ok(refreshed) => refreshed,
        Err(error) => {
            return Ok(Err(ConnectFailure::confirmation(
                error.client_message(),
                false,
            )));
        }
    };
    let cookie = crate::services::sessions::session_cookie(
        &refreshed.session.token,
        &refreshed.session.expires_at,
        auth_cfg.secure_cookies,
    );
    response_headers(cx).append(
        header::SET_COOKIE,
        cookie.parse().expect("valid session cookie"),
    );
    Ok(
        match mint_tool(cx, &caller, &refreshed.session.token, tool, display_name).await {
            Ok(key) => Ok(key),
            Err(error) if needs_confirmation(&error) => Err(ConnectFailure::terminal(
                "That still was not accepted. Sign out and sign back in, then try connecting again.",
            )),
            Err(error) => Err(ConnectFailure::terminal(error.client_message())),
        },
    )
}

#[procedure("/__native_settings/password")]
pub(super) async fn change_password(
    cx: &Cx,
    account: i64,
    current: String,
    next: String,
) -> topcoat::Result<(bool, String)> {
    let caller = match same_account(cx, account) {
        Ok(caller) => caller,
        Err(error) => return Ok((false, error.to_string())),
    };
    let Some(peer) = remote_addr(cx) else {
        return Ok((false, "Unable to update your password. Try again.".into()));
    };
    let cfg = app_context::<Config>(cx);
    let auth_cfg =
        crate::config::AuthConfig::from_server(&cfg.auth, cfg.server.public_url.as_deref());
    let result = caller
        .scope(crate::api::auth::change_password(
            State(context::db(cx).clone()),
            Extension(auth_cfg),
            Extension(caller.identity.clone()),
            Extension(app_context::<crate::realtime::RealtimeHub>(cx).clone()),
            ConnectInfo(peer),
            Extension(app_context::<Arc<[crate::ratelimit::IpNetwork]>>(cx).clone()),
            Some(Extension(
                app_context::<Arc<crate::ratelimit::RateLimiter>>(cx).clone(),
            )),
            headers(cx).clone(),
            Json(crate::api::auth::ChangePasswordRequest {
                current_password: current,
                new_password: next,
            }),
        ))
        .await;
    let (ok, message) =
        super::super::auth_actions::finish(cx, result, "Couldn't update your password. Try again.");
    Ok((ok, message))
}

#[procedure("/__native_settings/sign_out_all")]
pub(super) async fn sign_out_all(cx: &Cx, account: i64) -> topcoat::Result<(bool, String)> {
    let caller = match same_account(cx, account) {
        Ok(caller) => caller,
        Err(error) => return Ok((false, error.to_string())),
    };
    let cfg = app_context::<Config>(cx);
    let auth_cfg =
        crate::config::AuthConfig::from_server(&cfg.auth, cfg.server.public_url.as_deref());
    let result = caller
        .scope(crate::api::auth::revoke_all_sessions(
            State(context::db(cx).clone()),
            Extension(auth_cfg),
            Extension(caller.identity.clone()),
            Extension(app_context::<crate::realtime::RealtimeHub>(cx).clone()),
        ))
        .await;
    let (ok, message) =
        super::super::auth_actions::finish(cx, result, "Couldn't sign out everywhere. Try again.");
    Ok((
        ok,
        if ok {
            transport::mounted_url(cx, "/login")
        } else {
            message
        },
    ))
}

#[procedure("/__native_settings/sign_out")]
pub(super) async fn sign_out(cx: &Cx, account: i64) -> topcoat::Result<(bool, String)> {
    let caller = match same_account(cx, account) {
        Ok(caller) => caller,
        Err(error) => return Ok((false, error.to_string())),
    };
    let Some(token) = caller.session_token else {
        return Ok((
            false,
            "Sign out is only available to a signed-in browser session.".into(),
        ));
    };
    let cfg = app_context::<Config>(cx);
    let auth_cfg =
        crate::config::AuthConfig::from_server(&cfg.auth, cfg.server.public_url.as_deref());
    let mut request_headers = headers(cx).clone();
    let bearer = format!("Bearer {token}");
    let bearer = match bearer.parse() {
        Ok(value) => value,
        Err(_) => return Ok((false, "Unable to sign out. Try again.".into())),
    };
    request_headers.insert(header::AUTHORIZATION, bearer);
    let result = crate::api::auth::auth_logout(
        State(context::db(cx).clone()),
        Extension(auth_cfg),
        request_headers,
    )
    .await;
    let (ok, message) =
        super::super::auth_actions::finish(cx, result, "Couldn't sign out. Try again.");
    Ok((
        ok,
        if ok {
            transport::mounted_url(cx, "/login")
        } else {
            message
        },
    ))
}
