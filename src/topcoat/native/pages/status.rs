//! Lifecycle status controls for native page details.

use super::super::browser;
use super::actions::set_status;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

#[derive(Clone)]
pub(super) struct State {
    account: i64,
    page_id: i64,
    seq: Signal<i64>,
    busy: Signal<bool>,
    editable: bool,
}

impl State {
    pub(super) fn new(
        account: i64,
        page_id: i64,
        seq: Signal<i64>,
        busy: Signal<bool>,
        editable: bool,
    ) -> Self {
        Self {
            account,
            page_id,
            seq,
            busy,
            editable,
        }
    }
}

pub(super) fn detail<'a>(cx: &'a Cx, initial_status: String, state: State) -> BoxView<'a> {
    let State {
        account,
        page_id,
        seq,
        busy,
        editable,
    } = state;
    let selected = signal(cx, || initial_status.clone());
    let message = signal(cx, || "".to_owned());
    let read_only_label = super::view::status_label(&initial_status);
    let change = status_attributes(
        cx,
        (account, page_id, seq, busy.clone()),
        selected.clone(),
        message.clone(),
    );
    view! {
        cx =>
        <div class="mb-6 flex flex-wrap items-center gap-3">
            if editable {
                <select
                    aria-label="Page status"
                    data-native-page-status=""
                    class="px-2.5 py-1.5 rounded-md border border-solid border-[var(--border)] bg-[var(--surface)] text-body-sm text-[var(--text)]"
                    :value=$(selected.get())
                    :disabled=$(busy.get())
                    (change)
                >
                    <option value="draft" :selected=$(selected.get() == "draft")>
                        "Draft"
                    </option>
                    <option value="active" :selected=$(selected.get() == "active")>
                        "Active"
                    </option>
                    <option value="complete" :selected=$(selected.get() == "complete")>
                        "Complete"
                    </option>
                    <option value="archived" :selected=$(selected.get() == "archived")>
                        "Archived"
                    </option>
                </select>
            } else {
                <span class="text-body-sm text-[var(--text-muted)]">
                    $(read_only_label)
                </span>
            }
            <span class="text-body-sm text-[var(--text-muted)]" role="status">
                $(message.get())
            </span>
        </div>
    }
    .boxed()
}

fn status_attributes(
    cx: &Cx,
    page: (i64, i64, Signal<i64>, Signal<bool>),
    selected: Signal<String>,
    message: Signal<String>,
) -> Attributes {
    let (account, page_id, seq, busy) = page;
    let failed_busy = busy.clone();
    let failed_selected = selected.clone();
    let failed_message = message.clone();
    let browser = browser::bindings();
    let handler = expr!(async |event: Event| {
        if !busy.get() {
            let next_status = event.target.value.to_owned();
            let previous_status = selected.get();
            if next_status != previous_status {
                let sent_seq = seq.get();
                selected.set(next_status.clone());
                busy.set(true);
                message.set("".to_owned());
                let sent_status = next_status.clone();
                let failed_status = sent_status.clone();
                let failed_previous_status = previous_status.clone();
                let _failed = || {
                    if !browser.is_disposed() {
                        failed_busy.set(false);
                        if failed_selected.get() == failed_status {
                            failed_selected.set(failed_previous_status.clone());
                        }
                        failed_message.set("Couldn't save this page status. Try again.".to_owned());
                    }
                };
                let _save = async || {
                    if browser.is_disposed() {
                        return;
                    }
                    let outcome = set_status(account, page_id, sent_status, sent_seq).await;
                    if !browser.is_disposed() {
                        busy.set(false);
                        if outcome.status.is_ok() {
                            if selected.get() == next_status {
                                selected.set(outcome.page_status.unwrap());
                            }
                            if seq.get() == sent_seq {
                                seq.set(outcome.seq.unwrap());
                            }
                            message.set("Saved".to_owned());
                        } else {
                            let reason = outcome.status.unwrap_err();
                            if selected.get() == next_status {
                                selected.set(previous_status.clone());
                            }
                            message.set(if reason == "conflict" {
                                "This page changed elsewhere. Reload before changing its status."
                                    .to_owned()
                            } else if reason == "reauth" {
                                "Please sign in again.".to_owned()
                            } else if reason == "forbidden" {
                                "You can no longer edit this page.".to_owned()
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
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:change",
        handler.into_evaluated_and_js().1,
    );
    attrs
}
