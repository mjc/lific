//! One native shell owner resolves stored motion and current OS preference.
use topcoat::{
    context::Cx,
    runtime::{Event, Js, Signal, Surrogated, expr, signal},
    view::Attributes,
};

pub(super) fn mount(cx: &Cx) -> Attributes {
    let motion = signal(cx, || "system".to_owned());
    let arguments = Js::builder()
        .raw("[")
        .surrogate(&motion.into_surrogate())
        .raw("]")
        .build();
    let key = format!("{}#motion", super::home_shell::handler_url());
    super::handler_asset::mount(cx, &key, arguments)
}

pub(crate) fn handler_factory() -> Js {
    let handler = expr!(|_mount: Event, motion: &Signal<String>| {
        let _stored = || {
            raw!(
                "cx.hydrate((() => {try {return localStorage.getItem('lific_motion') ?? '';} catch {return '';}})())",
                String::new()
            )
        };
        let _set = |_value: String| {
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
        };
        let _refresh = |_event: Event| {
            let is_media = raw!("cx.hydrate(${_event}.inner.type === 'change')", false);
            if !is_media {
                let key = raw!("cx.hydrate(${_event}.inner.key || '')", String::new());
                if key == "lific_motion" {
                    let _value = raw!("cx.hydrate(${_event}.inner.newValue || '')", String::new());
                    raw!("${_set}(${_value});", ());
                } else {
                    let _stored_value = raw!("${_stored}()", String::new());
                    raw!("${_set}(${_stored_value});", ());
                }
            }
            let stored = motion.get();
            let os_reduced = raw!(
                "cx.hydrate(window.matchMedia('(prefers-reduced-motion: reduce)').matches)",
                false
            );
            let _resolved = if stored == "full" {
                "full"
            } else if stored == "reduced" {
                "reduced"
            } else if os_reduced {
                "reduced"
            } else {
                "full"
            };
            raw!(
                "document.documentElement.setAttribute('data-motion', ${_resolved}.toString());",
                ()
            );
        };
        raw!("${_refresh}(${_mount});", ());
        let _storage = |_event: Event| {
            let local_storage = raw!(
                "cx.hydrate((() => {try {return !${_event}.inner.storageArea || ${_event}.inner.storageArea === localStorage;} catch {return false;}})())",
                false
            );
            let key = raw!("cx.hydrate(${_event}.inner.key || '')", String::new());
            if local_storage {
                if key == "" {
                    raw!("${_refresh}(${_event});", ());
                } else if key == "lific_motion" {
                    raw!("${_refresh}(${_event});", ());
                }
            }
        };
        raw!(
            "window.addEventListener('storage', event=>${_storage}(cx.event(event)), {signal:cx.abortSignal});",
            ()
        );
        let _media = |_event: Event| {
            raw!("${_refresh}(${_event});", ());
        };
        raw!(
            "window.matchMedia('(prefers-reduced-motion: reduce)').addEventListener('change', event=>${_media}(cx.event(event)), {signal:cx.abortSignal});",
            ()
        );
    });
    handler.into_evaluated_and_js().1
}
