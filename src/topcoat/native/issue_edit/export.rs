//! Viewer-authorized native downloads and their independent toolbar state.

use topcoat::{
    context::Cx,
    router::{Body, path_param, response::Response, route},
};

use super::super::{context, session};

path_param!(identifier);

#[route(GET "/__native_issue_export/{identifier}")]
async fn download(cx: &Cx) -> topcoat::Result<Response> {
    let caller = session::read(cx, context::caller(cx))?;
    session::read(cx, crate::api::require_user(&caller.identity))?;
    let identifier = path_param::<Identifier>(cx).to_owned();
    let response = session::read(
        cx,
        crate::services::export::issue(context::db(cx).clone(), &caller.identity, identifier, None)
            .await,
    )?;
    // The body retains the existing temp directory, slot and stream deadlines.
    Ok(response.map(Body::new))
}

/// Keep the Issue route's established call site while sharing toolbar state.
pub(crate) fn toolbar_fragments<'a>(
    cx: &'a Cx,
    identifier: &str,
) -> (topcoat::view::BoxView<'a>, topcoat::view::BoxView<'a>) {
    super::super::document_export::toolbar_fragments(
        cx,
        super::super::document_export::DocumentKind::Issue,
        identifier,
    )
}
