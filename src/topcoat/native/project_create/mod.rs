//! Native ProjectNew page and its Rust-authored controls and commands.

mod actions;
mod draft;
mod form;
mod model;
mod picker;
mod picker_controls;
mod select;

#[cfg(test)]
mod production;

pub(crate) use actions::take_notice;
pub(crate) use draft::DraftStore;
pub(crate) use picker_controls::picker_with_attributes as icon_picker;
pub(crate) const STYLESHEET: &str = include_str!("form.css");

use topcoat::{context::Cx, runtime::signal, view::BoxView};

use super::super::shell::ParsedRoute;
use super::{avatar, context, home_shell, session};

pub(crate) fn screen<'a>(cx: &'a Cx, route: &ParsedRoute<'_>) -> topcoat::Result<BoxView<'a>> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    let projects = session::read(
        cx,
        crate::services::projects::list_visible_projects(context::db(cx), &caller.identity),
    )?;
    let (leads, groups) = session::read(
        cx,
        crate::services::project_form::form_catalog(context::db(cx), &caller.identity),
    )?;
    let mut lead_rows = vec![select::OptionRow::empty("No lead")];
    lead_rows.extend(leads.into_iter().map(|user| {
        let label = if user.display_name.is_empty() {
            user.username.clone()
        } else {
            user.display_name
        };
        select::OptionRow {
            value: Some(user.id),
            initials: avatar::initials(&label),
            label,
            username: user.username,
            admin: user.is_admin,
            created_at: user.created_at,
        }
    }));
    let mut group_rows = vec![select::OptionRow::empty("No group")];
    group_rows.extend(groups.into_iter().map(|group| select::OptionRow {
        value: Some(group.id),
        label: group.name,
        ..select::OptionRow::empty("")
    }));
    let draft = match query_handle(cx, "resume") {
        Some(handle) => session::read(cx, actions::resume(cx, &handle))?,
        None => model::Draft::default(),
    };
    // Archive capabilities require a real human cookie session and current admin.
    let is_bot =
        crate::db::queries::users::get_user_by_id(&*context::db(cx).read()?, user.id)?.is_bot;
    let can_import = caller.session_token.is_some() && user.is_admin && !is_bot;
    let (content, header) = form::views(cx, user.id, draft, lead_rows, group_rows, can_import);
    home_shell::shell_with_palette_for_page_and_topbar(
        cx,
        &user,
        &projects,
        route,
        content,
        signal(cx, || false),
        Some(header),
    )
}

// Handles are fixed lowercase hexadecimal strings, not arbitrary user content.
// Reject malformed or repeated keys rather than partially interpreting a query.
fn query_handle(cx: &Cx, key: &str) -> Option<String> {
    let query = topcoat::router::request::uri(cx).query()?;
    let mut entries = query
        .split('&')
        .filter_map(|entry| entry.split_once('='))
        .filter(|(name, _)| *name == key);
    let (_, handle) = entries.next()?;
    if entries.next().is_some()
        || handle.len() != 48
        || !handle
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    Some(handle.to_owned())
}
