//! Typed browser bindings shared by native shell handler factories.

use topcoat::runtime::{
    BoolSurrogate, Event, Expr, I64Surrogate, Js, StringSurrogate, Surrogate, Surrogated,
    UsizeSurrogate,
};

#[derive(Clone, Copy)]
pub(crate) struct Browser;

impl Browser {
    /// Read a string from a JSON storage object on either side of hydration.
    pub(crate) fn json_string(
        &self,
        wire: StringSurrogate,
        key: StringSurrogate,
        fallback: StringSurrogate,
    ) -> StringSurrogate {
        serde_json::from_str::<serde_json::Value>(&wire.into_real())
            .ok()
            .and_then(|value| {
                value
                    .get(key.into_real())
                    .and_then(|value| value.as_str().map(str::to_owned))
            })
            .unwrap_or_else(|| fallback.into_real())
            .into_surrogate()
    }

    /// Normalize a JSON object's string fields against a supplied schema/default.
    pub(crate) fn json_fields(
        &self,
        wire: StringSurrogate,
        defaults: StringSurrogate,
    ) -> StringSurrogate {
        let mut defaults: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&defaults.into_real()).expect("JSON field defaults are an object");
        if let Ok(serde_json::Value::Object(value)) = serde_json::from_str(&wire.into_real()) {
            for (key, fallback) in &mut defaults {
                if let Some(value) = value.get(key).filter(|value| value.is_string()) {
                    *fallback = value.clone();
                }
            }
        }
        serde_json::to_string(&defaults)
            .expect("JSON object is serializable")
            .into_surrogate()
    }

    pub(crate) fn json_set_string(
        &self,
        wire: StringSurrogate,
        key: StringSurrogate,
        value: StringSurrogate,
    ) -> StringSurrogate {
        let mut object: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&wire.into_real()).unwrap_or_default();
        object.insert(
            key.into_real(),
            serde_json::Value::String(value.into_real()),
        );
        serde_json::to_string(&object)
            .expect("JSON object is serializable")
            .into_surrogate()
    }

    pub(crate) fn json_array_contains(
        &self,
        wire: StringSurrogate,
        value: StringSurrogate,
    ) -> BoolSurrogate {
        let values: Vec<String> = serde_json::from_str(&wire.into_real()).unwrap_or_default();
        values.contains(&value.into_real()).into_surrogate()
    }

    /// Parse a positive i64 copied from a DOM string without a lossy JS Number round trip.
    pub(crate) fn positive_i64(
        &self,
        _value: StringSurrogate,
        _fallback: I64Surrogate,
    ) -> I64Surrogate {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn json_array_set_string(
        &self,
        wire: StringSurrogate,
        value: StringSurrogate,
        included: BoolSurrogate,
    ) -> StringSurrogate {
        let mut values: Vec<String> = serde_json::from_str(&wire.into_real()).unwrap_or_default();
        let value = value.into_real();
        values.retain(|item| item != &value);
        if included.into_real() {
            values.push(value);
        }
        serde_json::to_string(&values)
            .expect("string array is serializable")
            .into_surrogate()
    }

    pub(crate) fn focus_id(&self, _id: StringSurrogate) -> BoolSurrogate {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn focus_selector(&self, _selector: StringSurrogate) {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn has_visible_match(&self, _selector: StringSurrogate) -> BoolSurrogate {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn is_typing_context(&self) -> BoolSurrogate {
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

    pub(crate) fn write_clipboard<F>(&self, _text: StringSurrogate, _completed: F)
    where
        F: FnOnce(BoolSurrogate),
    {
        panic!("browser bindings are only callable in client expressions")
    }

    /// Download a response body using browser APIs, returning an empty string
    /// on success or a displayable error when the request or download fails.
    pub(crate) fn download<F>(&self, _url: StringSurrogate, _completed: F)
    where
        F: FnOnce(StringSurrogate),
    {
        panic!("browser bindings are only callable in client expressions")
    }

    /// Read the user's local time using the browser's locale and timezone.
    pub(crate) fn local_time_now(&self) -> StringSurrogate {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn set_timeout<F>(&self, _delay: UsizeSurrogate, _callback: F) -> UsizeSurrogate
    where
        F: FnOnce(),
    {
        panic!("browser bindings are only callable in client expressions")
    }

    pub(crate) fn clear_timeout(&self, _handle: UsizeSurrogate) {
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
        F: FnOnce() -> R,
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
                json_string: (wire, key, fallback) => {
                    try { const value = JSON.parse(wire.toString())?.[key.toString()];
                        return cx.hydrate(typeof value === 'string' ? value : fallback.toString());
                    } catch { return fallback; }
                },
                json_fields: (wire, defaults) => {
                    const fields = JSON.parse(defaults.toString());
                    try { const value = JSON.parse(wire.toString());
                        for (const key of Object.keys(fields)) if (typeof value?.[key] === 'string') fields[key] = value[key];
                    } catch {}
                    return cx.hydrate(JSON.stringify(fields));
                },
                json_set_string: (wire, key, value) => {
                    const fields = JSON.parse(wire.toString());
                    fields[key.toString()] = value.toString();
                    return cx.hydrate(JSON.stringify(fields));
                },
                json_array_contains: (wire, value) => {
                    try { const items = JSON.parse(wire.toString());
                        return cx.hydrate(Array.isArray(items) && items.every(item => typeof item === 'string') && items.includes(value.toString()));
                    } catch { return cx.hydrate(false); }
                },
                positive_i64: (value, fallback) => {
                    const decimal = value.toString();
                    if (!/^[1-9][0-9]{0,18}$/.test(decimal) ||
                        (decimal.length === 19 && decimal > '9223372036854775807')) return fallback;
                    return cx.hydrate({...fallback.dehydrate(), v:decimal});
                },
                json_array_set_string: (wire, value, included) => {
                    let items = [];
                    try { const parsed = JSON.parse(wire.toString());
                        if (Array.isArray(parsed) && parsed.every(item => typeof item === 'string')) items = parsed;
                    } catch {}
                    items = items.filter(item => item !== value.toString());
                    if (included.dehydrate()) items.push(value.toString());
                    return cx.hydrate(JSON.stringify(items));
                },
                focus_id: id => {
                    const node = document.getElementById(id.toString());
                    node?.focus();
                    return cx.hydrate(Boolean(node));
                },
                focus_selector: selector => document.querySelector(selector.toString())?.focus(),
                has_visible_match: selector => cx.hydrate(
                    Array.from(document.querySelectorAll(selector.toString())).some(element =>
                        !element.closest('[hidden]') &&
                        element.getClientRects().length > 0 &&
                        !['hidden', 'collapse'].includes(getComputedStyle(element).visibility)
                    )
                ),
                is_typing_context: () => cx.hydrate(
                    ['INPUT','TEXTAREA','SELECT'].includes(document.activeElement?.tagName) ||
                    Boolean(document.activeElement?.isContentEditable)
                ),
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
                write_clipboard: (text, completed) => {
                    const write = async () => {
                        if (cx.abortSignal.aborted) return;
                        let copied = false;
                        try {
                            if (navigator.clipboard?.writeText) {
                                await navigator.clipboard.writeText(text.toString());
                                copied = true;
                            }
                        } catch {}
                        if (cx.abortSignal.aborted) return;
                        if (!copied) {
                            let field;
                            try {
                                field = document.createElement('textarea');
                                field.value = text.toString();
                                field.setAttribute('readonly', '');
                                field.style.position = 'absolute';
                                field.style.left = '-9999px';
                                document.body.appendChild(field);
                                field.select();
                                copied = document.execCommand('copy');
                            } catch {} finally {
                                if (field) field.remove();
                            }
                        }
                        if (!cx.abortSignal.aborted) completed(cx.hydrate(Boolean(copied)));
                    };
                    void write();
                },
                download: (url, completed) => {
                    const download = async () => {
                        if (cx.abortSignal.aborted) return '';
                        try {
                            const response = await fetch(url.toString(), {
                                signal: cx.abortSignal,
                                redirect: 'error'
                            });
                            if (cx.abortSignal.aborted) return '';
                            if (!response.ok) return 'HTTP ' + response.status;
                            const filename = response.headers.get('content-disposition')
                                ?.match(/filename="([^"]+)"/)?.[1] || 'download';
                            const blob = await response.blob();
                            if (cx.abortSignal.aborted) return '';
                            const objectUrl = URL.createObjectURL(blob);
                            let anchor;
                            try {
                                anchor = document.createElement('a');
                                anchor.href = objectUrl;
                                anchor.download = filename;
                                document.body.appendChild(anchor);
                                anchor.click();
                            } finally {
                                try {
                                    anchor?.remove();
                                } finally {
                                    URL.revokeObjectURL(objectUrl);
                                }
                            }
                            return '';
                        } catch (failure) {
                            return failure instanceof Error ? failure.message : String(failure);
                        }
                    };
                    void download().then(failure => {
                        if (!cx.abortSignal.aborted) completed(cx.hydrate(failure));
                    });
                },
                local_time_now: () => cx.hydrate(new Date().toLocaleTimeString([], {
                    hour: '2-digit', minute: '2-digit'
                })),
                set_timeout: (delay, callback) => {
                    if (cx.abortSignal.aborted) return cx.hydrate({...delay.dehydrate(),v:'0'});
                    const timers = cx.nativeTimeouts ??= new Map();
                    const cancel = () => {
                        clearTimeout(id);timers.delete(id);
                        cx.abortSignal.removeEventListener('abort',cancel);
                    };
                    const id = setTimeout(() => {
                        cancel();
                        if (!cx.abortSignal.aborted) callback();
                    }, Number(delay.toString()));
                    timers.set(id,cancel);
                    cx.abortSignal.addEventListener('abort',cancel,{once:true});
                    return cx.hydrate({...delay.dehydrate(),v:String(id)});
                },
                clear_timeout: handle => {
                    const id = Number(handle.toString());
                    const cancel = cx.nativeTimeouts?.get(id);
                    if (cancel) cancel(); else clearTimeout(id);
                },
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
