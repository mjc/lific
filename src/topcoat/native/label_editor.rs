//! Shared label presentation; callers own selection, writes, and picker state.

use topcoat::{
    context::Cx,
    view::{Attributes, BoxView, ViewExt, view},
};

pub(crate) fn add_button(cx: &Cx, attrs: Attributes) -> BoxView<'_> {
    view! {
        cx =>
        <button
            type="button"
            title="Add label"
            aria-label="Add label"
            class="touch-target flex size-5 items-center justify-center rounded border border-dashed border-[var(--border)] text-[var(--text-faint)] transition-colors hover:border-[var(--accent)] hover:text-[var(--accent)]"
            (attrs)
        >
            "+"
        </button>
    }.boxed()
}

pub(crate) fn remove_button(cx: &Cx, name: String, attrs: Attributes) -> BoxView<'_> {
    let label = format!("Remove {name}");
    view! {
        cx =>
        <button
            type="button"
            title="Remove label"
            aria-label=(label)
            class="inline-flex size-3 items-center justify-center rounded-full opacity-60 transition-opacity hover:bg-[var(--bg-subtle)] hover:opacity-100"
            (attrs)
        >
            "×"
        </button>
    }.boxed()
}

pub(crate) fn option<'a>(
    cx: &'a Cx,
    name: String,
    color: Option<&str>,
    selected: bool,
    attrs: Attributes,
) -> BoxView<'a> {
    let color = color.map_or("#6B7280", super::project_overview::label_color);
    let style = format!("background-color: {color}");
    view! {
        cx =>
        <button
            type="button"
            role="option"
            :aria-selected=$(if selected { "true" } else { "false" })
            data-label-name=(name.clone())
            class="flex w-full items-center gap-2 rounded px-2 py-1 text-left text-sm hover:bg-[var(--bg-subtle)]"
            (attrs)
        >
            <span class="size-2.5 rounded-full" style=(style)></span>
            <span>(name)</span>
            <span class="ml-auto" :hidden=$(!selected)>"✓"</span>
        </button>
    }.boxed()
}

pub(crate) fn strip<'a>(
    cx: &'a Cx,
    chips: Vec<BoxView<'a>>,
    empty: Option<BoxView<'a>>,
    add: Option<BoxView<'a>>,
) -> BoxView<'a> {
    let chips_empty = chips.is_empty();
    view! {
        cx =>
        <div class="flex flex-wrap items-center gap-1.5">
            if chips_empty {
                if let Some(empty) = empty {
                    (empty)
                }
            }
            for chip in chips {
                (chip)
            }
            if let Some(add) = add {
                (add)
            }
        </div>
    }
    .boxed()
}

pub(crate) fn popover<'a>(
    cx: &'a Cx,
    width_class: &str,
    attrs: Attributes,
    children: BoxView<'a>,
) -> BoxView<'a> {
    let class = format!(
        "absolute left-0 top-full z-20 mt-1 {width_class} max-w-[calc(100vw-2rem)] rounded-md border border-[var(--border)] bg-[var(--surface)] py-1 shadow-lg"
    );
    view! { cx => <div class=(class) (attrs)>(children)</div> }
    .boxed()
}
