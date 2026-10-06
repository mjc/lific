//! Native project commands. Credentials remain at the server boundary.

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

use crate::{
    actor::Transport,
    config::{AuthConfig, Config},
    error::LificError,
    realtime::RealtimeHub,
};

use super::super::{context, transport};
use super::{
    draft::{DraftStore, Reservation},
    model::Draft,
};

// Status, destination or safe error, confirmation feedback or committed group warning.
pub(super) type Outcome = (String, String, String);

fn failed(status: &str, error: LificError) -> Outcome {
    let message = match error {
        LificError::BadRequest(message)
        | LificError::Forbidden(message)
        | LificError::Conflict(message)
        | LificError::NotFound(message)
        | LificError::TooManyRequests(message)
        | LificError::PayloadTooLarge(message) => message,
        error => {
            tracing::error!(error = %error, "native project creation failed");
            "Unable to create the project. Your draft is still here. Try again.".into()
        }
    };
    (status.into(), message, String::new())
}

async fn complete(
    cx: &Cx,
    caller: &context::Caller,
    draft: Draft,
    reservation: Option<Reservation>,
) -> topcoat::Result<Outcome> {
    let account = crate::api::require_user(&caller.identity)?;
    ensure_reservation(account.id, draft.group, &reservation)?;
    let input = match draft.input() {
        Ok(input) => input,
        Err(error) => return Ok(failed("invalid", error)),
    };
    let result = caller
        .scope(async {
            crate::services::projects::create_project(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                caller.session_token.as_deref(),
                input,
            )
        })
        .await;
    let project = match result {
        Ok(project) => project,
        Err(error) => {
            let outcome = failed("invalid", error);
            if let Some(reservation) = reservation {
                // Cookie has rotated: every failure returns a fresh document
                // continuation instead of leaving the revoked socket in use.
                let key = reservation.publish(outcome.1)?;
                return Ok((
                    "resume".into(),
                    transport::mounted_url(cx, &format!("/projects/new?resume={key}")),
                    String::new(),
                ));
            }
            return Ok(outcome);
        }
    };
    finish_created(cx, caller, draft, project, reservation).await
}

async fn rotate_and_complete(
    cx: &Cx,
    caller: context::Caller,
    draft: Draft,
    password: Option<String>,
) -> topcoat::Result<Outcome> {
    let account = crate::api::require_user(&caller.identity)?;
    let peer = match remote_addr(cx) {
        Some(peer) => peer,
        None => {
            return Ok(failed(
                "confirmation_failed",
                LificError::Unavailable("missing client address".into()),
            ));
        }
    };
    let cfg = app_context::<Config>(cx);
    let auth = AuthConfig::from_server(&cfg.auth, cfg.server.public_url.as_deref());
    let store = app_context::<DraftStore>(cx);
    // Admit storage before any session rotation. Never evict an existing draft.
    let reservation = match store.reserve(account.id, draft.clone()) {
        Ok(reservation) => reservation,
        Err(error) => return Ok(failed("confirmation_failed", error)),
    };
    // Keep the real forwarding headers for authoritative client_ip derivation.
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
    // Format is shared with the REST adapter: Secure/HttpOnly/SameSite remain
    // identical. A procedure result contains no credential.
    let cookie = crate::services::sessions::session_cookie(
        &refreshed.session.token,
        &refreshed.session.expires_at,
        auth.secure_cookies,
    );
    response_headers(cx).append(
        header::SET_COOKIE,
        cookie
            .parse()
            .expect("shared session formatter yields a valid response cookie"),
    );
    let fresh = context::Caller {
        identity: Some(crate::auth::fresh_identity(&refreshed.user, Transport::Web)),
        session_token: Some(refreshed.session.token),
    };
    complete(cx, &fresh, draft, Some(reservation)).await
}

fn same_account(cx: &Cx, expected: i64) -> Result<context::Caller, LificError> {
    let caller = context::caller(cx)?;
    if crate::api::require_user(&caller.identity)?.id != expected {
        return Err(LificError::Forbidden(
            "The signed-in account changed. Reload before creating the project.".into(),
        ));
    }
    Ok(caller)
}

