//! Native Page title editor interactions and sparse save handling.
use super::super::super::runtime::whitespace::StrEcmaTrimExt;
use super::super::browser;
use super::super::deferred_delete::ToastErrorRequest;
use super::actions::save_title;
use super::editor_state::EditorState;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr},
    view::Attributes,
};

pub(super) struct Controls {
    pub(super) editing: Signal<bool>,
    pub(super) revision: Signal<usize>,
    pub(super) busy: Signal<bool>,
    pub(super) save_busy: Signal<bool>,
    pub(super) message: Signal<String>,
    pub(super) last_saved: Signal<String>,
}

pub(super) fn trigger_attributes(
    cx: &Cx,
    editor: &EditorState,
    controls: &Controls,
    focus_id: String,
) -> Attributes {
    let browser = browser::bindings();
    let title = editor.title.clone();
    let title_draft = editor.title_draft.clone();
    let editing = controls.editing.clone();
    let revision = controls.revision.clone();
    let busy = controls.busy.clone();
    let message = controls.message.clone();
    let handler = expr!(|_event: Event| {
        if !browser.is_disposed() {
            if !editing.get() {
                if !busy.get() {
                    title_draft.set(title.get());
                }
                let next_revision = revision.get() + 1_usize;
                revision.set(next_revision);
                editing.set(true);
                message.set("".to_owned());
                let _focus = || {
                    if !browser.is_disposed() {
                        if editing.get() {
                            if revision.get() == next_revision {
                                browser.focus_id(focus_id.clone());
                            }
                        }
                    }
                };
                browser.microtask(_focus);
            }
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attributes
}

pub(super) fn input_attributes(
    cx: &Cx,
    account: i64,
    page_id: i64,
    editor: &EditorState,
    controls: &Controls,
) -> Attributes {
    let input_draft = editor.title_draft.clone();
    let input_revision = controls.revision.clone();
    let input_editing = controls.editing.clone();
    let input_message = controls.message.clone();
    let browser = browser::bindings();
    let input = expr!(|event: Event| {
        if !browser.is_disposed() {
            input_draft.set(event.target.value.to_owned());
            input_revision.set(input_revision.get() + 1_usize);
            input_editing.set(true);
            input_message.set("".to_owned());
        }
    });

    let title = editor.title.clone();
    let body = editor.body.clone();
    let title_draft = editor.title_draft.clone();
    let body_draft = editor.body_draft.clone();
    let seq = editor.seq.clone();
    let revision = controls.revision.clone();
    let editing = controls.editing.clone();
    let busy = controls.busy.clone();
    let save_busy = controls.save_busy.clone();
    let message = controls.message.clone();
    let last_saved = controls.last_saved.clone();
    let failed_busy = busy.clone();
    let failed_save_busy = save_busy.clone();
    let failed_revision = revision.clone();
    let failed_editing = editing.clone();
    let failed_title_draft = title_draft.clone();
    let failed_message = message.clone();
    let browser = browser::bindings();
    let finish = expr!(async |event: Event| {
        if browser.is_disposed() {
            return;
        }
        if !editing.get() {
            return;
        }
        let key = event.key.to_owned();
        let is_shortcut = if event.key == "s" {
            if event.ctrl_key { true } else { event.meta_key }
        } else {
            false
        };
        if key == "Escape" {
            event.prevent_default();
            title_draft.set(title.get());
            revision.set(revision.get() + 1_usize);
            editing.set(false);
            message.set("".to_owned());
            return;
        }
        if key != "" {
            if key != "Enter" {
                if !is_shortcut {
                    return;
                }
            }
        }
        if key == "Enter" {
            event.prevent_default();
        } else if is_shortcut {
            event.prevent_default();
        }
        if busy.get() {
            return;
        }
        let sent_raw = title_draft.get();
        let next_title = sent_raw.trim_ecmascript().to_owned();
        let current_title = title.get();
        if next_title.is_empty() {
            title_draft.set(current_title);
            editing.set(false);
            return;
        }
        if next_title == current_title {
            title_draft.set(current_title);
            editing.set(false);
            return;
        }

        let sent_revision = revision.get();
        let sent_seq = seq.get();
        busy.set(true);
        save_busy.set(true);
        editing.set(false);
        message.set("".to_owned());
        let failed_raw = sent_raw.clone();
        let _failed = || {
            if !browser.is_disposed() {
                failed_busy.set(false);
                failed_save_busy.set(false);
                if failed_revision.get() == sent_revision {
                    if failed_title_draft.get() == failed_raw {
                        failed_editing.set(true);
                    }
                }
                let error = "Couldn't save the page title. Your draft is still here.".to_owned();
                failed_message.set(error.clone());
                let _toast = ToastErrorRequest {
                    account_id: account,
                    message: error,
                };
                raw!(
                    "window.dispatchEvent(new CustomEvent('lific:native-toast-error',{detail:${_toast},cancelable:true}));",
                    ()
                );
            }
        };
        let _save = async || {
            if browser.is_disposed() {
                return;
            }
            let outcome = save_title(account, page_id, next_title.clone(), sent_seq).await;
            if browser.is_disposed() {
                return;
            }
            busy.set(false);
            save_busy.set(false);
            if outcome.status.is_ok() {
                let saved_seq = outcome.seq.unwrap();
                if saved_seq < seq.get() {
                    return;
                }
                let saved_title = outcome.title.clone().unwrap();
                let saved_body = outcome.content.clone().unwrap();
                let body_was_clean = body_draft.get() == body.get();
                title.set(saved_title);
                body.set(saved_body.clone());
                if body_was_clean {
                    body_draft.set(saved_body);
                }
                seq.set(saved_seq);
                last_saved.set(browser.local_time_now());
                if revision.get() == sent_revision {
                    if title_draft.get() == sent_raw {
                        title_draft.set(title.get());
                        editing.set(false);
                    }
                }
                message.set("Saved".to_owned());
            } else {
                let reason = outcome.status.unwrap_err();
                let error = if reason == "conflict" {
                    "This page changed elsewhere. Reload before saving again; your draft is still here.".to_owned()
                } else if reason == "reauth" {
                    "Please sign in again.".to_owned()
                } else if reason == "forbidden" {
                    "You can no longer edit this page.".to_owned()
                } else {
                    reason
                };
                if revision.get() == sent_revision {
                    if title_draft.get() == sent_raw {
                        editing.set(true);
                    }
                }
                message.set(error.clone());
                let _toast = ToastErrorRequest {
                    account_id: account,
                    message: error,
                };
                raw!(
                    "window.dispatchEvent(new CustomEvent('lific:native-toast-error',{detail:${_toast},cancelable:true}));",
                    ()
                );
            }
        };
        raw!(
            "Promise.resolve().then(()=>${_save}()).catch(()=>${_failed}());",
            ()
        );
    });

    let mut attributes = Attributes::with_capacity(3);
    attributes.insert(cx, "data-topcoat-on:input", input.into_evaluated_and_js().1);
    let finish_js = finish.into_evaluated_and_js().1;
    attributes.insert(cx, "data-topcoat-on:keydown", finish_js.clone());
    attributes.insert(cx, "data-topcoat-on:blur", finish_js);
    attributes
}
