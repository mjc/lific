//! Shared theme controls for native workspace chrome.

use super::icons::UiIcon;
use topcoat::{
    context::Cx,
    runtime::{Event, Js, Signal, SignalSurrogate, StringSurrogate, Surrogated, expr},
    view::{Attributes, BoxView, ViewExt, view},
};

pub(crate) fn theme_button<'a>(
    cx: &'a Cx,
    theme: Signal<String>,
    open: Signal<bool>,
) -> BoxView<'a> {
    let attributes = theme_button_click(cx, &open);
    view! {
        cx =>
        <button
            class="native-home-theme-button native-home-icon-button"
            aria-haspopup="menu"
            :aria-expanded=$(if open.get() { "true" } else { "false" })
            :aria-label=$(if theme.get() == "light" {
                "Choose theme, current: light"
            } else {
                if theme.get() == "dark" {
                    "Choose theme, current: dark"
                } else {
                    "Choose theme, current: system"
                }
            })
            (attributes)
        >
            <span :hidden=$(theme.get() != "system")>
                (super::icons::ui_icon(cx, UiIcon::SystemTheme, 15))
            </span>
            <span :hidden=$(theme.get() != "light")>
                (super::icons::ui_icon(cx, UiIcon::LightTheme, 15))
            </span>
            <span :hidden=$(theme.get() != "dark")>
                (super::icons::ui_icon(cx, UiIcon::DarkTheme, 15))
            </span>
        </button>
    }
    .boxed()
}

pub(crate) fn theme_menu<'a>(cx: &'a Cx, theme: Signal<String>, open: Signal<bool>) -> BoxView<'a> {
    view! {
        cx =>
        <div
            class="native-home-theme-menu"
            role="menu"
            aria-label="Theme"
            :hidden=$(!open.get())
        >
            for (preference, label) in [
                ("light", "Light"),
                ("dark", "Dark"),
                ("system", "System"),
            ] {
                (theme_choice(cx, theme.clone(), open.clone(), preference, label))
            }
        </div>
    }
    .boxed()
}

fn theme_choice<'a>(
    cx: &'a Cx,
    theme: Signal<String>,
    open: Signal<bool>,
    preference: &'static str,
    label: &'static str,
) -> BoxView<'a> {
    let attributes = theme_choice_click(cx, &theme, &open, preference);
    view! {
        cx =>
        <button
            role="menuitemradio"
            :aria-checked=$(if theme.get() == preference { "true" } else { "false" })
            (attributes)
        >
            (label)
        </button>
    }
    .boxed()
}

pub(crate) fn theme_toggle_factory() -> Js {
    let expression = expr!(|browser: super::browser::Browser,
                            _event: Event,
                            open: SignalSurrogate<bool>| {
        open.set(!open.get());
        browser.microtask(|| {
            browser.focus_selector(".native-home-theme-menu:not([hidden]) button".to_owned());
        });
    });
    let body = expression.into_evaluated_and_js().1;
    Js::builder()
        .source("(event,open)=>{const browser=")
        .expression(&super::browser::bindings())
        .source(";return (")
        .source(body.to_source())
        .source(")(browser,event,open)}")
        .build()
}

fn theme_button_click(cx: &Cx, open: &Signal<bool>) -> Attributes {
    let key = format!("{}#chromeThemeToggle", super::shell_handlers::handler_url());
    let arguments = Js::builder()
        .raw("[")
        .surrogate(&open.into_surrogate())
        .raw("]")
        .build();
    super::handler_asset::event(cx, &key, arguments, "click")
}

pub(crate) fn theme_choice_factory() -> Js {
    let expression = expr!(|browser: super::browser::Browser,
                            _event: Event,
                            theme: SignalSurrogate<String>,
                            open: SignalSurrogate<bool>,
                            preference: StringSurrogate| {
        theme.set(preference.clone());
        open.set(false);
        if preference == "system" {
            browser.remove_storage("lific_theme".to_owned());
        } else {
            browser.store("lific_theme".to_owned(), preference.clone());
        }
        let stored_value = if preference == "system" {
            "".to_owned()
        } else {
            preference
        };
        browser.broadcast_storage("lific_theme".to_owned(), stored_value);
    });
    let body = expression.into_evaluated_and_js().1;
    Js::builder()
        .source("(event,theme,open,preference)=>{const browser=")
        .expression(&super::browser::bindings())
        .source(";return (")
        .source(body.to_source())
        .source(")(browser,event,theme,open,preference)}")
        .build()
}

fn theme_choice_click(
    cx: &Cx,
    theme: &Signal<String>,
    open: &Signal<bool>,
    preference: &str,
) -> Attributes {
    let key = format!("{}#chromeThemeChoice", super::shell_handlers::handler_url());
    let arguments = Js::builder()
        .raw("[")
        .surrogate(&theme.into_surrogate())
        .raw(",")
        .surrogate(&open.into_surrogate())
        .raw(",")
        .surrogate(&preference.to_owned().into_surrogate())
        .raw("]")
        .build();
    super::handler_asset::event(cx, &key, arguments, "click")
}

pub(crate) fn collapse_factory() -> Js {
    let expression = expr!(|browser: super::browser::Browser,
                            _event: Event,
                            collapsed: SignalSurrogate<bool>| {
        collapsed.set(!collapsed.get());
        let value = if collapsed.get() {
            "1".to_owned()
        } else {
            "0".to_owned()
        };
        browser.store("lific:sidebar:collapsed".to_owned(), value);
    });
    let body = expression.into_evaluated_and_js().1;
    Js::builder()
        .source("(event,collapsed)=>{const browser=")
        .expression(&super::browser::bindings())
        .source(";return (")
        .source(body.to_source())
        .source(")(browser,event,collapsed)}")
        .build()
}

pub(crate) fn collapse_attributes(cx: &Cx, collapsed: &Signal<bool>) -> Attributes {
    let key = format!("{}#chromeCollapse", super::shell_handlers::handler_url());
    let arguments = Js::builder()
        .raw("[")
        .surrogate(&collapsed.into_surrogate())
        .raw("]")
        .build();
    super::handler_asset::event(cx, &key, arguments, "click")
}
