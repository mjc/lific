//! Shared field commit and canonical Page reconciliation.
use super::super::super::runtime::whitespace::StrEcmaTrimExt;
use super::super::browser;
use super::super::deferred_delete::ToastErrorRequest;
use super::actions::{save_content, save_title};
use super::editor_state::EditorState;
use topcoat::runtime::{Expr, Signal, expr};

pub(super) struct Controls {
    pub(super) editing: Signal<bool>,
    pub(super) revision: Signal<usize>,
    pub(super) busy: Signal<bool>,
    pub(super) save_busy: Signal<bool>,
    pub(super) message: Signal<String>,
    pub(super) last_saved: Signal<String>,
}

#[derive(Clone, Copy)]
pub(super) enum Field {
    Title,
    Content,
}

pub(super) fn commit_callback(
    account: i64,
    page_id: i64,
    field: Field,
    editor: &EditorState,
    controls: &Controls,
) -> Expr<impl FnOnce() + use<>> {
    let is_title = matches!(field, Field::Title);
    let revision = controls.revision.clone();
    let editing = controls.editing.clone();
    let busy = controls.busy.clone();
    let save_busy = controls.save_busy.clone();
    let message = controls.message.clone();
    let last_saved = controls.last_saved.clone();
    let title = editor.title.clone();
    let body = editor.body.clone();
    let title_draft = editor.title_draft.clone();
    let body_draft = editor.body_draft.clone();
    let seq = editor.seq.clone();
    let failed_revision = revision.clone();
    let failed_editing = editing.clone();
    let failed_busy = busy.clone();
    let failed_save_busy = save_busy.clone();
    let failed_message = message.clone();
    let failed_title_draft = title_draft.clone();
    let failed_body_draft = body_draft.clone();
    let browser = browser::bindings();
    expr!(|| {
        if !browser.is_disposed() {
            if !busy.get() {
                let sent_raw = if is_title {
                    title_draft.get()
                } else {
                    body_draft.get()
                };
                let next_value = if is_title {
                    sent_raw.trim_ecmascript().to_owned()
                } else {
                    sent_raw.clone()
                };
                let current_value = if is_title { title.get() } else { body.get() };
                let blank_title = if is_title {
                    next_value.is_empty()
                } else {
                    false
                };
                if blank_title {
                    title_draft.set(title.get());
                    editing.set(false);
                } else if next_value == current_value {
                    if is_title {
                        title_draft.set(title.get());
                    } else {
                        body_draft.set(body.get());
                    }
                    editing.set(false);
                } else {
                    let sent_revision = revision.get();
                    let sent_seq = seq.get();
                    busy.set(true);
                    save_busy.set(true);
                    if is_title {
                        editing.set(false);
                    }
                    message.set("".to_owned());
                    let failed_raw = sent_raw.clone();
                    let _failed = || {
                        if !browser.is_disposed() {
                            failed_busy.set(false);
                            failed_save_busy.set(false);
                            let current_draft = if is_title {
                                failed_title_draft.get()
                            } else {
                                failed_body_draft.get()
                            };
                            if failed_revision.get() == sent_revision {
                                if current_draft == failed_raw {
                                    if is_title {
                                        failed_editing.set(true);
                                    }
                                }
                            }
                            let error = if is_title {
                                "Couldn't save the page title. Your draft is still here.".to_owned()
                            } else {
                                "Couldn't save the page content. Your draft is still here."
                                    .to_owned()
                            };
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
                        if !browser.is_disposed() {
                            let outcome = if is_title {
                                save_title(account, page_id, next_value.clone(), sent_seq).await
                            } else {
                                save_content(account, page_id, next_value.clone(), sent_seq).await
                            };
                            if !browser.is_disposed() {
                                busy.set(false);
                                save_busy.set(false);
                                if outcome.status.is_ok() {
                                    let saved_seq = outcome.seq.unwrap();
                                    if saved_seq >= seq.get() {
                                        let title_was_clean = if is_title {
                                            false
                                        } else {
                                            title_draft.get() == title.get()
                                        };
                                        let body_was_clean = if is_title {
                                            body_draft.get() == body.get()
                                        } else {
                                            false
                                        };
                                        let saved_title = outcome.title.clone().unwrap();
                                        let saved_body = outcome.content.clone().unwrap();
                                        title.set(saved_title.clone());
                                        body.set(saved_body.clone());
                                        if title_was_clean {
                                            title_draft.set(saved_title);
                                        }
                                        if body_was_clean {
                                            body_draft.set(saved_body);
                                        }
                                        seq.set(saved_seq);
                                        last_saved.set(browser.local_time_now());
                                        let submitted_draft = if is_title {
                                            title_draft.get()
                                        } else {
                                            body_draft.get()
                                        };
                                        if revision.get() == sent_revision {
                                            if submitted_draft == sent_raw {
                                                if is_title {
                                                    title_draft.set(title.get());
                                                } else {
                                                    body_draft.set(body.get());
                                                }
                                                editing.set(false);
                                            }
                                        }
                                        message.set("Saved".to_owned());
                                    }
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
                                    let submitted_draft = if is_title {
                                        title_draft.get()
                                    } else {
                                        body_draft.get()
                                    };
                                    if revision.get() == sent_revision {
                                        if submitted_draft == sent_raw {
                                            if is_title {
                                                editing.set(true);
                                            }
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
                            }
                        }
                    };
                    raw!(
                        "Promise.resolve().then(()=>${_save}()).catch(()=>${_failed}());",
                        ()
                    );
                }
            }
        }
    })
}
