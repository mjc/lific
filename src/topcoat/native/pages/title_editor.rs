//! Native Page title editing controls.
use super::super::browser;
use super::editor_state::EditorState;
use topcoat::{
    context::Cx,
    runtime::{Event, Expr, Js, Signal, expr},
    view::Attributes,
};

pub(super) use super::editor_save::Controls;

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
        let focus_id = focus_id.clone();
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

fn cancel_callback(editor: &EditorState, controls: &Controls) -> Expr<impl Fn() + use<>> {
    let browser = browser::bindings();
    let title = editor.title.clone();
    let title_draft = editor.title_draft.clone();
    let editing = controls.editing.clone();
    let revision = controls.revision.clone();
    let message = controls.message.clone();
    expr!(|| {
        if !browser.is_disposed() {
            title_draft.set(title.get());
            revision.set(revision.get() + 1_usize);
            editing.set(false);
            message.set("".to_owned());
        }
    })
}

fn finish_handler<C: FnOnce(), D: Fn()>(
    editing: Signal<bool>,
    commit: &Expr<C>,
    cancel: &Expr<D>,
    keyboard: bool,
) -> Js {
    let browser = browser::bindings();
    let factory = super::callback_pair_factory::<C, D, _>(expr!(|event: Event, commit, cancel| {
        if !browser.is_disposed() {
            if editing.get() {
                if keyboard {
                    if event.key == "Escape" {
                        event.prevent_default();
                        browser.call0(cancel);
                    } else {
                        let is_shortcut = if event.key == "s" {
                            if event.ctrl_key { true } else { event.meta_key }
                        } else {
                            false
                        };
                        if event.key == "Enter" {
                            event.prevent_default();
                            browser.call0(commit);
                        } else if is_shortcut {
                            event.prevent_default();
                            browser.call0(commit);
                        }
                    }
                } else {
                    browser.call0(commit);
                }
            }
        }
    }));
    Js::builder()
        .source("event => (")
        .expression(&factory)
        .source(")(event, ")
        .expression(commit)
        .source(", ")
        .expression(cancel)
        .source(")")
        .build()
}

pub(super) fn input_attributes<C: FnOnce()>(
    cx: &Cx,
    editor: &EditorState,
    controls: &Controls,
    commit: &Expr<C>,
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
    let cancel = cancel_callback(editor, controls);
    let finish_key = finish_handler(controls.editing.clone(), commit, &cancel, true);
    let finish_blur = finish_handler(controls.editing.clone(), commit, &cancel, false);

    let mut attributes = Attributes::with_capacity(3);
    attributes.insert(cx, "data-topcoat-on:input", input.into_evaluated_and_js().1);
    attributes.insert(cx, "data-topcoat-on:keydown", finish_key);
    attributes.insert(cx, "data-topcoat-on:blur", finish_blur);
    attributes
}
