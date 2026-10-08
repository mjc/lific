//! Native Main Page pin control.

use super::super::browser;
use super::{actions::set_pinned, metadata};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

pub(super) fn detail<'a>(
    cx: &'a Cx,
    pinned: Signal<bool>,
    account: i64,
    page_id: i64,
    seq: Signal<i64>,
    busy: Signal<bool>,
    editable: bool,
) -> BoxView<'a> {
    let message = signal(cx, || "".to_owned());
    let click = pin_attributes(
        cx,
        account,
        page_id,
        pinned.clone(),
        seq,
        busy.clone(),
        message.clone(),
    );
    view! {
        cx =>
        <div class="contents">
            if editable {
                <button
                    type="button"
                    data-native-page-pin=""
                    class="flex items-center gap-1.5 text-body-sm font-medium px-2 py-1 rounded-md border transition-colors aria-pressed:text-[var(--accent)] aria-pressed:border-[var(--accent)] aria-pressed:bg-[var(--accent-subtle)] text-[var(--text-muted)] border-[var(--border)] hover:bg-[var(--bg-subtle)] hover:text-[var(--text)]"
                    :title=$(if pinned.get() {
                        "Unpin this page"
                    } else {
                        "Pin to top of the page list"
                    })
                    :aria-pressed=$(if pinned.get() { "true" } else { "false" })
                    :disabled=$(busy.get())
                    (click)
                >
                    <svg
                        width="13"
                        height="13"
                        viewBox="0 0 24 24"
                        :fill=$(if pinned.get() { "currentColor" } else { "none" })
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        aria-hidden="true"
                    >
                        <path d="M12 17v5" />
                        <path d="M15 5V2" />
                        <path d="M21 3H9l-3 7 6 6 7-7z" />
                        <path d="M9 3v2" />
                    </svg>
                    $(if pinned.get() { "Pinned" } else { "Pin" })
                </button>
                <span class="text-body-sm text-[var(--text-muted)]" role="status">
                    $(message.get())
                </span>
            }
        </div>
    }
    .boxed()
}

fn pin_attributes(
    cx: &Cx,
    account: i64,
    page_id: i64,
    pinned: Signal<bool>,
    seq: Signal<i64>,
    busy: Signal<bool>,
    message: Signal<String>,
) -> Attributes {
    let failed_busy = busy.clone();
    let failed_pinned = pinned.clone();
    let failed_message = message.clone();
    let browser = browser::bindings();
    let failure_messages =
        metadata::failure_messages("This page changed elsewhere. Reload before changing its pin.");
    let conflict_message = failure_messages.conflict;
    let reauth_message = failure_messages.reauth;
    let forbidden_message = failure_messages.forbidden;
    let handler = expr!(async |_event: Event| {
        if browser.is_disposed() {
            return;
        }
        if !busy.get() {
            let next_pinned = !pinned.get();
            let previous_pinned = pinned.get();
            let sent_seq = seq.get();
            pinned.set(next_pinned);
            busy.set(true);
            message.set("".to_owned());
            let failed_next_pinned = next_pinned;
            let failed_previous_pinned = previous_pinned;
            let _failed = || {
                if !browser.is_disposed() {
                    failed_busy.set(false);
                    if failed_pinned.get() == failed_next_pinned {
                        failed_pinned.set(failed_previous_pinned);
                    }
                    failed_message.set("Couldn't save this page pin. Try again.".to_owned());
                }
            };
            let _save = async || {
                if browser.is_disposed() {
                    return;
                }
                let outcome = set_pinned(account, page_id, next_pinned, sent_seq).await;
                if !browser.is_disposed() {
                    busy.set(false);
                    if outcome.status.is_ok() {
                        if pinned.get() == next_pinned {
                            pinned.set(outcome.pinned.unwrap());
                        }
                        if seq.get() == sent_seq {
                            seq.set(outcome.seq.unwrap());
                        }
                        message.set("Saved".to_owned());
                    } else {
                        let reason = outcome.status.unwrap_err();
                        if pinned.get() == next_pinned {
                            pinned.set(previous_pinned);
                        }
                        message.set(if reason == "conflict" {
                            conflict_message.clone()
                        } else if reason == "reauth" {
                            reauth_message.clone()
                        } else if reason == "forbidden" {
                            forbidden_message.clone()
                        } else {
                            reason
                        });
                    }
                }
            };
            browser.microtask(|| {
                if !browser.is_disposed() {
                    raw!(
                        "Promise.resolve().then(()=>${_save}()).catch(()=>${_failed}());",
                        ()
                    );
                }
            });
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attrs
}
