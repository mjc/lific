use std::future::Future;

use topcoat::{
    context::{Cx, app_context},
    router::{HeaderMap, HeaderValue, request::headers},
};

use crate::{
    actor::{ActorCtx, Transport},
    auth::AuthState,
    db::{DbPool, models::Project},
    error::LificError,
    resolve_caller::ResolvedIdentity,
};

/// Current authority for one render or action. Resolve it again on the next call.
/// Connected renders retain handshake credentials and need separate socket
/// replacement and revocation handling before they can serve live subscriptions.
pub(crate) struct Caller {
    pub(crate) identity: Option<ResolvedIdentity>,
    pub(crate) session_token: Option<String>,
}

impl Caller {
    /// Let existing session-only and recent-authentication gates consume the
    /// verified browser credential, including one supplied through its cookie.
    pub(crate) fn session_headers(&self) -> Result<HeaderMap, LificError> {
        let mut headers = HeaderMap::new();
        if let Some(token) = &self.session_token {
            let bearer = HeaderValue::from_str(&format!("Bearer {token}")).map_err(|error| {
                LificError::Internal(format!(
                    "failed to construct session authorization: {error}"
                ))
            })?;
            headers.insert("authorization", bearer);
        }
        Ok(headers)
    }

    pub(crate) async fn scope<F: Future>(&self, future: F) -> F::Output {
        let user_id = self
            .session_token
            .as_ref()
            .and_then(|_| self.identity.as_ref().map(|identity| identity.user.id));
        crate::actor::scope(
            ActorCtx {
                user_id,
                transport: Transport::Web,
            },
            future,
        )
        .await
    }
}

pub(crate) fn db(cx: &Cx) -> &DbPool {
    &app_context::<AuthState>(cx).db
}

/// Read credentials from this invocation rather than memoizing an identity in
/// `Cx`; a retained context must still observe revocation and changed authority.
pub(crate) fn caller(cx: &Cx) -> Result<Caller, LificError> {
    let auth = app_context::<AuthState>(cx);
    let Some(token) = crate::auth::browser_session_token(headers(cx))? else {
        if auth.required {
            return Err(LificError::Forbidden("authentication required".into()));
        }
        return Ok(Caller {
            identity: crate::resolve_caller::resolve_caller(&auth.db, None, Transport::Web)?,
            session_token: None,
        });
    };
    let user = {
        let conn = auth.db.read()?;
        match crate::db::queries::users::validate_session(&conn, &token) {
            Ok(user) => user,
            Err(LificError::BadRequest(message))
                if message == crate::db::queries::users::INVALID_SESSION_MESSAGE =>
            {
                return Err(LificError::Forbidden("authentication required".into()));
            }
            Err(error) => return Err(error),
        }
    };
    Ok(Caller {
        identity: Some(crate::auth::fresh_identity(&user, Transport::Web)),
        session_token: Some(token),
    })
}

/// Published reads use the existing publication check and public projections in
/// one snapshot. They never resolve an anonymous visitor as an operator.
pub(crate) fn with_published<T>(
    cx: &Cx,
    identifier: &str,
    read: impl FnOnce(&rusqlite::Connection, &Project) -> Result<T, LificError>,
) -> Result<T, LificError> {
    crate::api::public::with_public(db(cx), identifier, read)
}

#[cfg(test)]
mod tests {
    use topcoat::{context::CxTestBuilder, router::request::Request};

    use super::*;
    use crate::{
        actor::Transport,
        auth::AuthState,
        db::{
            self,
            models::{CreateProject, CreateUser},
            queries,
        },
    };

    fn seed_user(db: &db::DbPool, username: &str, admin: bool) -> (i64, String) {
        let conn = db.write().unwrap();
        let user = queries::users::create_user(
            &conn,
            &CreateUser {
                username: username.into(),
                email: format!("{username}@test.local"),
                password: "testpassword1".into(),
                display_name: None,
                is_admin: admin,
                is_bot: false,
            },
        )
        .unwrap();
        let session = queries::users::create_session(&conn, user.id, None).unwrap();
        (user.id, session.token)
    }

    fn cx(db: &db::DbPool, required: bool, cookie: Option<&str>, bearer: Option<&str>) -> Cx {
        let mut request = Request::new(());
        if let Some(cookie) = cookie {
            request
                .headers_mut()
                .insert("cookie", cookie.parse().unwrap());
        }
        if let Some(bearer) = bearer {
            request
                .headers_mut()
                .insert("authorization", bearer.parse().unwrap());
        }
        let (parts, ()) = request.into_parts();
        CxTestBuilder::new()
            .app_context(AuthState {
                db: db.clone(),
                public_url: "https://test.local".into(),
                required,
            })
            .request_context(parts)
            .build()
    }

    fn cookie(token: &str) -> String {
        format!("other=value; lific_token={token}")
    }

    #[test]
    fn caller_revalidates_the_same_request_after_authority_changes_and_revocation() {
        let db = db::open_memory().unwrap();
        let (id, token) = seed_user(&db, "member", false);
        let cx = cx(&db, true, Some(&cookie(&token)), None);
        assert!(!caller(&cx).unwrap().identity.unwrap().user.is_admin);

        db.write()
            .unwrap()
            .execute("UPDATE users SET is_admin = 1 WHERE id = ?1", [id])
            .unwrap();
        assert!(caller(&cx).unwrap().identity.unwrap().user.is_admin);

        queries::users::delete_session(&db.write().unwrap(), &token).unwrap();
        assert!(matches!(caller(&cx), Err(LificError::Forbidden(_))));
    }

