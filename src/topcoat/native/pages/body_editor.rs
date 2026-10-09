//! Native Page body mode controls and content-only commits.
use super::super::browser;
use super::editor_state::EditorState;
use topcoat::{
    context::Cx,
    runtime::{Event, Expr, Js, Signal, expr},
    view::Attributes,
};

pub(super) use super::editor_save::Controls;

fn edit_callback(
    editor: &EditorState,
    controls: &Controls,
    focus_id: String,
) -> Expr<impl FnOnce() + use<>> {
    let browser = browser::bindings();
    let body = editor.body.clone();
    let draft = editor.body_draft.clone();
    let editing = controls.editing.clone();
    let revision = controls.revision.clone();
    let busy = controls.busy.clone();
    let message = controls.message.clone();
    expr!(|| {
        let focus_id = focus_id.clone();
        if !browser.is_disposed() {
            if !busy.get() {
                draft.set(body.get());
                revision.set(revision.get() + 1_usize);
                editing.set(true);
                message.set("".to_owned());
                let next_revision = revision.get();
                let focus = || {
                    if !browser.is_disposed() {
                        if editing.get() {
                            if revision.get() == next_revision {
                                browser.focus_id(focus_id.clone());
                            }
                        }
                    }
                };
                browser.microtask(focus);
            }
        }
    })
}

fn cancel_callback(editor: &EditorState, controls: &Controls) -> Expr<impl Fn() + use<>> {
    let canonical = editor.body.clone();
    let draft = editor.body_draft.clone();
    let editing = controls.editing.clone();
    let revision = controls.revision.clone();
    let message = controls.message.clone();
    let browser = browser::bindings();
    expr!(|| {
        if !browser.is_disposed() {
            draft.set(canonical.get());
            revision.set(revision.get() + 1_usize);
            editing.set(false);
            message.set("".to_owned());
        }
    })
}

fn click_handler<C>(callback: &Expr<C>) -> Js
where
    C: FnOnce(),
{
    let browser = browser::bindings();
    let factory = super::callback_factory::<C, _>(expr!(|_event: Event, callback| {
        if !browser.is_disposed() {
            browser.call0(callback);
        }
    }));
    Js::builder()
        .source("event => (")
        .expression(&factory)
        .source(")(event, ")
        .expression(callback)
        .source(")")
        .build()
}

fn mode_handler<C, D>(
    editing: Signal<bool>,
    commit: &Expr<C>,
    edit: &Expr<D>,
    target_editing: bool,
) -> Js
where
    C: FnOnce(),
    D: FnOnce(),
{
    let browser = browser::bindings();
    let factory = super::callback_pair_factory::<C, D, _>(expr!(|_event: Event, commit, edit| {
        if !browser.is_disposed() {
            if target_editing {
                if !editing.get() {
                    browser.call0(edit);
                }
            } else {
                if editing.get() {
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
        .expression(edit)
        .source(")")
        .build()
}

pub(super) fn mode_attributes<C: FnOnce()>(
    cx: &Cx,
    editor: &EditorState,
    controls: &Controls,
    focus_id: String,
    commit: &Expr<C>,
    target_editing: bool,
) -> Attributes {
    let edit = edit_callback(editor, controls, focus_id);
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        mode_handler(controls.editing.clone(), commit, &edit, target_editing),
    );
    attrs
}

fn keydown_handler<C, D>(commit: &Expr<C>, cancel: &Expr<D>) -> Js
where
    C: FnOnce(),
    D: FnOnce(),
{
    let browser = browser::bindings();
    let factory = super::callback_pair_factory::<C, D, _>(expr!(|event: Event, commit, cancel| {
        if !browser.is_disposed() {
            if event.key == "Escape" {
                event.prevent_default();
                browser.call0(cancel);
            } else {
                let is_shortcut = if event.key == "s" {
                    if event.ctrl_key { true } else { event.meta_key }
                } else {
                    false
                };
                if is_shortcut {
                    event.prevent_default();
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
    let browser = browser::bindings();
    let draft = editor.body_draft.clone();
    let editing = controls.editing.clone();
    let revision = controls.revision.clone();
    let message = controls.message.clone();
    let input = expr!(|event: Event| {
        if !browser.is_disposed() {
            draft.set(event.target.value.to_owned());
            editing.set(true);
            revision.set(revision.get() + 1_usize);
            message.set("".to_owned());
        }
    });
    let cancel = cancel_callback(editor, controls);
    let keydown = keydown_handler(commit, &cancel);
    let blur = expr!(|_event: Event| {});
    let mut attrs = Attributes::with_capacity(3);
    attrs.insert(cx, "data-topcoat-on:input", input.into_evaluated_and_js().1);
    attrs.insert(cx, "data-topcoat-on:keydown", keydown);
    attrs.insert(cx, "data-topcoat-on:blur", blur.into_evaluated_and_js().1);
    attrs
}

pub(super) fn save_attributes<C: FnOnce()>(cx: &Cx, commit: &Expr<C>) -> Attributes {
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(cx, "data-topcoat-on:click", click_handler(commit));
    attrs
}

pub(super) fn cancel_attributes(cx: &Cx, editor: &EditorState, controls: &Controls) -> Attributes {
    let cancel = cancel_callback(editor, controls);
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(cx, "data-topcoat-on:click", click_handler(&cancel));
    attrs
}

fn keyboard_mount_handler<C: Fn(Event)>(listener: &Expr<C>) -> Js {
    let browser = browser::bindings();
    let factory = super::callback_factory::<C, _>(expr!(|_event: Event, listener| {
        browser.window_listener("keydown".to_owned(), listener);
    }));
    Js::builder()
        .source("event => (")
        .expression(&factory)
        .source(")(event, ")
        .expression(listener)
        .source(")")
        .build()
}

pub(super) fn keyboard_attributes(
    cx: &Cx,
    editor: &EditorState,
    controls: &Controls,
    title_editing: Signal<bool>,
    focus_id: String,
) -> Attributes {
    let browser = browser::bindings();
    let editing = controls.editing.clone();
    let busy = controls.busy.clone();
    let body = editor.body.clone();
    let draft = editor.body_draft.clone();
    let revision = controls.revision.clone();
    let message = controls.message.clone();
    let listener = expr!(|event: Event| {
        let editing = raw!("${editing}", &editing);
        let revision = raw!("${revision}", &revision);
        let focus_id = focus_id.clone();
        if !browser.is_disposed() {
            let is_edit_key = if event.key == "e" {
                true
            } else {
                event.key == "E"
            };
            if is_edit_key {
                if !event.ctrl_key {
                    if !event.meta_key {
                        if !event.alt_key {
                            let typing = browser.is_typing_context();
                            let overlay_selector =
                                "[role=dialog],[data-native-issue-peek],[data-native-context-menu]"
                                    .to_owned();
                            let visible_overlay = browser.has_visible_match(overlay_selector);
                            if !typing {
                                if !visible_overlay {
                                    if !busy.get() {
                                        if !editing.get() {
                                            if !title_editing.get() {
                                                event.prevent_default();
                                                draft.set(body.get());
                                                revision.set(revision.get() + 1_usize);
                                                editing.set(true);
                                                message.set("".to_owned());
                                                let opened_revision = revision.get();
                                                let focus = || {
                                                    if !browser.is_disposed() {
                                                        if editing.get() {
                                                            if revision.get() == opened_revision {
                                                                browser.focus_id(focus_id.clone());
                                                            }
                                                        }
                                                    }
                                                };
                                                browser.microtask(focus);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    let mount = keyboard_mount_handler(&listener);
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(cx, "data-topcoat-on:mount", mount);
    attrs
}