#[procedure("/__native_project/create")]
#[allow(clippy::too_many_arguments)]
pub(super) async fn create(
    cx: &Cx,
    expected: i64,
    name: String,
    identifier: String,
    identifier_touched: bool,
    description: String,
    emoji: String,
    lead: Option<i64>,
    group: Option<i64>,
) -> topcoat::Result<Outcome> {
    let caller = match same_account(cx, expected) {
        Ok(caller) => caller,
        Err(error) => return Ok(failed("account_changed", error)),
    };
    let draft = Draft {
        name,
        identifier,
        identifier_touched,
        description,
        emoji,
        lead,
        group,
        error: String::new(),
    };
    let input = match draft.input() {
        Ok(input) => input,
        Err(error) => return Ok(failed("invalid", error)),
    };
    let account = crate::api::require_user(&caller.identity)?;
    let store = app_context::<DraftStore>(cx);
    let reservation = if draft.group.is_some() {
        match store.reserve(account.id, draft.clone()) {
            Ok(reservation) => Some(reservation),
            Err(error) => return Ok(failed("invalid", error)),
        }
    } else {
        None
    };
    ensure_reservation(account.id, draft.group, &reservation)?;
    let result = caller
        .scope(async {
            crate::services::projects::create_project(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                caller.session_token.as_deref(),
                input,
            )
        })
        .await;
    match result {
        Ok(project) => finish_created(cx, &caller, draft, project, reservation).await,
        Err(LificError::Forbidden(message)) if message == "recent authentication required" => {
            drop(reservation);
            let automatic =
                crate::db::queries::settings::get(&*context::db(cx).read()?)?.web_auto_login;
            if automatic {
                let outcome = rotate_and_complete(cx, caller, draft, None).await?;
                if outcome.0 == "confirmation_failed" {
                    return Ok(("reauth".into(), String::new(), outcome.1));
                }
                Ok(outcome)
            } else {
                Ok(("reauth".into(), String::new(), String::new()))
            }
        }
        Err(error) => {
            drop(reservation);
            Ok(failed("invalid", error))
        }
    }
}

// The normal successful create shares only the post-create step; it does not
// retry create as a way to file an already-committed project into a group.
async fn finish_created(
    cx: &Cx,
    caller: &context::Caller,
    draft: Draft,
    project: crate::db::models::Project,
    reservation: Option<Reservation>,
) -> topcoat::Result<Outcome> {
    let destination = transport::mounted_url(cx, &format!("/{}/overview", project.identifier));
    let warning = match draft.group {
        Some(group) => caller
            .scope(async {
                crate::services::project_form::assign_created_project(
                    context::db(cx),
                    app_context::<RealtimeHub>(cx),
                    &caller.identity,
                    project.id,
                    group,
                )
            })
            .await
            .err()
            .map(|error| {
                format!(
                    "Project created, but it wasn't added to the group: {}",
                    failed("invalid", error).1
                )
            }),
        None => None,
    };
    Ok(finish_committed(destination, warning, reservation))
}

fn ensure_reservation(
    account: i64,
    group: Option<i64>,
    reservation: &Option<Reservation>,
) -> Result<(), LificError> {
    match reservation {
        Some(reservation) => reservation.ensure_owner(account),
        None if group.is_none() => Ok(()),
        None => Err(LificError::Internal(
            "group assignment requires reserved notice storage".into(),
        )),
    }
}

// Creation has committed. A notice failure cannot change that outcome.
fn finish_committed(
    mut destination: String,
    warning: Option<String>,
    reservation: Option<Reservation>,
) -> Outcome {
    let warning = warning.unwrap_or_default();
    if !warning.is_empty() {
        let published = reservation
            .ok_or_else(|| {
                LificError::Internal("committed group assignment lost its reservation".into())
            })
            .and_then(|reservation| reservation.publish(warning.clone()));
        match published {
            Ok(key) => destination.push_str(&format!("?notice={key}&group_warning=1")),
            Err(error) => {
                tracing::error!(error = %error, "project created but group warning storage failed");
                // Native overview renders a fixed safe warning for this marker.
                destination.push_str("?group_warning=1");
            }
        }
    }
    ("created".into(), destination, warning)
}

#[procedure("/__native_project/confirm_and_create")]
#[allow(clippy::too_many_arguments)]
pub(super) async fn confirm(
    cx: &Cx,
    expected: i64,
    name: String,
    identifier: String,
    identifier_touched: bool,
    description: String,
    emoji: String,
    lead: Option<i64>,
    group: Option<i64>,
    password: String,
) -> topcoat::Result<Outcome> {
    let caller = match same_account(cx, expected) {
        Ok(caller) => caller,
        Err(error) => return Ok(failed("account_changed", error)),
    };
    if password.is_empty() {
        return Ok((
            "confirmation_failed".into(),
            "Enter your current password.".into(),
            String::new(),
        ));
    }
    let draft = Draft {
        name,
        identifier,
        identifier_touched,
        description,
        emoji,
        lead,
        group,
        error: String::new(),
    };
    if let Err(error) = draft.input() {
        return Ok(failed("invalid", error));
    }
    rotate_and_complete(cx, caller, draft, Some(password)).await
}

pub(crate) fn resume(cx: &Cx, handle: &str) -> Result<Draft, LificError> {
    let caller = context::caller(cx)?;
    let account = crate::api::require_user(&caller.identity)?;
    app_context::<DraftStore>(cx).consume(handle, account.id)
}

