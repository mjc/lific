//! Native account Settings page.

mod actions;
mod appearance;
mod profile;
mod security;
mod templates;
mod tools;
mod view;

#[cfg(test)]
mod appearance_production;
#[cfg(test)]
mod production;
#[cfg(test)]
mod profile_production;

use super::super::shell::ParsedRoute;
use super::{context, home_shell, session};
use topcoat::{context::Cx, view::BoxView};

pub(super) const BUTTON: &str = "rounded-md px-3 py-1.5 text-body-sm font-medium transition-colors";
pub(super) const INPUT: &str = "w-full rounded-md border border-[var(--border)] bg-[var(--bg)] px-3 py-2 text-body text-[var(--text)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]";

pub(crate) fn screen<'a>(cx: &'a Cx, route: &ParsedRoute<'_>) -> topcoat::Result<BoxView<'a>> {
    super::workspace::common_screen(cx, route)
}

pub(super) fn region<'a>(
    cx: &'a Cx,
    route: &ParsedRoute<'_>,
    account: i64,
    caller: &context::Caller,
    profile: super::account_profile::Handles,
) -> topcoat::Result<BoxView<'a>> {
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return session::read(
            cx,
            Err(crate::error::LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let content = view::content(cx, profile, user.is_admin);
    Ok(home_shell::page_region(
        cx,
        content,
        None,
        home_shell::page_label(route),
    ))
}
