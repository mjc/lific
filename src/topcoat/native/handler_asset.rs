//! Invoke preloaded Rust-generated browser code with each mounting scope's handles.

use topcoat::{context::Cx, runtime::Js, view::Attributes};

pub(crate) fn source(factory: Js) -> String {
    source_named("mount", factory)
}

pub(crate) fn source_named(name: &str, factory: Js) -> String {
    format!(
        "export const {name}=(cx,event,...args)=>({})(event,...args);",
        factory.to_source()
    )
}

pub(crate) fn url(path: &str, source: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{path}?v={:x}", Sha256::digest(source.as_bytes()))
}

pub(crate) fn mount(cx: &Cx, url: &str, arguments: Js) -> Attributes {
    event(cx, url, arguments, "mount")
}

pub(crate) fn event(cx: &Cx, url: &str, arguments: Js, event_name: &str) -> Attributes {
    let handler = Js::builder()
        .raw("event=>globalThis.__lificNativeMounts[")
        .surrogate(&url)
        .raw(".toString()](cx,event,...")
        .source(arguments.to_source())
        .raw(")")
        .build();
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(cx, format!("data-topcoat-on:{event_name}"), handler);
    attributes
}