    #[test]
    fn caller_refuses_expired_and_deactivated_session_credentials() {
        let db = db::open_memory().unwrap();
        let (id, token) = seed_user(&db, "member", false);
        let cx = cx(&db, true, Some(&cookie(&token)), None);
        assert!(caller(&cx).is_ok());
        db.write()
            .unwrap()
            .execute("UPDATE users SET is_active = 0 WHERE id = ?1", [id])
            .unwrap();
        assert!(matches!(caller(&cx), Err(LificError::Forbidden(_))));
        db.write()
            .unwrap()
            .execute("UPDATE users SET is_active = 1 WHERE id = ?1", [id])
            .unwrap();
        db.write()
            .unwrap()
            .execute(
                "UPDATE sessions SET expires_at = '2000-01-01' WHERE user_id = ?1",
                [id],
            )
            .unwrap();
        assert!(matches!(caller(&cx), Err(LificError::Forbidden(_))));
    }

    #[test]
    fn explicit_header_identity_wins_and_invalid_headers_cannot_use_a_live_cookie() {
        let db = db::open_memory().unwrap();
        let (_, first) = seed_user(&db, "first", false);
        let (second_id, second) = seed_user(&db, "second", false);
        let cookie = cookie(&first);
        let header = format!("Bearer {second}");
        let cx = cx(&db, true, Some(&cookie), Some(&header));
        assert_eq!(caller(&cx).unwrap().identity.unwrap().user.id, second_id);

        for header in [
            "Bearer lific_sess_invalid",
            "Bearer lific_sk_invalid",
            "Basic invalid",
        ] {
            let cx = self::cx(&db, false, Some(&cookie), Some(header));
            assert!(matches!(caller(&cx), Err(LificError::Forbidden(_))));
        }
    }

    #[tokio::test]
    async fn optional_auth_operator_keeps_web_actor_anonymous_and_bad_cookies_fail_closed() {
        let db = db::open_memory().unwrap();
        let (admin_id, _) = seed_user(&db, "admin", true);
        assert!(matches!(
            caller(&cx(&db, true, None, None)),
            Err(LificError::Forbidden(_))
        ));
        let operator = caller(&cx(&db, false, None, None)).unwrap();
        assert_eq!(operator.identity.as_ref().unwrap().user.id, admin_id);
        let actor = operator.scope(async { crate::actor::current() }).await;
        assert_eq!(actor.user_id, None);
        assert_eq!(actor.transport, Transport::Web);
        for value in ["lific_token=lific_sess_invalid", "lific_token=invalid"] {
            assert!(matches!(
                caller(&cx(&db, false, Some(value), None)),
                Err(LificError::Forbidden(_))
            ));
        }
    }

    #[tokio::test]
    async fn cookie_session_supplies_existing_recent_auth_headers_and_web_actor_scope() {
        let db = db::open_memory().unwrap();
        let (id, token) = seed_user(&db, "member", false);
        let caller = caller(&cx(&db, true, Some(&cookie(&token)), None)).unwrap();
        let headers = caller.session_headers().unwrap();
        let recent_token = crate::auth::recent_session_token(&headers).unwrap();
        assert_eq!(recent_token, token);
        assert_eq!(
            crate::auth::revalidate_recent_session(&db.read().unwrap(), &recent_token, id)
                .unwrap()
                .id,
            id
        );
        let actor = caller.scope(async { crate::actor::current() }).await;
        assert_eq!(actor.user_id, Some(id));
        assert_eq!(actor.transport, Transport::Web);
    }

    #[test]
    fn published_reads_stay_anonymous_and_recheck_publication_in_one_snapshot() {
        let db = db::open_memory().unwrap();
        seed_user(&db, "admin", true);
        let public_id = {
            let conn = db.write().unwrap();
            let public = queries::create_project(
                &conn,
                &CreateProject {
                    name: "Public".into(),
                    identifier: "PUB".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            queries::create_project(
                &conn,
                &CreateProject {
                    name: "Private".into(),
                    identifier: "PRV".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            conn.execute(
                "UPDATE projects SET is_public = 1 WHERE id = ?1",
                [public.id],
            )
            .unwrap();
            public.id
        };
        let cx = cx(&db, false, None, Some("Bearer invalid"));
        let id = with_published(&cx, "PUB", |conn, project| {
            assert!(!conn.is_autocommit());
            assert_eq!(project.lead_user_id, None);
            Ok(project.id)
        })
        .unwrap();
        assert_eq!(id, public_id);
        for identifier in ["PRV", "MISS"] {
            let result = with_published(&cx, identifier, |_, _| -> Result<(), LificError> {
                panic!("unpublished reader ran")
            });
            assert!(matches!(result, Err(LificError::NotFound(message)) if message == "not found"));
        }
        db.write()
            .unwrap()
            .execute(
                "UPDATE projects SET is_public = 0 WHERE id = ?1",
                [public_id],
            )
            .unwrap();
        assert!(matches!(
            with_published(&cx, "PUB", |_, project| Ok(project.id)),
            Err(LificError::NotFound(_))
        ));
    }
}
