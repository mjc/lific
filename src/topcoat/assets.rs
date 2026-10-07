//! Document asset URLs identify the exact embedded bytes across deployments.

use std::sync::OnceLock;

use sha2::{Digest, Sha256};
use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

pub(crate) const RUNTIME: &str = include_str!("assets/runtime.js");

pub(crate) fn runtime_source() -> &'static str {
    static SOURCE: OnceLock<String> = OnceLock::new();
    SOURCE.get_or_init(|| {
        let handlers = [
            (super::native::home_shell::handler_url(), "mount as native0,homeRefresh as nativeHomeRefresh,mobileDispatch as nativeMobileDispatch,accountFocus as nativeAccountFocus,sessionStorage as nativeSessionStorage,motion as nativeMotion,navigationAuthority as nativeNavigationAuthority"),
            (super::native::project_sidebar::handler_url(), "mount as native1,recentsRefresh as nativeRecentsRefresh"),
        ];
        let mut source = String::new();
        let mut bindings = Vec::new();
        for (index, (url, exports)) in handlers.into_iter().enumerate() {
            let import = serde_json::to_string(&format!(".{url}")).expect("static handler URL");
            let key = serde_json::to_string(url).expect("static handler URL");
            source.push_str(&format!("import {{{exports}}} from {import};\n"));
            bindings.push(format!("{key}:native{index}"));
        }
        for (key, function) in [
            (format!("{}#home-refresh", super::native::home_shell::handler_url()), "nativeHomeRefresh"),
            (format!("{}#mobile-dispatch", super::native::home_shell::handler_url()), "nativeMobileDispatch"),
            (format!("{}#account-focus", super::native::home_shell::handler_url()), "nativeAccountFocus"),
            (format!("{}#session-storage", super::native::home_shell::handler_url()), "nativeSessionStorage"),
            (format!("{}#motion", super::native::home_shell::handler_url()), "nativeMotion"),
            (format!("{}#navigation-authority", super::native::home_shell::handler_url()), "nativeNavigationAuthority"),
            (format!("{}#recents-refresh", super::native::project_sidebar::handler_url()), "nativeRecentsRefresh"),
        ] {
            let key = serde_json::to_string(&key).expect("static handler URL");
            bindings.push(format!("{key}:{function}"));
        }
        source.push_str(&format!(
            "Object.defineProperty(globalThis,'__lificNativeMounts',{{value:Object.freeze({{{}}}),writable:false,configurable:false}});\n",
            bindings.join(",")
        ));
        source.push_str(RUNTIME);
        source
    })
}

pub(crate) const DM_SANS_ITALIC_LATIN_EXT: &[u8] =
    include_bytes!("assets/fonts/dm-sans-italic-latin-ext.woff2");
pub(crate) const DM_SANS_ITALIC_LATIN: &[u8] =
    include_bytes!("assets/fonts/dm-sans-italic-latin.woff2");
pub(crate) const DM_SANS_NORMAL_LATIN_EXT: &[u8] =
    include_bytes!("assets/fonts/dm-sans-normal-latin-ext.woff2");
pub(crate) const DM_SANS_NORMAL_LATIN: &[u8] =
    include_bytes!("assets/fonts/dm-sans-normal-latin.woff2");
pub(crate) const SPACE_GROTESK_NORMAL_VIETNAMESE: &[u8] =
    include_bytes!("assets/fonts/space-grotesk-normal-vietnamese.woff2");
pub(crate) const SPACE_GROTESK_NORMAL_LATIN_EXT: &[u8] =
    include_bytes!("assets/fonts/space-grotesk-normal-latin-ext.woff2");
pub(crate) const SPACE_GROTESK_NORMAL_LATIN: &[u8] =
    include_bytes!("assets/fonts/space-grotesk-normal-latin.woff2");

pub(crate) fn app_stylesheet() -> &'static str {
    static CSS: OnceLock<String> = OnceLock::new();
    CSS.get_or_init(|| {
        [
            include_str!("assets/base.css"),
            super::controls::STYLESHEET,
            super::shell::STYLESHEET,
            super::native::icons::stylesheet(),
            super::native::home_view::STYLESHEET,
            super::native::home_sections::STYLESHEET,
            super::native::home_shell::STYLESHEET,
            include_str!("native/project_sidebar/sidebar.css"),
            super::native::home::STYLESHEET,
            super::native::issue_edit::controls::STYLESHEET,
            super::native::issue_edit::activity::STYLESHEET,
            super::native::issue_list::STYLESHEET,
            super::native::board::STYLESHEET,
            include_str!("native/markdown/styles.css"),
            include_str!("native/issue_edit/detail.css"),
            super::native::issue_edit::delete_menu::STYLESHEET,
            include_str!("native/assets/deferred-delete.css"),
            super::native::project_create::STYLESHEET,
            super::native::project_overview::STYLESHEET,
            include_str!("native/assets/motion.css"),
            include_str!("assets/tailwind.css"),
        ]
        .join("\n")
    })
}

fn fingerprinted_url(path: &str, content: &str) -> String {
    format!("{path}?v={:x}", Sha256::digest(content.as_bytes()))
}

pub(crate) fn app_stylesheet_url() -> &'static str {
    static URL: OnceLock<String> = OnceLock::new();
    URL.get_or_init(|| fingerprinted_url("/__topcoat-app.css", app_stylesheet()))
}

pub(crate) fn runtime_url() -> &'static str {
    static URL: OnceLock<String> = OnceLock::new();
    URL.get_or_init(|| fingerprinted_url("/__topcoat-runtime.js", runtime_source()))
}

pub(crate) fn runtime_script(cx: &Cx) -> BoxView<'_> {
    let src = super::native::transport::mounted_url(cx, runtime_url());
    view! { cx =>
        <script type="module" src=(src) data-topcoat-usize-bits=(usize::BITS)></script>
    }
    .boxed()
}
