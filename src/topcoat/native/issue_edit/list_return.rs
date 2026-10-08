//! One saved layout choice for issue breadcrumb, Escape and deferred Delete.
use topcoat::{
    context::Cx,
    runtime::{BoolSurrogate, Event, Expr, Js, Signal, StringSurrogate, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

pub(crate) fn board_path(project: &str) -> String {
    format!("/{project}/board")
}

/// Storage only supplies a preference; Rust chooses the two genuine routes.
fn destination(project: &str) -> Expr<String> {
    let key = format!("lific:list:layout:{project}");
    let preferred = expr!(raw!(
        "cx.hydrate((() => { try { return localStorage.getItem(${key}.toString()) === 'board'; } catch { return false; } })())",
        false
    ));
    let list = format!("/{project}/issues");
    let initial = list.clone();
    let board = board_path(project);
    let choose = expr!(|preferred: BoolSurrogate| {
        if preferred {
            board.clone()
        } else {
            list.clone()
        }
    });
    Expr::evaluate(
        || initial,
        Js::builder()
            .expression(&choose)
            .source("(")
            .expression(&preferred)
            .source(")")
            .build(),
    )
}

/// Re-read at invocation, matching the original backHref() at Delete time.
pub(crate) fn handler<T>(project: &str, callback: &Expr<T>) -> Js {
    Js::builder()
        .source("() => ")
        .expression(callback)
        .source("(")
        .expression(&destination(project))
        .source(")")
        .build()
}

fn breadcrumb_id(identifier: &str) -> String {
    format!("native-issue-list-return-{identifier}")
}

pub(crate) fn breadcrumb<'a>(cx: &'a Cx, project: &str, identifier: &str) -> BoxView<'a> {
    let id = breadcrumb_id(identifier);
    let list = format!("/{project}/issues");
    let href = signal(cx, || super::super::transport::mounted_url(cx, &list));
    let label = signal(cx, || "Issues".to_owned());
    let mount = super::super::transport::trusted_mount(cx)
        .unwrap_or_default()
        .to_owned();
    let board = board_path(project);
    let update_label = label.clone();
    let update_href = href.clone();
    let update = expr!(|destination: StringSurrogate| {
        update_label.set(if destination == board {
            "Board".to_owned()
        } else {
            "Issues".to_owned()
        });
        update_href.set(mount.clone());
        update_href.push_str(destination.clone());
    });
    let mut attributes = super::super::navigation::attrs(cx, &list);
    let _ = attributes.remove("href");
    attributes.insert(cx, "data-topcoat-on:mount", handler(project, &update));
    view! {
        cx =>
        <a
            id=(id)
            class=(super::super::breadcrumbs::LINK_CLASS)
            :href=$(href.get())
            :title=$(label.get())
            (attributes)
        >
            <span class=(super::super::breadcrumbs::LABEL_CLASS) data-label="">
                $(label.get())
            </span>
        </a>
    }
    .boxed()
}

pub(crate) fn keyboard_mount(
    cx: &Cx,
    project: &str,
    identifier: &str,
    editing: Signal<bool>,
    properties_open: Signal<bool>,
) -> Attributes {
    let id = breadcrumb_id(identifier);
    let mount = super::super::transport::trusted_mount(cx)
        .unwrap_or_default()
        .to_owned();
    let navigate = expr!(|_destination: StringSurrogate| {
        // Activate the real breadcrumb so native navigation and fresh
        // destination authorization own the transition.
        raw!(
            "const link=document.getElementById(${id}.toString()); if (link) { link.href=${mount}.toString()+${_destination}.toString(); link.click(); }",
            ()
        );
    });
    let browser = super::super::browser::bindings();
    let keyboard = expr!(|event: Event| {
        if event.key == "Escape" {
            let typing = browser.is_typing_context();
            let suppressed = if event.default_prevented {
                true
            } else {
                typing
            };
            if !suppressed {
                if !editing.get() {
                    event.prevent_default();
                    if properties_open.get() {
                        properties_open.set(false);
                    } else {
                        raw!("nativeListReturn()", ());
                    }
                }
            }
        }
    });
    let js = Js::builder()
        .source("() => { const nativeListReturn = ")
        .source(handler(project, &navigate).to_source())
        .source("; window.addEventListener('keydown', event => ")
        .expression(&keyboard)
        .source("(cx.event(event)), {signal:cx.abortSignal}); }")
        .build();
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(cx, "data-topcoat-on:mount", js);
    attributes
}
