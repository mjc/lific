//! Native signed-out authentication using the existing login boundary.
use topcoat::{context::Cx, view::BoxView};
mod actions;
mod form;
mod shell;

pub(crate) const MASCOT: &[u8] = include_bytes!("../assets/reading-lizzy.png");

pub(crate) fn screen(cx: &Cx) -> topcoat::Result<BoxView<'_>> {
    super::auth_shell::signed_out(cx)?;
    let settings = crate::db::queries::settings::get(&*super::context::db(cx).read()?)?;
    let auth = topcoat::context::app_context::<crate::auth::AuthState>(cx);
    Ok(shell::content(
        cx,
        settings.instance_name,
        settings.login_message,
        settings.allow_signup,
        settings.web_auto_login || !auth.required,
    ))
}

#[cfg(test)]
mod production;
