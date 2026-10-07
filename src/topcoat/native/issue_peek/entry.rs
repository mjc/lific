//! One account-bound sheet selection shared by every private workspace page.

use super::super::icons::{self, UiIcon};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, component, view},
};

pub(crate) fn button<'a>(cx: &'a Cx, identifier: &str) -> BoxView<'a> {
    let identifier = identifier.to_owned();
    let label = format!("Peek {identifier}");
    let open = expr!(|event: Event| {
        event.prevent_default();
        event.stop_propagation();
        raw!(
            "document.dispatchEvent(new CustomEvent('lific:native-issue-peek-request',{detail:{identifier:${identifier}.toString()}}));",
            ()
        );
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(cx, "data-topcoat-on:click", open.into_evaluated_and_js().1);
    view! {
        cx =>
        <button
            type="button"
            title="Peek"
            aria-label=(label)
            data-native-peek-open=(identifier)
            class="inline-flex size-6 shrink-0 items-center justify-center rounded text-[var(--text-faint)] hover:text-[var(--accent)] hover:bg-[var(--bg-subtle)] transition-colors [@media(hover:hover)]:opacity-0 [@media(hover:hover)]:group-hover:opacity-100 focus-visible:opacity-100 pointer-coarse:size-8"
            (attrs)
        >
            (icons::ui_icon(cx, UiIcon::DetailsPanel, 13))
        </button>
    }.boxed()
}

pub(crate) fn shared_owner(cx: &Cx, account: i64) -> BoxView<'_> {
    view! { cx => shared_sheet(account: account) }.boxed()
}

#[component]
async fn shared_sheet(cx: &Cx, account: i64) -> topcoat::Result<impl View> {
    let account_cx = cx.keyed(account);
    let selected = signal(&account_cx, String::new);
    let open_selection = selected.clone();
    let close_selection = selected.clone();
    let open = expr!(|_mount: Event| {
        let _open = |_event: Event| {
            let identifier = raw!(
                "cx.hydrate(${_event}.inner.detail.identifier)",
                String::new()
            );
            open_selection.set(identifier);
        };
        let _close = |_event: Event| {
            close_selection.set("".to_owned());
        };
        raw!(
            "document.addEventListener('lific:native-issue-peek-request',event=>${_open}(cx.event(event)),{signal:cx.abortSignal}); document.addEventListener('topcoat:before-navigation-commit',event=>${_close}(cx.event(event)),{signal:cx.abortSignal});",
            ()
        );
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(cx, "data-topcoat-on:mount", open.into_evaluated_and_js().1);
    let selection = selected.clone();
    Ok(view! {
        cx =>
        <div data-native-peek-owner="" (attrs)>
            selected_sheet(
                account: account,
                identifier: $(selection.get()),
                close: selected
            )
        </div>
    })
}

#[shard("/__native_issue_peek/selected")]
async fn selected_sheet(
    cx: &Cx,
    account: i64,
    identifier: String,
    close: Signal<String>,
) -> topcoat::Result<impl View> {
    if identifier.is_empty() {
        Ok(view! { cx => }.boxed())
    } else {
        super::surface(cx, account, &identifier, true, close)
    }
}
