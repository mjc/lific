use super::super::icons::{UiIcon, ui_icon};
use topcoat::{
    context::Cx,
    runtime::{Event, expr},
    view::{Attributes, BoxView, ViewExt, view},
};

fn preference_button<'a>(
    cx: &'a Cx,
    key: &'static str,
    value: &'static str,
    label: &'static str,
) -> BoxView<'a> {
    let remove_default = matches!(value, "system" | "comfortable" | "md" | "indigo");
    let handler = expr!(|_event: Event| {
        if remove_default {
            raw!(
                "try { localStorage.removeItem(${key}.toString()); } catch {}",
                ()
            );
        } else {
            raw!(
                "try { localStorage.setItem(${key}.toString(), ${value}.toString()); } catch {}",
                ()
            );
        }
        if key == "lific_theme" {
            let _theme = if value == "light" {
                "light"
            } else if value == "dark" {
                "dark"
            } else {
                "system"
            };
            raw!(
                "document.documentElement.setAttribute('data-theme', ${_theme}.toString())",
                ()
            );
        } else if key == "lific_accent" {
            raw!(
                "document.documentElement.setAttribute('data-accent', ${value}.toString())",
                ()
            );
        } else if key == "lific_density" {
            let _density = if value == "compact" {
                "compact"
            } else {
                "comfortable"
            };
            raw!(
                "document.documentElement.setAttribute('data-density', ${_density}.toString()); document.documentElement.classList.toggle('density-compact', ${_density}.toString() === 'compact')",
                ()
            );
        } else if key == "lific_font_scale" {
            let _scale = if value == "sm" {
                "sm"
            } else if value == "lg" {
                "lg"
            } else {
                "md"
            };
            raw!(
                "document.documentElement.setAttribute('data-font-scale', ${_scale}.toString())",
                ()
            );
        } else {
            let _motion = if value == "reduced" {
                "reduced"
            } else if value == "full" {
                "full"
            } else {
                "system"
            };
            raw!(
                "document.documentElement.setAttribute('data-motion', ${_motion}.toString())",
                ()
            );
        }
        raw!(
            "window.dispatchEvent(new StorageEvent('storage', {key:${key}.toString(), newValue:${value}.toString(), storageArea:localStorage}));",
            ()
        );
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    view! { cx => <button type="button" class=(super::BUTTON) (attrs)>(label)</button> }.boxed()
}

pub(super) fn section(cx: &Cx) -> BoxView<'_> {
    let theme_buttons = [
        preference_button(cx, "lific_theme", "light", "Light"),
        preference_button(cx, "lific_theme", "dark", "Dark"),
        preference_button(cx, "lific_theme", "system", "System"),
    ];
    let accent_buttons = ["indigo", "teal", "rose", "amber", "green", "violet"]
        .map(|accent| preference_button(cx, "lific_accent", accent, accent));
    let density_buttons = [
        preference_button(cx, "lific_density", "comfortable", "Comfortable"),
        preference_button(cx, "lific_density", "compact", "Compact"),
    ];
    let scale_buttons = [
        preference_button(cx, "lific_font_scale", "sm", "S"),
        preference_button(cx, "lific_font_scale", "md", "M"),
        preference_button(cx, "lific_font_scale", "lg", "L"),
    ];
    let motion_buttons = [
        preference_button(cx, "lific_motion", "system", "System"),
        preference_button(cx, "lific_motion", "reduced", "Reduced"),
        preference_button(cx, "lific_motion", "full", "Full"),
    ];
    view! {
        cx =>
        <section
            class="rounded-xl bg-[var(--surface)] p-5 shadow-[0_1px_2px_rgba(0,0,0,0.06)]"
        >
            <h2
                class="mb-1 flex items-center gap-2 text-body-lg font-semibold text-[var(--text)]"
            >
                (ui_icon(cx, UiIcon::Appearance, 15))
                "Appearance"
            </h2>
            <p class="mb-3.5 text-body-sm text-[var(--text-muted)]">
                "System follows your OS."
            </p>
            <div
                class="inline-flex rounded-lg bg-[var(--bg)] p-0.5 shadow-[inset_0_1px_2px_rgba(0,0,0,0.10)]"
            >
                for button in theme_buttons {
                    (button)
                }
            </div>
            <div class="mt-5 border-t border-[var(--border)] pt-5">
                <span
                    class="mb-2.5 block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                >
                    "Accent color"
                </span>
                <div class="flex flex-wrap items-center gap-2.5">
                    for button in accent_buttons {
                        (button)
                    }
                </div>
            </div>
            <div class="mt-5 border-t border-[var(--border)] pt-5">
                <span
                    class="mb-2.5 block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                >
                    "Density"
                </span>
                <div class="inline-flex rounded-lg bg-[var(--bg)] p-0.5">
                    for button in density_buttons {
                        (button)
                    }
                </div>
            </div>
            <div class="mt-5 border-t border-[var(--border)] pt-5">
                <span
                    class="mb-2.5 block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                >
                    "Text size"
                </span>
                <div class="inline-flex rounded-lg bg-[var(--bg)] p-0.5">
                    for button in scale_buttons {
                        (button)
                    }
                </div>
            </div>
            <div class="mt-5 border-t border-[var(--border)] pt-5">
                <span
                    class="mb-2.5 block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                >
                    "Motion"
                </span>
                <div class="inline-flex rounded-lg bg-[var(--bg)] p-0.5">
                    for button in motion_buttons {
                        (button)
                    }
                </div>
                <p class="mt-2 text-caption text-[var(--text-muted)]">
                    "System honors your OS's reduce-motion setting."
                </p>
            </div>
        </section>
    }.boxed()
}
