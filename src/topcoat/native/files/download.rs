//! Mounted same-origin binary delivery for native Files rows.
use topcoat::{
    context::{Cx, app_context},
    router::{Body, path_param, request::headers, response::Response, route},
};

use super::super::{context, session};

path_param!(attachment_id);

#[route(GET "/__native_files/download/{attachment_id}")]
async fn download(cx: &Cx) -> topcoat::Result<Response> {
    let caller = session::read(cx, context::caller(cx))?;
    session::read(cx, crate::api::require_user(&caller.identity))?;
    let attachment_id = path_param::<AttachmentId>(cx)
        .parse::<i64>()
        .map_err(|_| topcoat::router::error::not_found())?;
    let store = app_context::<crate::storage::AttachmentStore>(cx).clone();
    let response = session::read(
        cx,
        crate::services::files::download_response(
            context::db(cx),
            &store,
            &caller.identity,
            attachment_id,
            &headers(cx),
        ),
    )?;
    Ok(response.map(Body::new))
}