pub(crate) fn take_notice(cx: &Cx, handle: &str) -> Result<String, LificError> {
    resume(cx, handle).map(|draft| draft.error)
}

#[cfg(test)]
mod reservation_tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::{
        actor::{ActorCtx, Transport},
        db::{
            models::{CreateProject, CreateProjectGroup},
            queries,
        },
        realtime::RealtimeEvent,
    };

    #[test]
    fn group_write_requires_reserved_storage_for_the_same_account() {
        let store = DraftStore::default();
        assert!(ensure_reservation(1, Some(5), &None).is_err());
        assert!(ensure_reservation(1, None, &None).is_ok());
        let reservation = Some(store.reserve(1, Draft::default()).unwrap());
        assert!(ensure_reservation(2, Some(5), &reservation).is_err());
        assert!(ensure_reservation(1, Some(5), &reservation).is_ok());
    }

    #[tokio::test]
    async fn committed_creation_keeps_one_event_and_recoverable_group_warning_after_ten_minutes() {
        let (db, _, _, _, owner, other, _) = crate::api::test_helpers::setup_membership_test();
        let hub = RealtimeHub::new();
        let mut events = hub.subscribe();
        let identity = Some(crate::auth::fresh_identity(&owner, Transport::Web));
        let foreign = queries::project_groups::create_group(
            &db.write().unwrap(),
            other.id,
            &CreateProjectGroup {
                name: "Foreign".into(),
            },
        )
        .unwrap();
        let store = DraftStore::default();
        let previous = Instant::now()
            .checked_sub(Duration::from_secs(601))
            .expect("clock supports the prior reservation instant");
        let reservation = Some(
            store
                .reserve_at(owner.id, Draft::default(), previous)
                .unwrap(),
        );
        ensure_reservation(owner.id, Some(foreign.id), &reservation).unwrap();
        let project = crate::actor::scope(
            ActorCtx {
                user_id: Some(owner.id),
                transport: Transport::Web,
            },
            async {
                crate::services::projects::create_project(
                    &db,
                    &hub,
                    &identity,
                    None,
                    CreateProject {
                        name: "Created once".into(),
                        identifier: "WAIT".into(),
                        ..Default::default()
                    },
                )
                .unwrap()
            },
        )
        .await;
        // Another operation runs normal TTL pruning while this committed
        // create still owns its reserved slot. No wall-clock sleep is needed.
        let unrelated = store.preserve(other.id, Draft::default()).unwrap();
        let error = crate::services::project_form::assign_created_project(
            &db, &hub, &identity, project.id, foreign.id,
        )
        .unwrap_err();
        let warning = format!(
            "Project created, but it wasn't added to the group: {}",
            failed("invalid", error).1
        );
        let outcome = finish_committed("/WAIT/overview".into(), Some(warning.clone()), reservation);
        assert_eq!(outcome.0, "created");
        assert_eq!(outcome.2, warning);
        assert!(outcome.1.starts_with("/WAIT/overview?notice="));
        assert!(outcome.1.ends_with("&group_warning=1"));
        let handle = outcome
            .1
            .split_once("?notice=")
            .unwrap()
            .1
            .split('&')
            .next()
            .unwrap();
        // Native overview uses this same-account, one-shot consumption.
        // Its real fresh-cookie boundary remains a separate HTTP contract.
        assert!(store.consume(handle, other.id).is_err());
        assert_eq!(store.consume(handle, owner.id).unwrap().error, warning);
        assert!(store.consume(handle, owner.id).is_err());
        assert!(store.consume(&unrelated, other.id).is_ok());
        assert_eq!(
            queries::get_project(&db.read().unwrap(), project.id)
                .unwrap()
                .identifier,
            "WAIT"
        );
        let audits: i64 = db.read().unwrap().query_row(
            "SELECT COUNT(*) FROM audit_log WHERE entity_type='project' AND entity_id=?1 AND action='create' AND actor_user_id=?2 AND transport='web'",
            rusqlite::params![project.id, owner.id], |row| row.get(0),
        ).unwrap();
        assert_eq!(audits, 1);
        assert_eq!(
            events.try_recv().unwrap().event,
            RealtimeEvent::ProjectCreated {
                project_id: project.id
            }
        );
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn committed_success_carries_group_warning_even_if_notice_publication_fails() {
        let store = DraftStore::default();
        let reservation = Some(store.reserve(1, Draft::default()).unwrap());
        let poisoned = store;
        std::thread::spawn(move || poisoned.poison_for_test())
            .join()
            .unwrap_err();
        let warning = "Project created, but it wasn't added to the group.".to_owned();
        let outcome = finish_committed("/DONE/overview".into(), Some(warning.clone()), reservation);
        assert_eq!(
            outcome,
            (
                "created".into(),
                "/DONE/overview?group_warning=1".into(),
                warning
            )
        );
    }
}
