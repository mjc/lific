//! Document asset URLs identify the exact embedded bytes across deployments.

use std::sync::OnceLock;

use sha2::{Digest, Sha256};

pub(crate) const RUNTIME: &str = include_str!("assets/runtime.js");

pub(crate) fn app_stylesheet() -> &'static str {
    static CSS: OnceLock<String> = OnceLock::new();
    CSS.get_or_init(|| {
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            include_str!("assets/base.css"),
            super::controls::STYLESHEET,
            super::shell::STYLESHEET,
            super::native::home_view::STYLESHEET,
            super::native::home_sections::STYLESHEET,
            super::native::home_shell::STYLESHEET,
            super::native::home::STYLESHEET,
            super::native::issue_edit::controls::STYLESHEET,
            super::native::issue_list::STYLESHEET,
            include_str!("native/markdown/styles.css"),
            include_str!("native/issue_edit/detail.css"),
        )
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
