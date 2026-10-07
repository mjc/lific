//! Shared native appearance preference initialization and storage synchronization.

use topcoat::{
    context::Cx,
    runtime::{Event, Js, Signal, SignalSurrogate, StringSurrogate, Surrogated, expr},
    view::Attributes,
};

pub(crate) fn mount(cx: &Cx, theme: &Signal<String>) -> Attributes {
    let arguments = Js::builder()
        .raw("[")
        .surrogate(&theme.into_surrogate())
        .raw("]")
        .build();
    let key = format!("{}#preferences", super::home_shell::handler_url());
    super::handler_asset::mount(cx, &key, arguments)
}

pub(crate) fn handler_factory() -> Js {
    let handler = expr!(|_mount: Event, theme: &SignalSurrogate<String>| {
        let _stored = |_key: String| {
            raw!(
                "cx.hydrate((()=>{try{return localStorage.getItem(${_key}.toString())??''}catch{return ''}})())",
                String::new()
            )
        };
        let _apply = |_key: StringSurrogate, _value: StringSurrogate| {
            if _key == "lific_theme" {
                let theme_value = if _value == "light" {
                    "light"
                } else if _value == "dark" {
                    "dark"
                } else {
                    "system"
                };
                theme.set(theme_value.to_owned());
                raw!(
                    "document.documentElement.setAttribute('data-theme', ${theme_value}.toString());",
                    ()
                );
            } else if _key == "lific_accent" {
                let _accent = if _value == "teal" {
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
                };
                raw!(
                    "document.documentElement.setAttribute('data-accent', ${_accent}.toString());",
                    ()
                );
            } else if _key == "lific_density" {
                let _density = if _value == "compact" {
                    "compact"
                } else {
                    "comfortable"
                };
                raw!(
                    "document.documentElement.setAttribute('data-density', ${_density}.toString()); document.documentElement.classList.toggle('density-compact', ${_density}.toString() === 'compact');",
                    ()
                );
            } else if _key == "lific_font_scale" {
                let _scale = if _value == "sm" {
                    "sm"
                } else if _value == "lg" {
                    "lg"
                } else {
                    "md"
                };
                raw!(
                    "document.documentElement.setAttribute('data-font-scale', ${_scale}.toString());",
                    ()
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
                }
            }
        };
        raw!(
            "window.addEventListener('storage', event=>${_storage}(cx.event(event)), {signal:cx.abortSignal});",
            ()
        );
    });
    handler.into_evaluated_and_js().1
}
