//! Native signup with the existing instance policy and session transaction.

use topcoat::{context::Cx, view::BoxView};

mod actions;
#[cfg(test)]
mod browser_fixture;
mod form;
#[cfg(test)]
mod production;
mod validation;

pub(crate) const MASCOT: &[u8] = include_bytes!("../assets/writing-lizzy.png");

pub(crate) fn screen(cx: &Cx) -> topcoat::Result<BoxView<'_>> {
    super::auth_shell::signed_out(cx)?;
    let conn = super::context::db(cx).read()?;
    let tx = conn.unchecked_transaction()?;
    let settings = crate::db::queries::settings::get(&tx)?;
    let has_users = crate::db::queries::users::has_human_users(&tx)?;
    tx.commit()?;
    Ok(super::auth_shell::content(
        cx,
        super::auth_shell::Mode::Signup { fresh: !has_users },
        settings.instance_name,
        settings.login_message,
        settings.allow_signup,
        form::content(cx, settings.allow_signup),
    ))
}
