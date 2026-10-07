//! Shared quota and authority admission for native runtime WebSockets.
//!
//! Register after RuntimeLayer so this runs before its early upgrade return.
//! Published scope validates publication without resolving private credentials.

use std::sync::Arc;

use topcoat::{
    context::{Cx, app_context},
    router::{Body, Layer, LayerFuture, Next, Path, request::uri, response::Response},
};

use crate::{error::LificError, realtime::RealtimeHub};

pub(crate) struct SocketAdmission;

impl Layer for SocketAdmission {
    fn path(&self) -> Option<&Path> {
        None
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        Box::pin(async move {
            if !super::super::runtime::requests_runtime_socket(cx) {
                return next.run(cx, body).await;
            }
            let hub = app_context::<RealtimeHub>(cx);
            let path = uri(cx).path();
            if path.starts_with("/public/") {
                use super::public_route::Route;
                let project = match super::public_route::resolve(path) {
                    Some(
                        Route::Issues { project }
                        | Route::Board { project }
                        | Route::IssueDetail { project, .. }
                        | Route::Pages { project }
                        | Route::PageDetail { project, .. },
                    ) => project,
                    Some(Route::Redirect(_)) | None => {
                        return Err(topcoat::router::error::not_found().into());
                    }
                };
                super::session::read(
                    cx,
                    super::context::with_published(cx, &project, |_, _| Ok(())),
                )?;
                let Some(permit) = hub.try_acquire_published_socket() else {
                    return connection_limit();
                };
                let scope = super::super::runtime::SocketRunPolicy::new(|_, uri| {
                    super::public_route::resolve(uri.path()).is_some()
                });
                let socket_context = cx.with(Arc::new(permit)).with(scope);
                return next.run(&socket_context, body).await;
            }

            // Subscribe before authorization so admission cannot miss retirement.
            let revocations = super::session::subscribe_revocations(cx);
            // Resolve fresh request credentials, before the runtime captures them.
            // Private optional local-operator behavior stays in the existing caller.
            let user = super::context::caller(cx)
                .and_then(|caller| crate::api::require_user(&caller.identity))
                .map_err(|error| match error {
                    LificError::Forbidden(_) => topcoat::router::error::forbidden().into(),
                    error => topcoat::Error::from(error),
                })?;
            let Some(permit) = hub.try_acquire_socket(user.id) else {
                return connection_limit();
            };
            // The generic runtime patch retains this child context through its
            // raw socket task, including idle-before-first-render and failed upgrade.
            let session_context = cx.with(Arc::new(permit));
            let lifetime = super::session::socket_lifetime(
                &session_context,
                revocations,
                user.id,
                user.is_admin,
            );
            let socket_context = session_context.with(lifetime);
            next.run(&socket_context, body).await
        })
    }
}

fn connection_limit() -> topcoat::Result<Response> {
    Ok(Response::builder()
        .status(429)
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"error":"websocket connection limit reached"}"#,
        ))?)
}
