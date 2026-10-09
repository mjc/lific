use super::super::icons::{UiIcon, ui_icon};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

const SELECTED_CONTROL: &str = "rounded-md px-3 py-1.5 text-body-sm font-medium transition-all bg-[var(--surface)] text-[var(--text)] shadow-[0_1px_2px_rgba(0,0,0,0.12)]";
const UNSELECTED_CONTROL: &str = "rounded-md px-3 py-1.5 text-body-sm font-medium transition-all text-[var(--text-muted)] hover:text-[var(--text)]";
const ACCENT_SELECTED: &str = "grid size-8 shrink-0 place-items-center rounded-full transition-transform motion-safe:active:scale-90 ring-2 ring-offset-2 ring-offset-[var(--surface)] ring-[var(--text)]";
const ACCENT_UNSELECTED: &str = "grid size-8 shrink-0 place-items-center rounded-full transition-transform motion-safe:active:scale-90 hover:scale-110";
const ACCENTS: [(&str, &str, &str); 6] = [
    ("indigo", "Indigo", "#9287d7"),
    ("teal", "Teal", "#4dd9c7"),
    ("rose", "Rose", "#f27a9c"),
    ("amber", "Amber", "#e0a530"),
    ("green", "Green", "#5cd192"),
    ("violet", "Violet", "#b48af0"),
];

fn selection_mount(
    cx: &Cx,
    theme: Signal<String>,
    accent: Signal<String>,
    density: Signal<String>,
    scale: Signal<String>,
    motion: Signal<String>,
) -> Attributes {
    let handler = expr!(|_event: Event| {
        let _stored = |_key: String| {
            raw!(
                "cx.hydrate((()=>{try{return localStorage.getItem(${_key}.toString())??''}catch{return ''}})())",
                String::new()
            )
        };
        let _apply = |_key: String, _value: String| {
            if _key == "lific_theme" {
                theme.set(
                    if _value == "light" {
                        "light"
                    } else if _value == "dark" {
                        "dark"
                    } else {
                        "system"
                    }
                    .to_owned(),
                );
            } else if _key == "lific_accent" {
                accent.set(
                    if _value == "teal" {
                        "teal"
                    } else if _value == "rose" {
                        "rose"
                    } else if _value == "amber" {
                        "amber"
                    } else if _value == "green" {
                        "green"
                    } else if _value == "violet" {
                        "violet"
                    } else {
                        "indigo"
                    }
                    .to_owned(),
                );
            } else if _key == "lific_density" {
                density.set(
                    if _value == "compact" {
                        "compact"
                    } else {
                        "comfortable"
                    }
                    .to_owned(),
                );
            } else if _key == "lific_font_scale" {
                scale.set(
                    if _value == "sm" {
                        "sm"
                    } else if _value == "lg" {
                        "lg"
                    } else {
                        "md"
                    }
                    .to_owned(),
                );
            } else if _key == "lific_motion" {
                motion.set(
                    if _value == "reduced" {
                        "reduced"
                    } else if _value == "full" {
                        "full"
                    } else {
                        "system"
                    }
                    .to_owned(),
                );
            }
        };
        let _refresh = || {
            let _value = raw!("${_stored}(cx.hydrate('lific_theme'))", String::new());
            raw!("${_apply}(cx.hydrate('lific_theme'), ${_value});", ());
            let _value = raw!("${_stored}(cx.hydrate('lific_accent'))", String::new());
            raw!("${_apply}(cx.hydrate('lific_accent'), ${_value});", ());
            let _value = raw!("${_stored}(cx.hydrate('lific_density'))", String::new());
            raw!("${_apply}(cx.hydrate('lific_density'), ${_value});", ());
            let _value = raw!("${_stored}(cx.hydrate('lific_font_scale'))", String::new());
            raw!("${_apply}(cx.hydrate('lific_font_scale'), ${_value});", ());
            let _value = raw!("${_stored}(cx.hydrate('lific_motion'))", String::new());
            raw!("${_apply}(cx.hydrate('lific_motion'), ${_value});", ());
        };
        raw!("${_refresh}();", ());
        let _storage = |_event: Event| {
            let local_storage = raw!(
                "cx.hydrate((() => {try {return !${_event}.inner.storageArea || ${_event}.inner.storageArea === localStorage;} catch {return false;}})())",
                false
            );
            let key = raw!("cx.hydrate(${_event}.inner.key || '')", String::new());
            if local_storage {
                if key == "" {
                    raw!("${_refresh}();", ());
                } else if key == "lific_theme" {
                    let _value = raw!("cx.hydrate(${_event}.inner.newValue || '')", String::new());
                    raw!("${_apply}(${key}, ${_value});", ());
                } else if key == "lific_accent" {
                    let _value = raw!("cx.hydrate(${_event}.inner.newValue || '')", String::new());
                    raw!("${_apply}(${key}, ${_value});", ());
                } else if key == "lific_density" {
                    let _value = raw!("cx.hydrate(${_event}.inner.newValue || '')", String::new());
                    raw!("${_apply}(${key}, ${_value});", ());
                } else if key == "lific_font_scale" {
                    let _value = raw!("cx.hydrate(${_event}.inner.newValue || '')", String::new());
                    raw!("${_apply}(${key}, ${_value});", ());
                } else if key == "lific_motion" {
                    let _value = raw!("cx.hydrate(${_event}.inner.newValue || '')", String::new());
                    raw!("${_apply}(${key}, ${_value});", ());
                }
            }
        };
        raw!(
            "window.addEventListener('storage', event=>${_storage}(cx.event(event)), {signal:cx.abortSignal});",
            ()
        );
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

fn preference_button<'a>(
    cx: &'a Cx,
    key: &'static str,
    value: &'static str,
    label: &'static str,
    selected: Signal<String>,
) -> BoxView<'a> {
    let remove_default = matches!(value, "system" | "comfortable" | "md" | "indigo");
    let preference = selected.clone();
    let handler = expr!(|_event: Event| {
        preference.set(value.to_owned());
        if remove_default {
            raw!(
                "try {localStorage.removeItem(${key}.toString());} catch {}",
                ()
            );
            raw!(
                "(()=>{let area=null;try{area=localStorage}catch{};window.dispatchEvent(new StorageEvent('storage', {key:${key}.toString(), newValue:null, storageArea:area}))})();",
                ()
            );
        } else {
            raw!(
                "try {localStorage.setItem(${key}.toString(), ${value});} catch {}",
                ()
            );
            raw!(
                "(()=>{let area=null;try{area=localStorage}catch{};window.dispatchEvent(new StorageEvent('storage', {key:${key}.toString(), newValue:${value}.toString(), storageArea:area}))})();",
                ()
            );
        }
    });
    let mut attrs = Attributes::with_capacity(2);
    attrs.insert(
        cx,
        "data-native-appearance-choice",
        format!("{key}:{value}"),
    );
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    view! {
        cx =>
        <button
            type="button"
            :class=$(if selected.get() == value {
                SELECTED_CONTROL
            } else {
                UNSELECTED_CONTROL
            })
            :aria-pressed=$(if selected.get() == value { "true" } else { "false" })
            (attrs)
        >
            (label)
        </button>
    }
    .boxed()
}

fn accent_button<'a>(
    cx: &'a Cx,
    selected: Signal<String>,
    (value, label, swatch): (&'static str, &'static str, &'static str),
) -> BoxView<'a> {
    let preference = selected.clone();
    let handler = expr!(|_event: Event| {
        preference.set(value.to_owned());
        if value == "indigo" {
            raw!(
                "try {localStorage.removeItem('lific_accent');} catch {}",
                ()
            );
            raw!(
                "(()=>{let area=null;try{area=localStorage}catch{};window.dispatchEvent(new StorageEvent('storage', {key:'lific_accent', newValue:null, storageArea:area}))})();",
                ()
            );
        } else {
            raw!(
                "try {localStorage.setItem('lific_accent', ${value});} catch {}",
                ()
            );
            raw!(
                "(()=>{let area=null;try{area=localStorage}catch{};window.dispatchEvent(new StorageEvent('storage', {key:'lific_accent', newValue:${value}.toString(), storageArea:area}))})();",
                ()
            );
        }
    });
    let mut attrs = Attributes::with_capacity(2);
    attrs.insert(
        cx,
        "data-native-appearance-choice",
        format!("lific_accent:{value}"),
    );
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    view! {
        cx =>
        <button
            type="button"
            :class=$(if selected.get() == value {
                ACCENT_SELECTED
            } else {
                ACCENT_UNSELECTED
            })
            style=(format!("background-color: {swatch}"))
            title=(label.to_owned())
            aria-label=(format!("Accent: {label}"))
            :aria-pressed=$(if selected.get() == value { "true" } else { "false" })
            (attrs)
        >
            <span
                :hidden=$(selected.get() != value)
                class="text-white drop-shadow-[0_1px_1px_rgba(0,0,0,0.4)]"
            >
                (ui_icon(cx, UiIcon::Selected, 14))
            </span>
        </button>
    }
    .boxed()
}

