//! Initial browser primitives. Parsing and application decisions belong to Rust consumers.

use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr},
    view::Attributes,
};

pub(crate) fn mount(
    cx: &Cx,
    initialized: Signal<bool>,
    inputs: Signal<String>,
    storage_key: String,
) -> Attributes {
    let handler = expr!(|_event: Event| {
        if !initialized.get() {
            inputs.set(raw!(
                    r#"cx.hydrate((() => {
                        const now = new Date();
                        let storedValue = null;
                        try { storedValue = localStorage.getItem(${storage_key}.toString()); } catch {}
                        return JSON.stringify({epochMilliseconds: now.getTime(),
                            timezoneOffsetMinutes: now.getTimezoneOffset(),
                            locale: new Intl.Collator().resolvedOptions().locale, storedValue});
                    })())"#,
                    String::new()
                ));
            initialized.set(true);
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
    attributes
}
