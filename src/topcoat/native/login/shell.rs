//! Login form in the shared signed-out application chrome.
use topcoat::{context::Cx, view::BoxView};

pub(super) fn content<'a>(
    cx: &'a Cx,
    instance_name: Option<String>,
    login_message: Option<String>,
    allow_signup: bool,
    auto: bool,
) -> BoxView<'a> {
    super::super::auth_shell::content(
        cx,
        super::super::auth_shell::Mode::Login,
        instance_name,
        login_message,
        allow_signup,
        super::form::content(cx, auto),
    )
}
