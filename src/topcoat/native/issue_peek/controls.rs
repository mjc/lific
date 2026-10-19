use super::*;
use data::save;
use topcoat::{
    runtime::{Event, expr, signal},
    view::Attributes,
};

#[derive(Clone)]
pub(super) struct State {
    pub(super) seq: Signal<i64>,
    pub(super) title: Signal<String>,
    pub(super) draft: Signal<String>,
    pub(super) editing: Signal<bool>,
    pub(super) status: Signal<String>,
    pub(super) priority: Signal<String>,
    pub(super) module: Signal<String>,
    pub(super) updated: Signal<String>,
    pub(super) busy: Signal<bool>,
    pub(super) error: Signal<String>,
    pub(super) undo_field: Signal<String>,
    pub(super) undo_value: Signal<String>,
}
impl State {
    pub(super) fn new(cx: &Cx, issue: &crate::db::models::Issue) -> Self {
        Self {
            seq: signal(cx, || issue.seq),
            title: signal(cx, || issue.title.clone()),
            draft: signal(cx, || issue.title.clone()),
            editing: signal(cx, || false),
            status: signal(cx, || issue.status.to_string()),
            priority: signal(cx, || issue.priority.to_string()),
            module: signal(cx, || {
                issue
                    .module_id
                    .map_or_else(String::new, |id| id.to_string())
            }),
            updated: signal(cx, || issue.updated_at.clone()),
            busy: signal(cx, || false),
            error: signal(cx, String::new),
            undo_field: signal(cx, String::new),
            undo_value: signal(cx, String::new),
        }
    }
}

pub(super) fn edit(
    cx: &Cx,
    state: &State,
    account: i64,
    identifier: &str,
    field: &str,
    event_name: &str,
    undo: bool,
) -> Attributes {
    let identifier = identifier.to_owned();
    let field = field.to_owned();
    let keyboard = event_name == "keydown";
    let seq = state.seq.clone();
    let title = state.title.clone();
    let draft = state.draft.clone();
    let editing = state.editing.clone();
    let status = state.status.clone();
    let priority = state.priority.clone();
    let module = state.module.clone();
    let updated = state.updated.clone();
    let busy = state.busy.clone();
    let error = state.error.clone();
    let undo_field = state.undo_field.clone();
    let undo_value = state.undo_value.clone();
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let handler = expr!(|event: Event| {
        let key = raw!("cx.hydrate(${event}.inner.key ?? '')", String::new());
        if key == "Escape" {
            event.prevent_default();
            editing.set(false);
            draft.set(title.get());
        } else {
            let modifier = raw!(
                "cx.hydrate(Boolean(${event}.inner.ctrlKey || ${event}.inner.metaKey))",
                false
            );
            let trigger = if keyboard {
                if key == "Enter" {
                    true
                } else {
                    if key == "s" { modifier } else { false }
                }
            } else {
                true
            };
            let allowed = if field == "title" {
                editing.get()
            } else {
                true
            };
            if trigger {
                if allowed {
                    if !busy.get() {
                        event.prevent_default();
                        let field = if undo { undo_field.get() } else { field };
                        if undo {
                            undo_field.set("".to_owned());
                        }
                        let before = if field == "title" {
                            title.get()
                        } else {
                            if field == "status" {
                                status.get()
                            } else {
                                if field == "priority" {
                                    priority.get()
                                } else {
                                    module.get()
                                }
                            }
                        };
                        let value = if undo {
                            undo_value.get()
                        } else {
                            if field == "title" {
                                draft.get().trim().to_owned()
                            } else {
                                raw!("cx.hydrate(${event}.inner.target.value)", String::new())
                            }
                        };
                        raw!(
                            "if (${event}.inner.target.tagName === 'SELECT') ${event}.inner.target.value=${before}.toString();",
                            ()
                        );
                        let changed = if field == "title" {
                            if value.is_empty() {
                                false
                            } else {
                                value != before
                            }
                        } else {
                            value != before
                        };
                        if changed {
                            busy.set(true);
                            error.set("".to_owned());
                            if field == "title" {
                                editing.set(false);
                            }
                            let _failed = || {
                                failed_busy.set(false);
                                failed_error.set("Unable to save. Try again.".to_owned());
                            };
                            let _run = async || {
                                let outcome =
                                    save(account, identifier, field.clone(), value, seq.get())
                                        .await;
                                busy.set(false);
                                if outcome.2 >= seq.get() {
                                    seq.set(outcome.2);
                                    title.set(outcome.3.clone());
                                    status.set(outcome.4);
                                    priority.set(outcome.5);
                                    module.set(outcome.6);
                                    updated.set(outcome.7);
                                    error.set(outcome.1);
                                    if outcome.0 {
                                        editing.set(false);
                                        draft.set(outcome.3);
                                        if undo {
                                            undo_field.set("".to_owned());
                                        } else {
                                            if field != "title" {
                                                undo_field.set(field);
                                                undo_value.set(before);
                                            }
                                        }
                                    }
                                }
                            };
                            raw!(
                                "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                                ()
                            );
                        } else {
                            editing.set(false);
                            draft.set(title.get());
                        }
                    }
                }
            }
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        format!("data-topcoat-on:{event_name}"),
        handler.into_evaluated_and_js().1,
    );
    attrs
}

pub(super) fn clipboard(cx: &Cx, identifier: &str) -> Attributes {
    let identifier = identifier.to_owned();
    let handler = expr!(|_event: Event| {
        raw!(
            "navigator.clipboard.writeText(${identifier}.toString()).catch(()=>{});",
            ()
        );
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

pub(super) fn dismiss(cx: &Cx, close: Signal<String>) -> Attributes {
    let handler = expr!(|_event: Event| {
        raw!(
            r#"const panel = ${_event}.inner.target;
            const previous = document.activeElement;
            (panel.querySelector('[aria-label="Close preview"]') ?? panel).focus({preventScroll:true});
            cx.abortSignal.addEventListener('abort', () => {
                if (previous?.isConnected && (panel.contains(document.activeElement) || document.activeElement === document.body)) {
                    previous.focus({preventScroll:true});
                }
            }, {once:true});"#,
            ()
        );
        let _key = |event: Event| {
            let key = raw!("cx.hydrate(${event}.inner.key)", String::new());
            if key == "Escape" {
                event.prevent_default();
                close.set("".to_owned());
            } else {
                if key == "Tab" {
                    raw!(
                        r#"const controls = Array.from(panel.querySelectorAll('a[href],button,input,select,textarea,[tabindex]'))
                            .filter(node => !node.disabled && node.tabIndex !== -1 && !node.closest('[hidden],[inert]') && node.getClientRects().length);
                        const first = controls[0] ?? panel;
                        const last = controls.at(-1) ?? panel;
                        const active = document.activeElement;
                        const backwards = ${event}.inner.shiftKey;
                        if (!panel.contains(active) || active === panel || (backwards ? active === first : active === last)) {
                            ${event}.inner.preventDefault();
                            (backwards ? last : first).focus({preventScroll:true});
                        }"#,
                        ()
                    );
                }
            }
        };
        raw!(
            "window.addEventListener('keydown',event=>${_key}(cx.event(event)),{signal:cx.abortSignal});",
            ()
        );
        raw!(
            "const reduced=matchMedia('(prefers-reduced-motion: reduce)').matches; const mobile=innerWidth<768; if(!reduced)panel.animate([{transform:mobile?'translateY(480px)':'translateX(480px)'},{transform:'none'}],{duration:240,easing:'ease-out'});",
            ()
        );
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
    attrs
}
