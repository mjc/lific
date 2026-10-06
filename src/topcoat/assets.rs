//! Document asset URLs identify the exact embedded bytes across deployments.

use std::sync::OnceLock;

use sha2::{Digest, Sha256};

pub(crate) const RUNTIME: &str = include_str!("assets/runtime.js");

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
            super::native::icons::STYLESHEET,
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
    URL.get_or_init(|| fingerprinted_url("/__topcoat-runtime.js", RUNTIME))
}
