//! Import complete project archives through the existing REST API.

use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

pub(crate) const SCRIPT_PATH: &str = "/__topcoat-project-settings.js";
pub(crate) const SCRIPT: &str = include_str!("assets/project-settings.js");
pub(crate) const STYLESHEET_PATH: &str = "/__topcoat-project-settings.css";
pub(crate) const STYLESHEET: &str = include_str!("assets/project-settings.css");

pub(crate) fn archive_import(cx: &Cx) -> BoxView<'_> {
    view! { cx =>
        <section class="tc-project-settings" data-topcoat-project-settings="archive" aria-busy="true">
            <h1>"Import project archive"</h1>
            <p data-project-settings-status="" role="status" aria-live="polite">"Loading project access…"</p>
            <p data-project-settings-error="" role="alert"></p>
            <div data-project-settings-content=""></div>
        </section>
    }.boxed()
}
