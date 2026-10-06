//! Frozen native grants and fresh-document continuation after session rotation.
use super::super::{context, transport};
use super::{
    management_model::{Command, Continuation},
    management_store::ManagementStore,
};
use crate::{
    actor::Transport,
    config::{AuthConfig, Config},
    error::LificError,
    realtime::RealtimeHub,
};
use std::sync::Arc;
use topcoat::{
    context::{Cx, app_context},
    router::{
        header,
        request::{headers, remote_addr},
        response::response_headers,
    },
    runtime::procedure,
};
pub(super) type Outcome = (String, String, String);
fn failed(kind: &str, error: LificError) -> Outcome {
    (
        kind.into(),
        super::actions::error_message(error),
        String::new(),
    )
}
fn same_account(cx: &Cx, account: i64) -> Result<context::Caller, LificError> {
    let caller = context::caller(cx)?;
    if crate::api::require_user(&caller.identity)?.id != account {
        return Err(LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        ));
    }
    Ok(caller)
}
async fn commit(
    cx: &Cx,
    caller: &context::Caller,
    project: i64,
    command: &Command,
) -> Result<(), LificError> {
    caller
        .scope(async {
            let db = context::db(cx);
            let hub = app_context::<RealtimeHub>(cx);
            match command {
                Command::Add { user, role } => {
                    crate::services::project_members::add(
                        db,
                        hub,
                        &caller.identity,
                        caller.session_token.as_deref(),
                        project,
                        *user,
                        role,
                    )?;
                }
                Command::Role { user, role, .. } => {
                    crate::services::project_members::change_role(
                        db,
                        hub,
                        &caller.identity,
                        caller.session_token.as_deref(),
                        project,
                        *user,
                        role,
                    )?;
                }
                Command::Remove { user } => crate::services::project_members::remove(
                    db,
                    hub,
                    &caller.identity,
                    project,
                    *user,
                )?,
                Command::Lead { user, .. } => {
                    crate::services::project_overview::update(
                        db,
                        hub,
                        &caller.identity,
                        caller.session_token.as_deref(),
                        project,
                        crate::db::models::UpdateProject {
                            lead_user_id: Some(*user),
                            ..Default::default()
                        },
                    )?;
                }
            }
            Ok(())
        })
        .await
}
fn destination(cx: &Cx, project: i64) -> Result<String, LificError> {
    let conn = context::db(cx).read()?;
    let project = crate::db::queries::get_project(&conn, project)?;
    Ok(transport::mounted_url(
        cx,
        &format!("/{}/overview", project.identifier),
    ))
}
async fn rotate_and_commit(
    cx: &Cx,
    caller: context::Caller,
    continuation: Continuation,
    password: Option<String>,
) -> topcoat::Result<Outcome> {
    let account = crate::api::require_user(&caller.identity)?;
    crate::authz::require_role(
        context::db(cx),
        &caller.identity,
        continuation.project,
        crate::db::models::Role::Lead,
    )?;
    // Resolve the fallback destination before rotating. A concurrent deletion
    // must not strand the revoked document while reporting a grant failure.
    let original_destination = destination(cx, continuation.project)?;
    let Some(peer) = remote_addr(cx) else {
        return Ok(failed(
            "confirmation_failed",
            LificError::Unavailable("missing client address".into()),
        ));
    };
    let cfg = app_context::<Config>(cx);
    let auth = AuthConfig::from_server(&cfg.auth, cfg.server.public_url.as_deref());
    let reservation =
        match app_context::<ManagementStore>(cx).reserve(account.id, continuation.clone()) {
            Ok(reservation) => reservation,
            Err(error) => return Ok(failed("confirmation_failed", error)),
        };
    let mut session_headers = headers(cx).clone();
    session_headers.extend(caller.session_headers()?);
    reservation.ensure_owner(account.id)?;
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
        .await;
    let refreshed = match refreshed {
        Ok(refreshed) => refreshed,
        Err(error) => {
            drop(reservation);
            return Ok(failed("confirmation_failed", error));
        }
    };
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
    let fresh = context::Caller {
        identity: Some(crate::auth::fresh_identity(&refreshed.user, Transport::Web)),
        session_token: Some(refreshed.session.token),
    };
    let result = commit(cx, &fresh, continuation.project, &continuation.command).await;
    let destination = destination(cx, continuation.project).unwrap_or(original_destination);
    match result {
        Ok(()) => {
            drop(reservation);
            Ok(("navigate".into(), destination, String::new()))
        }
        Err(error) => {
            let handle = reservation.publish(super::actions::error_message(error))?;
            Ok((
                "resume".into(),
                format!("{destination}?management_resume={handle}"),
                String::new(),
            ))
        }
    }
}
#[procedure("/__native_overview/manage")]
#[allow(clippy::too_many_arguments)]
pub(super) async fn attempt(
    cx: &Cx,
    account: i64,
    project: i64,
    kind: String,
    user: Option<i64>,
    role: String,
    previous: String,
    allow_automatic: bool,
) -> topcoat::Result<Outcome> {
    let caller = match same_account(cx, account) {
        Ok(caller) => caller,
        Err(error) => return Ok(failed("account_changed", error)),
    };
    let command = match Command::from_wire(&kind, user, role, previous) {
        Ok(command) => command,
        Err(error) => return Ok(failed("invalid", error)),
    };
    match commit(cx, &caller, project, &command).await {
        Ok(()) => Ok(("saved".into(), String::new(), String::new())),
        Err(LificError::Forbidden(message)) if message == "recent authentication required" => {
            let automatic = if allow_automatic {
                let conn = context::db(cx).read()?;
                crate::db::queries::settings::get(&conn)?.web_auto_login
            } else {
                false
            };
            if automatic {
                let outcome = rotate_and_commit(
                    cx,
                    caller,
                    Continuation {
                        project,
                        command,
                        error: String::new(),
                        automatic_note: String::new(),
                    },
                    None,
                )
                .await?;
                if outcome.0 == "confirmation_failed" {
                    Ok(("reauth".into(), String::new(), outcome.1))
                } else {
                    Ok(outcome)
                }
            } else {
                Ok(("reauth".into(), String::new(), String::new()))
            }
        }
        Err(error) => Ok(failed("invalid", error)),
    }
}
#[procedure("/__native_overview/manage_confirm")]
#[allow(clippy::too_many_arguments)]
pub(super) async fn confirm(
    cx: &Cx,
    account: i64,
    project: i64,
    kind: String,
    user: Option<i64>,
    role: String,
    previous: String,
    password: String,
) -> topcoat::Result<Outcome> {
    let caller = match same_account(cx, account) {
        Ok(caller) => caller,
        Err(error) => return Ok(failed("account_changed", error)),
    };
    let command = match Command::from_wire(&kind, user, role, previous) {
        Ok(command) => command,
        Err(error) => return Ok(failed("invalid", error)),
    };
    rotate_and_commit(
        cx,
        caller,
        Continuation {
            project,
            command,
            error: String::new(),
            automatic_note: String::new(),
        },
        Some(password),
    )
    .await
}
pub(super) fn take_continuation(
    cx: &Cx,
    query: &str,
    account: i64,
    project: i64,
) -> Result<Option<Continuation>, LificError> {
    let handles = query
        .split('&')
        .filter_map(|part| part.strip_prefix("management_resume="))
        .collect::<Vec<_>>();
    if handles.len() != 1
        || handles[0].len() != 48
        || !handles[0]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Ok(None);
    }
    let caller = same_account(cx, account)?;
    crate::authz::require_role(
        context::db(cx),
        &caller.identity,
        project,
        crate::db::models::Role::Viewer,
    )?;
    match app_context::<ManagementStore>(cx).consume(handles[0], account, project) {
        Ok(continuation) => Ok(Some(continuation)),
        Err(LificError::NotFound(_)) => Ok(None),
        Err(error) => Err(error),
    }
}
