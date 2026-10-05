//! One native shell owner resolves stored motion and current OS preference.
use topcoat::{
    context::Cx,
    runtime::{Event, expr},
    view::Attributes,
};

pub(super) fn mount(cx: &Cx) -> Attributes {
    let handler = expr!(|_mount: Event| {
        let _refresh = || {
            let stored = raw!(
                "cx.hydrate((() => {try {return localStorage.getItem('lific_motion') ?? '';} catch {return '';}})())",
                String::new()
            );
            let os_reduced = raw!(
                "cx.hydrate(window.matchMedia('(prefers-reduced-motion: reduce)').matches)",
                false
            );
            let _motion = if stored == "full" {
                "full"
            } else {
                if stored == "reduced" {
                    "reduced"
                } else {
                    if os_reduced { "reduced" } else { "full" }
                }
            };
            raw!(
                "document.documentElement.setAttribute('data-motion', ${_motion}.toString());",
                ()
            );
        };
        raw!("${_refresh}();", ());
        raw!(
            "window.addEventListener('storage', ${_refresh}, {signal:cx.abortSignal});",
            ()
        );
        raw!(
            "window.matchMedia('(prefers-reduced-motion: reduce)').addEventListener('change', ${_refresh}, {signal:cx.abortSignal});",
            ()
        );
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
    attributes
}
