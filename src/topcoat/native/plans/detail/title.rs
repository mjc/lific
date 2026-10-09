use super::{PlanEditor, StrEcmaTrimExt, browser, mutate_plan};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr},
    view::{Attributes, BoxView, ViewExt, view},
};

pub(super) fn owner_attributes(
    cx: &Cx,
    account: i64,
    project: String,
    plan_id: i64,
    title_state: (Signal<String>, Signal<String>, Signal<bool>),
    completion_state: (Signal<i64>, Signal<String>),
) -> Attributes {
    let (title, draft, editing) = title_state;
    let (revision, message) = completion_state;
    let browser = browser::bindings();
    let start_title = title.clone();
    let start_draft = draft.clone();
    let start_editing = editing.clone();
    let start_message = message.clone();
    let submit_title = title.clone();
    let submit_draft = draft.clone();
    let submit_editing = editing.clone();
    let submit_message = message.clone();
    let failed_message_async = message;
    let submit_revision_async = revision;
    let cancel_title = title;
    let cancel_draft = draft.clone();
    let cancel_editing = editing.clone();
    let handler = expr!(|event: Event| {
        if !browser.is_disposed() {
            let trigger = raw!(
                "cx.hydrate(Boolean(${event}.inner.target?.closest('[data-native-plan-title-trigger]'))) ",
                false
            );
            let input_target = raw!(
                "cx.hydrate(Boolean(${event}.inner.target?.closest('[data-native-plan-title-input]'))) ",
                false
            );
            let key_commit = if event.event_type == "keydown" {
                if input_target {
                    if event.key == "Enter" {
                        true
                    } else if event.key == "s" {
                        if event.ctrl_key { true } else { event.meta_key }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            let should_commit = if event.event_type == "focusout" {
                input_target
            } else {
                key_commit
            };
            if event.event_type == "click" {
                if trigger {
                    start_draft.set(start_title.get());
                    start_message.set("".to_owned());
                    start_editing.set(true);
                }
            } else if event.event_type == "keydown" {
                if trigger {
                    if if event.key == "Enter" {
                        true
                    } else {
                        event.key == " "
                    } {
                        event.prevent_default();
                        start_draft.set(start_title.get());
                        start_message.set("".to_owned());
                        start_editing.set(true);
                    }
                } else if input_target {
                    if event.key == "Escape" {
                        event.prevent_default();
                        if cancel_editing.get() {
                            cancel_draft.set(cancel_title.get());
                            cancel_editing.set(false);
                        }
                    } else if key_commit {
                        event.prevent_default();
                    }
                }
            } else if event.event_type == "input" {
                if input_target {
                    if editing.get() {
                        draft.set(event.target.value.to_owned());
                    }
                }
            }
            if should_commit {
                if submit_editing.get() {
                    submit_editing.set(false);
                    let next_title = submit_draft.get().trim_ecmascript();
                    if !next_title.is_empty() {
                        if next_title != submit_title.get() {
                            submit_message.set("".to_owned());
                            let _failed = || {
                                if !browser.is_disposed() {
                                    failed_message_async
                                        .set("Unable to save plan title.".to_owned());
                                }
                            };
                            let _run = async || {
                                let committed_title = mutate_plan(
                                    account,
                                    project,
                                    plan_id,
                                    0_i64,
                                    "title".to_owned(),
                                    next_title,
                                )
                                .await;
                                if !browser.is_disposed() {
                                    submit_title.set(committed_title);
                                    submit_revision_async.set(submit_revision_async.get() + 1_i64);
                                }
                            };
                            raw!(
                                "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                                ()
                            );
                        }
                    }
                }
            }
        }
    });
    let handler_js = handler.into_evaluated_and_js().1;
    let mut attrs = Attributes::with_capacity(4);
    attrs.insert(cx, "data-topcoat-on:click", handler_js.clone());
    attrs.insert(cx, "data-topcoat-on:keydown", handler_js.clone());
    attrs.insert(cx, "data-topcoat-on:input", handler_js.clone());
    attrs.insert(cx, "data-topcoat-on:focusout", handler_js);
    attrs
}

pub(super) fn reconcile_attributes(
    cx: &Cx,
    revision_value: i64,
    revision: Signal<i64>,
    current_title: Signal<String>,
    saved_title: String,
) -> Attributes {
    let browser = browser::bindings();
    let reconcile = expr!(|_event: Event| {
        if !browser.is_disposed() {
            if revision.get() == revision_value {
                current_title.set(saved_title.clone());
            }
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:mount",
        reconcile.into_evaluated_and_js().1,
    );
    attrs
}

pub(super) fn form<'a>(cx: &'a Cx, title: Signal<String>, editor: PlanEditor) -> BoxView<'a> {
    let draft = editor.title_draft;
    let editing = editor.title_editing;
    let mut trigger = Attributes::with_capacity(1);
    trigger.insert(cx, "data-native-plan-title-trigger", "true");
    let mut editor_attributes = Attributes::with_capacity(1);
    editor_attributes.insert(cx, "data-native-plan-title-input", "true");
    view! {
        cx =>
        if editing.get() {
            <input
                type="text"
                class="w-full text-title mb-4 font-display tracking-tight bg-transparent border-0 border-b-2 border-solid border-b-[var(--accent)] outline-none text-[var(--text)] py-1"
                :value=$(draft.get())
                autofocus="autofocus"
                (editor_attributes)
            />
        } else {
            <button
                type="button"
                class="text-title mb-4 w-full text-left font-display tracking-tight text-[var(--text)] py-1 rounded transition-colors bg-transparent border-0 cursor-text hover:bg-[var(--bg-subtle)]"
                (trigger)
            >
                $(title.get())
            </button>
        }
    }
    .boxed()
}
