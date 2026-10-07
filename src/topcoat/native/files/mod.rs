//! Native project Files manager.
mod actions;
mod download;
pub(crate) mod model;
mod view;

#[cfg(test)]
mod production;

use super::super::shell::ParsedRoute;
use super::{context, home_shell, session};
use crate::error::LificError;
use topcoat::{context::Cx, view::BoxView};

pub(crate) fn screen<'a>(cx: &'a Cx, route: &ParsedRoute<'_>) -> topcoat::Result<BoxView<'a>> {
    super::workspace::common_screen(cx, route)
}

pub(super) fn region<'a>(
    cx: &'a Cx,
    route: &ParsedRoute<'_>,
    account: i64,
    caller: &context::Caller,
) -> topcoat::Result<BoxView<'a>> {
    let identifier = route
        .project
        .ok_or_else(topcoat::router::error::not_found)?
        .to_owned();
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return session::read(
            cx,
            Err(LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let query = crate::db::models::ProjectAttachmentQuery {
        limit: Some(model::PAGE_SIZE),
        sort: Some("created_at".into()),
        ..Default::default()
    };
    let snapshot = session::read(
        cx,
        crate::services::files::snapshot(context::db(cx), &caller.identity, &identifier, &query),
    )?;
    let title = home_shell::page_label(route);
    let content = view::content(cx, account, snapshot);
    Ok(home_shell::page_region(cx, content, None, title))
}
