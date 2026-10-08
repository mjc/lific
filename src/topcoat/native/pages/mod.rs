//! Native Pages browsing and project page detail.
mod actions;
mod activity;
mod detail_presentation;
mod editor_state;
mod export;
mod folder_create;
mod folder_tree;
mod labels;
pub(crate) mod labels_action;
mod metadata;
mod move_picker;
mod pin;
mod status;
mod title_editor;
mod view;

#[cfg(test)]
mod export_production;
#[cfg(test)]
mod production;
#[cfg(test)]
mod save_lifecycle_production;
#[cfg(test)]
mod title_commit_production;

use super::super::shell::{Page, ParsedRoute};
use super::{context, home_shell, session};
use crate::{db::queries, error::LificError};
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
        .ok_or_else(topcoat::router::error::not_found)?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return session::read(
            cx,
            Err(LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let project = session::read(
        cx,
        (|| {
            let conn = context::db(cx).read()?;
            let id = queries::resolve_project_identifier(&conn, identifier)?;
            queries::get_project(&conn, id)
        })(),
    )?;
    let content = match route.page {
        Page::Pages => view::list(cx, identifier, project.id, &caller.identity, account),
        Page::Record(page_id) => view::detail(cx, identifier, page_id, &caller.identity, account),
        _ => Err(topcoat::router::error::not_found().into()),
    }?;
    Ok(home_shell::page_region(
        cx,
        content,
        None,
        home_shell::page_label(route),
    ))
}
