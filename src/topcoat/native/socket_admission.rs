//! Shared quota admission for private native runtime WebSockets.
//!
//! Register after RuntimeLayer so this runs before its early upgrade return.
//! Published scope bypasses private identity resolution; its credential-free
//! transport policy needs separate implementation and acceptance proof.

use std::sync::Arc;

use topcoat::{
    context::{Cx, app_context},
    router::{
        Body, Layer, LayerFuture, Method, Next, Path,
        request::{headers, method, uri},
        response::Response,
    },
    runtime::RUNTIME_PROTOCOL,
};

use crate::{error::LificError, realtime::RealtimeHub};

pub(crate) struct SocketAdmission;

impl Layer for SocketAdmission {
    fn path(&self) -> Option<&Path> {
        None
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        Box::pin(async move {
            let runtime_socket = *method(cx) == Method::GET
                && headers(cx)
                    .get_all("sec-websocket-protocol")
                    .iter()
                    .filter_map(|value| value.to_str().ok())
                    .flat_map(|protocols| protocols.split(','))
                    .any(|protocol| protocol.trim() == RUNTIME_PROTOCOL);
            if !runtime_socket {
                return next.run(cx, body).await;
            }
            let route = super::super::shell::ParsedRoute::parse(uri(cx).path());
            if matches!(route.layout, super::super::shell::Layout::Public) {
                return next.run(cx, body).await;
            }

            // Resolve fresh request credentials, before the runtime captures them.
            // Private optional local-operator behavior stays in the existing caller.
            let user = super::context::caller(cx)
                .and_then(|caller| crate::api::require_user(&caller.identity))
                .map_err(|error| match error {
                    LificError::Forbidden(_) => topcoat::router::error::forbidden().into(),
                    error => topcoat::Error::from(error),
                })?;
            let hub = app_context::<RealtimeHub>(cx);
            let Some(permit) = hub.try_acquire_socket(user.id) else {
                return Ok(Response::builder()
                    .status(429)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"error":"websocket connection limit reached"}"#,
                    ))?);
            };
            // The generic runtime patch retains this child context through its
            // raw socket task, including idle-before-first-render and failed upgrade.
            let socket_context = cx.with(Arc::new(permit));
            next.run(&socket_context, body).await
        })
    }
}
