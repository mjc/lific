//! Viewer-authorized native Page downloads.

use topcoat::{
    context::Cx,
    router::{Body, path_param, response::Response, route},
};

use super::super::{context, session};

path_param!(identifier);

#[route(GET "/__native_page_export/{identifier}")]
async fn download(cx: &Cx) -> topcoat::Result<Response> {
    let caller = session::read(cx, context::caller(cx))?;
    session::read(cx, crate::api::require_user(&caller.identity))?;
    let identifier = path_param::<Identifier>(cx).to_owned();
    let response = session::read(
        cx,
        crate::services::export::page(context::db(cx).clone(), &caller.identity, identifier, None)
            .await,
    )?;
    // The body retains the existing temp directory, export slot and stream deadlines.
    Ok(response.map(Body::new))
}
