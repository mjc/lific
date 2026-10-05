//! Shared same-account session confirmation and rotation.

use std::{net::SocketAddr, sync::Arc};

use axum::http::HeaderMap;

use crate::{
    config::AuthConfig,
    db::{
        DbPool,
        models::{Session, User},
    },
    error::LificError,
    resolve_caller::ResolvedIdentity,
};

pub(crate) struct RefreshedSession {
    pub(crate) user: User,
    pub(crate) session: Session,
}

// These are the real request dependencies previously supplied by Axum.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn refresh_session(
    db: &DbPool,
    auth_cfg: &AuthConfig,
    identity: &Option<ResolvedIdentity>,
    peer: SocketAddr,
    trusted_proxies: &[crate::ratelimit::IpNetwork],
    limiter: Option<&Arc<crate::ratelimit::RateLimiter>>,
    headers: &HeaderMap,
    supplied_password: Option<String>,
) -> Result<RefreshedSession, LificError> {
    let caller = crate::api::require_user(identity)?;
    // Shape check only. Which user it names, and whether it is still live, is
    // established inside the transaction below.
    let session_token = crate::auth::session_bearer_token(headers)?;

    // Read the mode and the current hash on a pooled connection so the
    // expensive verify happens with no lock held. Both are re-read
    // authoritatively inside the transaction.
    let (passwordless, current_hash) = {
        let conn = db.read()?;
        let settings = crate::db::queries::settings::get(&conn)?;
        let user = crate::db::queries::users::get_user_by_id(&conn, caller.id)?;
        (
            settings.web_auto_login || !auth_cfg.required,
            user.password_hash,
        )
    };

    // A password-bearing refresh runs Argon2, so it is rate-limited on the
    // same reserve-then-refund terms as login, on its own key namespace: this
    // is an authenticated caller re-proving themselves, and it must not share
    // (or drain) the login budget for the same account. Both slots are taken
    // before the verify, so at most `max_attempts` verifies can be in flight.
    // A passwordless refresh does no expensive work and needs no reservation.
    let ip_key = format!(
        "reauth_ip:{}",
        crate::ratelimit::client_ip(peer.ip(), headers, trusted_proxies)
            .map_err(|_| LificError::Unavailable("invalid proxy identity".into()))?
    );
    let user_key = format!("reauth_user:{}", caller.id);
    let reservation = match (&supplied_password, &limiter) {
        (Some(_), Some(rl)) => {
            match crate::ratelimit::Reservation::acquire(rl, &ip_key, &user_key) {
                Ok(reservation) => Some(reservation),
                Err(rejected) => {
                    let retry = match rejected {
                        crate::ratelimit::ReservationRejection::First => rl.retry_after(&ip_key),
                        crate::ratelimit::ReservationRejection::Second => rl.retry_after(&user_key),
                    };
                    return Err(LificError::BadRequest(
                        crate::ratelimit::retry_after_message(
                            "too many confirmation attempts",
                            retry,
                        ),
                    ));
                }
            }
        }
        _ => None,
    };

    let verified_hash = match supplied_password {
        Some(password) => {
            crate::db::queries::users::reject_oversized_password(&password)?;
            let hash = current_hash.clone();
            let ok = tokio::task::spawn_blocking(move || {
                crate::db::queries::users::verify_password(&password, &hash).unwrap_or(false)
            })
            .await
            .map_err(|e| LificError::Internal(format!("password verification task failed: {e}")))?;
            if !ok {
                return Err(LificError::BadRequest("incorrect password".into()));
            }
            Some(current_hash)
        }
        None => {
            if !passwordless {
                return Err(LificError::BadRequest(
                    "your password is required to confirm this".into(),
                ));
            }
            None
        }
    };

    let (user, session) = db.transaction(|tx| {
        // The presented session must still be live, and must still be this
        // caller's. Not `revalidate_recent_session`: an old session is exactly
        // what this endpoint is for.
        let user =
            crate::db::queries::users::validate_session(tx, &session_token).map_err(|_| {
                LificError::BadRequest(crate::db::queries::users::INVALID_SESSION_MESSAGE.into())
            })?;
        if user.id != caller.id {
            return Err(LificError::BadRequest(
                crate::db::queries::users::INVALID_SESSION_MESSAGE.into(),
            ));
        }
        if !crate::db::queries::users::credential_is_live(tx, &user)? {
            return Err(LificError::BadRequest(
                "this account has been deactivated. Ask an admin to restore it.".into(),
            ));
        }

        let settings = crate::db::queries::settings::get(tx)?;
        match &verified_hash {
            // Password path: the hash verified moments ago must still be the
            // stored one, or the password presented is the old one.
            Some(hash) => {
                if &user.password_hash != hash {
                    return Err(LificError::BadRequest("incorrect password".into()));
                }
            }
            // Passwordless path: re-read the mode, so an admin who has just
            // turned it off wins.
            None => {
                if !settings.web_auto_login && auth_cfg.required {
                    return Err(LificError::BadRequest(
                        "your password is required to confirm this".into(),
                    ));
                }
            }
        }

        // Only the presented session is replaced. Nothing here consults
        // `resolve_caller`, so there is no first-admin fallback to land on:
        // the new session is for `user.id` and can be for nobody else.
        crate::db::queries::users::delete_session(tx, &session_token)?;
        let session = crate::db::queries::users::create_session(
            tx,
            user.id,
            Some(settings.session_lifetime_days * 24),
        )?;
        Ok((user, session))
    })?;

    // The confirmation worked, so it was not an attack: give the slots back.
    // Every failure above returns with the reservation intact, which is what
    // makes repeated wrong passwords hit the limit.
    if let Some(reservation) = reservation {
        reservation.refund();
    }

    Ok(RefreshedSession { user, session })
}

/// Build a Set-Cookie header for the session token with security flags.
///
/// LIF-207: `secure` gates the `Secure` attribute. It's on by default and only
/// disabled for an explicitly-`http://` deployment, because browsers silently
/// drop a `Secure` cookie over plain HTTP — which would break the OAuth approve
/// flow (the one place the cookie is actually read) on a local-first install.
pub(crate) fn session_cookie(token: &str, expires_at: &str, secure: bool) -> String {
    use chrono::DateTime;
    // Parse expiry for Max-Age calculation; fall back to 30 days
    let max_age = DateTime::parse_from_rfc3339(expires_at).map_or(30 * 24 * 3600, |exp| {
        let exp_utc: DateTime<chrono::Utc> = exp.into();
        (exp_utc - chrono::Utc::now()).num_seconds().max(0)
    });

    let secure_attr = if secure { "; Secure" } else { "" };
    format!("lific_token={token}; Path=/; Max-Age={max_age}; HttpOnly{secure_attr}; SameSite=Lax")
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
