//! Session outcomes shared by native pages and connected reads.

use topcoat::{
    context::Cx,
    runtime::{Event, connected_untracked, expr},
    view::Attributes,
};

use crate::error::LificError;

pub(crate) fn read<T>(cx: &Cx, result: Result<T, LificError>) -> topcoat::Result<T> {
    match result {
        Ok(value) => Ok(value),
        Err(LificError::Forbidden(message)) if message == "authentication required" => {
            // Socket renders bypass the outer HTTP middleware that mounts Location.
            let destination = if connected_untracked(cx) {
                super::transport::mounted_url(cx, "/login")
            } else {
                "/login".to_owned()
            };
            Err(topcoat::router::error::redirect(destination).into())
        }
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn mount(cx: &Cx) -> Attributes {
    let handler = expr!(|_mount: Event| {
        let _storage_handler = |_event: Event| {
            let local = raw!(
                r#"cx.hydrate((() => {
                    try { return ${_event}.storageArea === window.localStorage; }
                    catch { return false; }
                })())"#,
                false
            );
            if local {
                let key = raw!(
                    r#"cx.hydrate({t:"Option",v:${_event}.key})"#,
                    None::<String>
                );
                let before = raw!(
                    "cx.hydrate(JSON.stringify(${_event}.oldValue))",
                    String::new()
                );
                let after = raw!(
                    "cx.hydrate(JSON.stringify(${_event}.newValue))",
                    String::new()
                );
                let reload = if key.is_none() {
                    true
                } else {
                    if key.unwrap() == "lific_token" {
                        before != after
                    } else {
                        false
                    }
                };
                if reload {
                    raw!("window.location.reload()", ());
                }
            }
        };
        raw!(
            "window.addEventListener('storage', ${_storage_handler}, {signal:cx.abortSignal})",
            ()
        );
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
    attributes
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, process::Stdio, sync::Arc};

    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    use topcoat::{
        context::CxTestBuilder,
        router::{RemoteAddr, error::RedirectError, response::IntoResponse},
    };

    use super::*;
    use crate::{
        auth::AuthState,
        db::{self, queries},
        ratelimit::IpNetwork,
    };

    fn context(db: &db::DbPool, required: bool, cookie: Option<&str>, prefix: &str) -> Cx {
        let mut request = axum::http::Request::builder().header("x-forwarded-prefix", prefix);
        if let Some(cookie) = cookie {
            request = request.header("cookie", cookie);
        }
        let (mut parts, ()) = request.body(()).unwrap().into_parts();
        parts
            .extensions
            .insert(RemoteAddr("127.0.0.1:3000".parse::<SocketAddr>().unwrap()));
        let proxies: Arc<[IpNetwork]> = vec![IpNetwork::parse("127.0.0.1").unwrap()].into();
        CxTestBuilder::new()
            .app_context(AuthState {
                db: db.clone(),
                public_url: "https://test.local".into(),
                required,
            })
            .app_context(proxies)
            .request_context(parts)
            .build()
    }

    fn caller(cx: &Cx) -> Result<super::super::context::Caller, LificError> {
        let caller = super::super::context::caller(cx)?;
        crate::api::require_user(&caller.identity)?;
        Ok(caller)
    }

    fn seed_operator(db: &db::DbPool) -> String {
        let conn = db.write().unwrap();
        conn.execute(
            "INSERT INTO users (username, email, password_hash, is_admin, is_bot)
             VALUES ('session_operator', 'session@test.local', 'fixture', 1, 0)",
            [],
        )
        .unwrap();
        queries::users::create_session(&conn, conn.last_insert_rowid(), None)
            .unwrap()
            .token
    }

    fn assert_login<T>(cx: &Cx, result: topcoat::Result<T>) {
        let Err(error) = result else {
            panic!("denied session unexpectedly retained authority");
        };
        let redirect = error
            .downcast_cloned::<RedirectError>()
            .expect("authentication denial must become a browser login redirect");
        let response = redirect.into_response(cx).unwrap();
        assert!(response.status().is_redirection());
        assert_eq!(
            response.headers()["location"],
            "/login",
            "HTTP middleware owns mounting"
        );
    }

    #[test]
    fn native_session_http_denial_keeps_login_logical_for_outer_mounting() {
        let db = db::open_memory().unwrap();
        for prefix in ["", "/app", "/ACC"] {
            let cx = context(&db, true, None, prefix);
            assert_login(&cx, read(&cx, caller(&cx)));
        }
    }

    #[tokio::test]
    async fn native_session_optional_operator_keeps_authority_and_anonymous_web_actor() {
        let db = db::open_memory().unwrap();
        seed_operator(&db);
        let cx = context(&db, false, None, "/app");
        let caller = read(&cx, caller(&cx)).unwrap();
        assert_eq!(
            caller.identity.as_ref().unwrap().user.username,
            "session_operator"
        );
        let actor = caller.scope(async { crate::actor::current() }).await;
        assert_eq!(actor.user_id, None);
        assert_eq!(actor.transport, crate::actor::Transport::Web);
    }

    #[test]
    fn native_session_invalid_and_revoked_credentials_never_fall_back_to_operator() {
        let db = db::open_memory().unwrap();
        let token = seed_operator(&db);
        queries::users::delete_session(&db.write().unwrap(), &token).unwrap();
        for required in [false, true] {
            for cookie in [
                "lific_token=invalid".to_owned(),
                format!("lific_token={token}"),
            ] {
                let cx = context(&db, required, Some(&cookie), "/ACC");
                assert_login(&cx, read(&cx, caller(&cx)));
            }
        }
    }

    #[test]
    fn native_session_non_authentication_errors_remain_errors() {
        let cx = CxTestBuilder::new().build();
        let error =
            read::<()>(&cx, Err(LificError::Internal("session read failed".into()))).unwrap_err();
        assert!(error.to_string().contains("session read failed"));
        assert!(error.downcast_cloned::<RedirectError>().is_err());
        let error = read::<()>(
            &cx,
            Err(LificError::Forbidden(
                "insufficient project permissions".into(),
            )),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("insufficient project permissions")
        );
        assert!(error.downcast_cloned::<RedirectError>().is_err());
        assert_eq!(read(&cx, Ok::<_, LificError>(42)).unwrap(), 42);
    }

    #[tokio::test]
    async fn native_session_browser_replaces_accounts_and_retires_revoked_private_content() {
        let fixture = super::super::home_fixture::fixture();
        let (replacement_token, revocation_tokens) = {
            let conn = fixture.db.write().unwrap();
            let admin = queries::users::get_user_by_username(&conn, "admin").unwrap();
            let viewer = queries::users::get_user_by_username(&conn, "viewer").unwrap();
            let replacement_token = queries::users::create_session(&conn, admin.id, None)
                .unwrap()
                .token;
            let revocation_tokens = (0..3)
                .map(|_| {
                    queries::users::create_session(&conn, viewer.id, None)
                        .unwrap()
                        .token
                })
                .collect::<Vec<_>>();
            (replacement_token, revocation_tokens)
        };
        let arguments = serde_json::json!({
            "replacementToken": replacement_token,
            "revocationTokens": revocation_tokens,
        })
        .to_string();
        let (origin, server) = super::super::home_fixture::serve(&fixture).await;
        let mut command = super::super::home_fixture::browser_command(
            "src/topcoat/native/session.browser.test.cjs",
            &origin,
            &fixture.token,
        );
        let mut child = command
            .arg(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut stderr = child.stderr.take().unwrap();
        let errors = tokio::spawn(async move {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).await.unwrap();
            String::from_utf8_lossy(&bytes).into_owned()
        });
        let mut output = String::new();
        let result = tokio::time::timeout(std::time::Duration::from_secs(120), async {
            while let Some(line) = stdout.next_line().await.unwrap() {
                if let Some(index) = line.strip_prefix("@lific-fixture:revoke:") {
                    let token = revocation_tokens
                        .get(index.parse::<usize>().unwrap())
                        .expect("browser requested an unknown fixture session");
                    queries::users::delete_session(&fixture.db.write().unwrap(), token).unwrap();
                    stdin.write_all(b"revoked\n").await.unwrap();
                } else {
                    output.push_str(&line);
                    output.push('\n');
                }
            }
            child.wait().await.unwrap()
        })
        .await;
        server.abort();
        match result {
            Ok(status) => {
                assert!(status.success(), "{output}\n{}", errors.await.unwrap());
            }
            Err(timeout) => {
                let cleanup = child.kill().await;
                let stderr = errors.await.unwrap();
                panic!(
                    "native session browser timed out: {timeout}; cleanup: {cleanup:?}\n{output}\n{stderr}"
                );
            }
        }
    }
}
