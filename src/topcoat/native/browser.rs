//! Typed browser bindings shared by native shell handler factories.

use topcoat::runtime::{BoolSurrogate, Event, Expr, Js, StringSurrogate, Surrogate, Surrogated};

#[derive(Clone, Copy)]
pub(crate) struct Browser;

impl Browser {
    pub(crate) fn focus_id(&self, _id: StringSurrogate) -> BoolSurrogate {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn focus_selector(&self, _selector: StringSurrogate) {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn focus_project(&self, _project: StringSurrogate) {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn scroll_selector(&self, _selector: StringSurrogate) {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn media(&self, _query: StringSurrogate) -> BoolSurrogate {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn stored(&self, _key: StringSurrogate) -> StringSurrogate {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn store(&self, _key: StringSurrogate, _value: StringSurrogate) {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn remove_storage(&self, _key: StringSurrogate) {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn broadcast_storage(&self, _key: StringSurrogate, _value: StringSurrogate) {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn is_disposed(&self) -> BoolSurrogate {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn window_listener<F>(&self, _name: StringSurrogate, _listener: F)
    where
        F: Fn(Event),
    {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn document_listener<F>(&self, _name: StringSurrogate, _listener: F)
    where
        F: Fn(Event),
    {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn on_dispose<F>(&self, _callback: F)
    where
        F: Fn(),
    {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn microtask<F>(&self, _callback: F)
    where
        F: Fn(),
    {
        panic!("browser bindings are only callable in client expressions")
    }

    /// Adapts a closure call through a facade method while expr! lowers calls.
    pub(crate) fn call0<F, R>(&self, _callback: F) -> R
    where
        F: Fn() -> R,
    {
        panic!("browser bindings are only callable in client expressions")
    }

    /// Adapts a one-argument closure call through a facade method while expr! lowers calls.
    pub(crate) fn call1<F, A, R>(&self, _callback: F, _arg: A) -> R
    where
        F: Fn(A) -> R,
    {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn click_capture<F>(&self, _listener: F)
    where
        F: Fn(Event),
    {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn media_listener<F>(&self, _query: StringSurrogate, _listener: F)
    where
        F: Fn(Event),
    {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn navigate(&self, _path: StringSurrogate) {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn open_tab(&self, _url: StringSurrogate) {
        panic!("browser bindings are only callable in client expressions")
    }
}

impl Surrogated for Browser {
    type Surrogate = Self;

    fn into_surrogate(self) -> Self::Surrogate {
        self
    }
}

impl Surrogate for Browser {
    type Real = Self;

    fn into_real(self) -> Self::Real {
        self
    }
}

/// A JavaScript binding object paired with a type-check-only Rust facade.
pub(crate) fn bindings() -> Expr<Browser> {
    Expr::evaluate(
        || Browser,
        Js::source("globalThis.__lificNativeMounts.browser(cx)"),
    )
}

/// The browser facade factory, registered once in the shared shell asset.
pub(crate) fn factory() -> Js {
    Js::source(
        r#"()=>({
                focus_id: id => {
                    const node = document.getElementById(id.toString());
                    node?.focus();
                    return cx.hydrate(Boolean(node));
                },
                focus_selector: selector => document.querySelector(selector.toString())?.focus(),
                focus_project: project => {
                    const link = Array.from(document.querySelectorAll('[data-native-project-trigger]'))
                        .find(element => element.getAttribute('data-native-project-trigger') === project.toString());
                    (link || document.querySelector('[data-native-mobile-root] button'))?.focus();
                },
                scroll_selector: selector => document.querySelector(selector.toString())?.scrollIntoView({block:'nearest',inline:'nearest'}),
                media: query => cx.hydrate(window.matchMedia(query.toString()).matches),
                stored: key => cx.hydrate((() => {
                    try { return localStorage.getItem(key.toString()) || ''; }
                    catch { return ''; }
                })()),
                store: (key, value) => {
                    try { localStorage.setItem(key.toString(), value.toString()); } catch {}
                },
                remove_storage: key => {
                    try { localStorage.removeItem(key.toString()); } catch {}
                },
                broadcast_storage: (key, value) => {
                    let storage = null;
                    try { storage = localStorage; } catch {}
                    window.dispatchEvent(new StorageEvent('storage', {
                        key: key.toString(),
                        newValue: value.toString() === '' ? null : value.toString(),
                        storageArea: storage
                    }));
                },
                is_disposed: () => cx.hydrate(cx.abortSignal.aborted),
                window_listener: (name, listener) => window.addEventListener(
                    name.toString(), event => listener(cx.event(event)), {signal:cx.abortSignal}
                ),
                document_listener: (name, listener) => document.addEventListener(
                    name.toString(), event => listener(cx.event(event)), {signal:cx.abortSignal}
                ),
                on_dispose: callback => cx.abortSignal.addEventListener(
                    'abort', callback, {once:true}
                ),
                microtask: callback => {
                    if (!cx.abortSignal.aborted) queueMicrotask(() => {
                        if (!cx.abortSignal.aborted) callback();
                    });
                },
                // expr! lowers Rust closure calls as receiver method calls. Keep the call
                // behind an ordinary JS callback invocation until that compiler gap is fixed.
                call0: callback => callback(),
                call1: (callback, arg) => callback(arg),
                click_capture: listener => window.addEventListener(
                    'click', event => listener(cx.event(event)), {capture:true,signal:cx.abortSignal}
                ),
                media_listener: (query, listener) => window.matchMedia(query.toString()).addEventListener(
                    'change', event => listener(cx.event(event)), {signal:cx.abortSignal}
                ),
                navigate: path => { void cx.navigate(path.toString()); },
                open_tab: url => { window.open(url.toString(), '_blank', 'noopener'); }
            })"#,
    )
}
