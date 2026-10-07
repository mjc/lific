//! Native instance administration.

mod actions;
mod view;

#[cfg(test)]
mod mutations_production;
#[cfg(test)]
mod production;

use super::{context, home_shell, session};
use topcoat::{context::Cx, view::BoxView};

pub(crate) fn screen<'a>(
    cx: &'a Cx,
    route: &super::super::shell::ParsedRoute<'_>,
) -> topcoat::Result<BoxView<'a>> {
    super::workspace::common_screen(cx, route)
}

pub(super) fn region<'a>(
    cx: &'a Cx,
    route: &super::super::shell::ParsedRoute<'_>,
    account: i64,
    caller: &context::Caller,
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
    let content = view::content(cx, account, user.is_admin);
    Ok(home_shell::page_region(
        cx,
        content,
        None,
        home_shell::page_label(route),
    ))
}
