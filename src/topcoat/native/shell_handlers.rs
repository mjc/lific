//! Shared source assembly for preloaded native shell handler factories.

use topcoat::runtime::{Event, Js, SignalSurrogate, expr};

const HANDLER_PATH: &str = "/__native-home-shell.js";

fn mount_factory() -> Js {
    let browser = super::browser::bindings();
    let chrome = expr!(|_event: Event, collapsed: &SignalSurrogate<bool>| {
        collapsed.set(browser.stored("lific:sidebar:collapsed".to_owned()) == "1");
    })
    .into_evaluated_and_js()
    .1;
    Js::builder()
        .raw("(event,chrome,palette,status,request)=>{(")
        .source(chrome.to_source())
        .raw(")(event,chrome[0]);const open=(")
        .source(super::palette::handler_factory().to_source())
        .raw(")(event,chrome,palette,status,request);(")
        .source(super::mobile_navigation::handler_factory().to_source())
        .raw(")(event,chrome,request[0],{'call':open});}")
        .build()
}

pub(crate) fn handler_source() -> &'static str {
    static SOURCE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SOURCE.get_or_init(|| {
        source(
            mount_factory(),
            [
                ("browser", super::browser::factory()),
                ("paletteProjection", super::palette::projection_factory()),
                ("homeRefresh", super::home_refresh::handler_factory()),
                ("accountFocus", super::session::account_handler_factory()),
                (
                    "mobileDispatch",
                    super::mobile_navigation::dispatch_factory(),
                ),
                ("sessionStorage", super::session::handler_factory()),
                ("motion", super::motion::handler_factory()),
                ("preferences", super::preferences::handler_factory()),
                (
                    "navigationAuthority",
                    super::navigation::authority_handler_factory(),
                ),
                (
                    "archiveImport",
                    super::project_import::transport::handler_factory(),
                ),
                (
                    "chromeThemeToggle",
                    super::chrome_controls::theme_toggle_factory(),
                ),
                (
                    "chromeThemeChoice",
                    super::chrome_controls::theme_choice_factory(),
                ),
                ("chromeCollapse", super::chrome_controls::collapse_factory()),
            ],
        )
    })
}

pub(crate) fn handler_url() -> &'static str {
    static URL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    URL.get_or_init(|| url(handler_source()))
}

/// Assemble the mount factory followed by named shell factories.
///
/// Callers supply the named exports after the mount factory so layouts share
/// the same asset.
pub(crate) fn source(mount: Js, factories: impl IntoIterator<Item = (&'static str, Js)>) -> String {
    let mut source = super::handler_asset::source(mount);
    for (name, factory) in factories {
        source.push_str(&source_named(name, factory));
    }
    source
}

pub(crate) fn source_named(name: &str, factory: Js) -> String {
    super::handler_asset::source_named(name, factory)
}

pub(crate) fn url(source: &str) -> String {
    super::handler_asset::url(HANDLER_PATH, source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_exports_mount_and_each_named_factory() {
        let source = source(
            Js::source("(_event)=>{}"),
            [
                ("homeRefresh", Js::source("(_event)=>{}")),
                ("accountFocus", Js::source("(_event)=>{}")),
                ("mobileDispatch", Js::source("(_event)=>{}")),
                ("sessionStorage", Js::source("(_event)=>{}")),
                ("motion", Js::source("(_event)=>{}")),
                ("preferences", Js::source("(_event)=>{}")),
                ("navigationAuthority", Js::source("(_event)=>{}")),
                ("archiveImport", Js::source("(_event)=>{}")),
            ],
        );

        for name in [
            "mount",
            "homeRefresh",
            "accountFocus",
            "mobileDispatch",
            "sessionStorage",
            "motion",
            "preferences",
            "navigationAuthority",
            "archiveImport",
        ] {
            assert!(source.contains(&format!("export const {name}=")));
        }
        assert!(url(&source).starts_with("/__native-home-shell.js?v="));
    }
}
