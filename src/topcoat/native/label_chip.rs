//! Shared display for project label chips.

use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

pub(crate) fn render<'a>(cx: &'a Cx, name: String, color: Option<&str>) -> BoxView<'a> {
    let style = color.map_or_else(
        || "border-color:var(--border)".to_owned(),
        |color| {
            let color = super::project_overview::label_color(color);
            format!("color:{color};border-color:{color}40;background:{color}10")
        },
    );
    view! {
        cx =>
        <span
            class="native-label-chip inline-flex items-center gap-1 text-caption font-medium px-2 py-0.5 rounded-full border normal-case"
            style=(style)
        >
            (name)
        </span>
    }
    .boxed()
}
