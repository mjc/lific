//! Session outcomes shared by native pages and connected reads.

use topcoat::{
    context::{Cx, app_context},
    runtime::{Event, connected_untracked, expr, procedure, signal},
    view::{Attributes, BoxView, ViewExt, emit, live},
};

use crate::error::LificError;
use tokio::sync::broadcast::{Receiver, error::RecvError};

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
        Err(LificError::Forbidden(_)) => Err(topcoat::router::error::forbidden().into()),
        Err(LificError::BadRequest(message)) => {
            Err(topcoat::router::error::bad_request(message).into())
        }
        Err(LificError::NotFound(_)) => Err(topcoat::router::error::not_found().into()),
        Err(LificError::TooManyRequests(_)) => {
            Err(topcoat::router::error::too_many_requests(30).into())
        }
        Err(LificError::PayloadTooLarge(_)) => {
            Err(topcoat::router::error::content_too_large().into())
        }
        Err(LificError::Unavailable(_)) => {
            Err(topcoat::router::error::service_unavailable(2).into())
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

fn current_account(cx: &Cx) -> Result<Option<(i64, bool)>, LificError> {
    match super::context::caller(cx).and_then(|caller| crate::api::require_user(&caller.identity)) {
        Ok(user) => Ok(Some((user.id, user.is_admin))),
        Err(LificError::Forbidden(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Register before the render's fresh authorization read so no retirement is missed.
pub(crate) fn subscribe_revocations(cx: &Cx) -> Receiver<i64> {
    app_context::<crate::realtime::RealtimeHub>(cx).subscribe_revocations()
}

/// The content connection owns this future and drops its receiver on replacement.
pub(crate) fn revocation_lifetime(
    cx: &Cx,
    mut revoked: Receiver<i64>,
    account_id: i64,
    is_admin: bool,
    connected: bool,
) -> BoxView<'static> {
    let context = cx.clone();
    live! { cx =>
        let token = emit! { <span hidden="hidden"></span> }?;
        if !connected {
            return Ok(token);
        }
        loop {
            let retire = match revoked.recv().await {
                Ok(user_id) => user_id == account_id,
                Err(RecvError::Lagged(_)) => {
                    // Lost notifications require fresh bound-credential authority.
                    // An unavailable authority cannot keep private content live.
                    current_account(&context)
                        .map_or(true, |account| account != Some((account_id, is_admin)))
                }
                Err(RecvError::Closed) => true,
            };
            if retire {
                // Socket headers retain their original cookie. A fresh document
                // can recover a replacement cookie or reach the existing login boundary.
                return Err(topcoat::router::error::redirect(
                    super::transport::mounted_url(&context, "/"),
                ).into());
            }
        }
    }
    .boxed()
}

#[procedure("/__native_home/session")]
pub(super) async fn native_home_session(cx: &Cx) -> topcoat::Result<(Option<i64>, bool)> {
    // Pinned Topcoat cannot serialize a tuple nested inside Option by reference.
    // Absence is explicit; the flag carries no authority when the ID is absent.
    Ok(match current_account(cx)? {
        Some((id, is_admin)) => (Some(id), is_admin),
        None => (None, false),
    })
}

/// Compare fresh HTTP authority with the account that rendered the whole Home.
/// Connected shard credentials and browser storage cannot replace this baseline.
pub(crate) fn account_mount(cx: &Cx, account_id: i64, is_admin: bool) -> Attributes {
    let busy = signal(cx, || false);
    let pending = signal(cx, || false);
    let revision = signal(cx, || 0usize);
    let failed_busy = busy.clone();
    let failed_pending = pending.clone();
    let failed_revision = revision.clone();
    let ready_pending = pending.clone();
    let ready_revision = revision.clone();
    let request_busy = busy.clone();
    let request_pending = pending.clone();
    let request_revision = revision.clone();
    let check_pending = pending.clone();
    let disposed_busy = busy.clone();
    let disposed_pending = pending.clone();
    let disposed_revision = revision.clone();
    let handler = expr!(|_mount: Event| {
        let _dispose = || {
            disposed_revision.increment();
            disposed_busy.set(false);
            disposed_pending.set(false);
        };
        let _focus = |_event: Event| {
            if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                if busy.get() {
                    pending.set(true);
                } else {
                    busy.set(true);
                    pending.set(true);
                    revision.increment();
                    let sent_revision = revision.get();
                    let _failed = || {
                        if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                            if failed_revision.get() == sent_revision {
                                // Consume an existing focus; never retry failure on its own.
                                if !failed_pending.get() {
                                    failed_busy.set(false);
                                }
                            }
                        }
                    };
                    let _ready = || {
                        if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                            false
                        } else {
                            if ready_revision.get() != sent_revision {
                                false
                            } else {
                                ready_pending.get()
                            }
                        }
                    };
                    let _request = async || {
                        // Disposal may happen before the promise adapter starts this task.
                        if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                            if request_revision.get() == sent_revision {
                                let current = native_home_session().await;
                                if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                                    if request_revision.get() == sent_revision {
                                        // A newer focus needs fresh cookies, regardless of this result.
                                        if !request_pending.get() {
                                            let reload = if current.0.is_none() {
                                                true
                                            } else {
                                                let current_id = current.0.unwrap();
                                                if current_id != account_id {
                                                    true
                                                } else {
                                                    current.1 != is_admin
                                                }
                                            };
                                            if reload {
                                                // Keep occupied until navigation retires its owner.
                                                raw!("window.location.reload()", ());
                                            } else {
                                                request_busy.set(false);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    };
                    let _check = async || {
                        while raw!("${_ready}()", false) {
                            check_pending.set(false);
                            raw!(
                                "await Promise.resolve().then(() => ${_request}()).catch(() => ${_failed}());",
                                ()
                            );
                            // Keep control flow out of the macro's trailing-expression return.
                            let _iteration_complete = false;
                        }
                        let _complete = false;
                    };
                    raw!("Promise.resolve().then(() => ${_check}());", ());
                }
            }
        };
        raw!(
            "cx.abortSignal.addEventListener('abort', ${_dispose}, {once:true});",
            ()
        );
        raw!(
            "window.addEventListener('focus', ${_focus}, {signal:cx.abortSignal});",
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

    #[test]
    fn native_session_account_check_reads_the_current_cookie_and_admin_flag() {
        let db = db::open_memory().unwrap();
        let admin_token = seed_operator(&db);
        let (admin_id, viewer_id, viewer_token) = {
            let conn = db.write().unwrap();
            let admin_id = queries::users::validate_session(&conn, &admin_token)
                .unwrap()
                .id;
            conn.execute(
                "INSERT INTO users (username, email, password_hash, is_admin, is_bot)
                 VALUES ('session_viewer', 'viewer@test.local', 'fixture', 0, 0)",
                [],
            )
            .unwrap();
            let viewer_id = conn.last_insert_rowid();
            let viewer_token = queries::users::create_session(&conn, viewer_id, None)
                .unwrap()
                .token;
            (admin_id, viewer_id, viewer_token)
        };
        let admin = context(
            &db,
            true,
            Some(&format!("lific_token={admin_token}")),
            "/ACC",
        );
        let viewer = context(
            &db,
            true,
            Some(&format!("lific_token={viewer_token}")),
            "/ACC",
        );
        assert_eq!(current_account(&admin).unwrap(), Some((admin_id, true)));
        assert_eq!(current_account(&viewer).unwrap(), Some((viewer_id, false)));
        db.write()
            .unwrap()
            .execute("UPDATE users SET is_admin = 0 WHERE id = ?1", [admin_id])
            .unwrap();
        assert_eq!(current_account(&admin).unwrap(), Some((admin_id, false)));
    }

    #[test]
    fn native_session_account_check_returns_denial_without_operator_fallback() {
        let db = db::open_memory().unwrap();
        let token = seed_operator(&db);
        assert_eq!(
            current_account(&context(&db, true, None, "/app")).unwrap(),
            None
        );
        assert!(
            current_account(&context(&db, false, None, "/app"))
                .unwrap()
                .is_some()
        );
        queries::users::delete_session(&db.write().unwrap(), &token).unwrap();
        for required in [false, true] {
            for cookie in [
                "lific_token=invalid".to_owned(),
                format!("lific_token={token}"),
            ] {
                assert_eq!(
                    current_account(&context(&db, required, Some(&cookie), "/app")).unwrap(),
                    None
                );
            }
        }
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
                .clone()
                .downcast_cloned::<topcoat::router::error::ForbiddenError>()
                .is_ok()
        );
        assert!(error.downcast_cloned::<RedirectError>().is_err());
        assert_eq!(read(&cx, Ok::<_, LificError>(42)).unwrap(), 42);
    }

    #[test]
    fn native_session_http_domain_denials_keep_their_status_instead_of_becoming_500() {
        let cx = CxTestBuilder::new().build();
        for (error, expected, retry) in [
            (
                LificError::Forbidden("insufficient project permissions".into()),
                403,
                None,
            ),
            (
                LificError::BadRequest("invalid identifier".into()),
                400,
                None,
            ),
            (LificError::NotFound("missing issue".into()), 404, None),
            (
                LificError::TooManyRequests("budget exhausted".into()),
                429,
                Some("30"),
            ),
            (
                LificError::PayloadTooLarge("resource ceiling".into()),
                413,
                None,
            ),
            (
                LificError::Unavailable("store occupied".into()),
                503,
                Some("2"),
            ),
        ] {
            let error = read::<()>(&cx, Err(error)).unwrap_err();
            assert!(error.clone().downcast_cloned::<RedirectError>().is_err());
            let response = error.into_response(&cx).unwrap();
            assert_eq!(response.status().as_u16(), expected);
            assert_eq!(
                response
                    .headers()
                    .get("retry-after")
                    .map(|value| value.to_str().unwrap()),
                retry
            );
        }
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