pub(super) fn section(cx: &Cx) -> BoxView<'_> {
    let theme = signal(cx, || "system".to_owned());
    let accent = signal(cx, || "indigo".to_owned());
    let density = signal(cx, || "comfortable".to_owned());
    let scale = signal(cx, || "md".to_owned());
    let motion = signal(cx, || "system".to_owned());

    let theme_buttons = [
        preference_button(cx, "lific_theme", "light", "Light", theme.clone()),
        preference_button(cx, "lific_theme", "dark", "Dark", theme.clone()),
        preference_button(cx, "lific_theme", "system", "System", theme.clone()),
    ];
    let accent_buttons = ACCENTS.map(|preset| accent_button(cx, accent.clone(), preset));
    let density_buttons = [
        preference_button(
            cx,
            "lific_density",
            "comfortable",
            "Comfortable",
            density.clone(),
        ),
        preference_button(cx, "lific_density", "compact", "Compact", density.clone()),
    ];
    let scale_buttons = [
        preference_button(cx, "lific_font_scale", "sm", "S", scale.clone()),
        preference_button(cx, "lific_font_scale", "md", "M", scale.clone()),
        preference_button(cx, "lific_font_scale", "lg", "L", scale.clone()),
    ];
    let motion_buttons = [
        preference_button(cx, "lific_motion", "system", "System", motion.clone()),
        preference_button(cx, "lific_motion", "reduced", "Reduced", motion.clone()),
        preference_button(cx, "lific_motion", "full", "Full", motion.clone()),
    ];
    let mount = selection_mount(cx, theme, accent, density, scale, motion);

    view! {
        cx =>
        <section
            data-native-appearance=""
            class="rounded-xl bg-[var(--surface)] p-5 shadow-[0_1px_2px_rgba(0,0,0,0.06)]"
            (mount)
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
    }
    .boxed()
}
