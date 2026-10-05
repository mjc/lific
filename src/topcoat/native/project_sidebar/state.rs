//! One typed signal owner for both sidebar layout wrappers.
use super::{
    actions::{apply, finish, prepare, recover},
    model::State,
};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::Attributes,
};

#[derive(Clone)]
pub(super) struct Signals {
    pub(super) account: i64,
    pub(super) model: Signal<String>,
    pub(super) draft: Signal<String>,
    pub(super) revision: Signal<usize>,
    pub(super) busy: Signal<bool>,
    pub(super) error: Signal<String>,
    pub(super) menu_kind: Signal<String>,
    pub(super) menu_id: Signal<i64>,
    pub(super) menu_x: Signal<f64>,
    pub(super) menu_y: Signal<f64>,
    pub(super) focus: Signal<String>,
    pub(super) dom: Signal<String>,
    pub(super) frozen: Signal<String>,
}
pub(super) type Handles = (
    Signal<String>,
    Signal<String>,
    Signal<usize>,
    Signal<bool>,
    Signal<String>,
    Signal<String>,
    Signal<i64>,
    Signal<f64>,
    Signal<f64>,
    Signal<String>,
    Signal<String>,
    Signal<String>,
);
impl Signals {
    pub(super) fn new(cx: &Cx, state: State) -> Result<Self, crate::error::LificError> {
        let owner = cx.keyed(format!("native-sidebar-owner-{}", state.catalog.owner));
        let wire = super::actions::encode(&state)?;
        Ok(Self {
            account: state.catalog.owner,
            model: signal(&owner, || wire),
            draft: signal(&owner, String::new),
            revision: signal(&owner, || 0usize),
            busy: signal(&owner, || false),
            error: signal(&owner, String::new),
            menu_kind: signal(&owner, String::new),
            menu_id: signal(&owner, || 0i64),
            menu_x: signal(&owner, || 0.0),
            menu_y: signal(&owner, || 0.0),
            focus: signal(&owner, String::new),
            dom: signal(&owner, String::new),
            frozen: signal(&owner, String::new),
        })
    }
    pub(super) fn handles(&self) -> Handles {
        (
            self.model.clone(),
            self.draft.clone(),
            self.revision.clone(),
            self.busy.clone(),
            self.error.clone(),
            self.menu_kind.clone(),
            self.menu_id.clone(),
            self.menu_x.clone(),
            self.menu_y.clone(),
            self.focus.clone(),
            self.dom.clone(),
            self.frozen.clone(),
        )
    }
    pub(super) fn from_handles(account: i64, h: Handles) -> Self {
        Self {
            account,
            model: h.0,
            draft: h.1,
            revision: h.2,
            busy: h.3,
            error: h.4,
            menu_kind: h.5,
            menu_id: h.6,
            menu_x: h.7,
            menu_y: h.8,
            focus: h.9,
            dom: h.10,
            frozen: h.11,
        }
    }
}

pub(super) fn invoke(
    cx: &Cx,
    state: &Signals,
    command: &str,
    id: i64,
    value: String,
    event_name: &str,
) -> Attributes {
    let account = state.account;
    let command = command.to_owned();
    let keyboard = event_name == "keydown";
    let model = state.model.clone();
    let revision = state.revision.clone();
    let draft = state.draft.clone();
    let busy = state.busy.clone();
    let error = state.error.clone();
    let menu = state.menu_kind.clone();
    let focus = state.focus.clone();
    let dom = state.dom.clone();
    let frozen = state.frozen.clone();
    let recovery_model = model.clone();
    let recovery_revision = revision.clone();
    let recovery_focus = focus.clone();
    let recovery_frozen = frozen.clone();
    let recovery_error = error.clone();
    let recovery_busy = busy.clone();
    let unknown_busy = busy.clone();
    let unknown_error = error.clone();
    let handler = expr!(async |event: Event| {
        let allowed = if keyboard {
            if command == "cancel_edit" {
                event.key == "Escape"
            } else {
                if event.key == "Enter" {
                    true
                } else {
                    event.key == "Escape"
                }
            }
        } else {
            true
        };
        if allowed {
            event.prevent_default();
            event.stop_propagation();
            if !busy.get() {
                let current_command = if keyboard {
                    if event.key == "Escape" {
                        "cancel_edit".to_owned()
                    } else {
                        command.clone()
                    }
                } else {
                    command.clone()
                };
                let submitted = if current_command == "save_group" {
                    draft.get()
                } else {
                    value.clone()
                };
                let active = raw!(
                    "cx.hydrate(document.activeElement?.id ?? '')",
                    String::new()
                );
                let anchor = if focus.get().is_empty() {
                    active
                } else {
                    focus.get()
                };
                let _snapshot = raw!(
                    r#"cx.hydrate(JSON.stringify((() => {
                    const a=document.activeElement;
                    return {id:a?.id??'',start:a?.selectionStart??null,end:a?.selectionEnd??null,direction:a?.selectionDirection??'none',
                    regions:[...document.querySelectorAll('[data-native-sidebar-scroll]')].map(n=>({id:n.id,top:n.scrollTop,left:n.scrollLeft}))};
                })()))"#,
                    String::new()
                );
                if frozen.get().is_empty() {
                    dom.set(_snapshot);
                }
                busy.set(true);
                error.set("".to_owned());
                menu.set("".to_owned());
                let _unknown = || {
                    unknown_busy.set(false);
                    unknown_error.set(
                        "Couldn't confirm the change. Confirm it or reload the page before trying again.".to_owned(),
                    );
                };
                let _failed = async || {
                    if !recovery_frozen.get().is_empty() {
                        let result = recover(account, recovery_frozen.get()).await;
                        let merged =
                            finish(account, recovery_model.get(), recovery_frozen.get(), result)
                                .await;
                        if !merged.1.is_empty() {
                            recovery_focus.set(merged.1);
                        }
                        recovery_model.set(merged.0);
                        recovery_revision.increment();
                        recovery_frozen.set("".to_owned());
                        recovery_error.set("".to_owned());
                    } else {
                        recovery_error.set("Couldn't update the sidebar. Try again.".to_owned());
                    }
                    recovery_busy.set(false);
                };
                let _run = async || {
                    if !frozen.get().is_empty() {
                        let result = recover(account, frozen.get()).await;
                        let merged = finish(account, model.get(), frozen.get(), result).await;
                        if !merged.1.is_empty() {
                            focus.set(merged.1);
                        }
                        model.set(merged.0);
                        revision.increment();
                        frozen.set("".to_owned());
                        error.set("".to_owned());
                    } else {
                        let prepared = prepare(
                            account,
                            model.get(),
                            current_command.clone(),
                            id,
                            submitted,
                            draft.get(),
                            anchor,
                        )
                        .await;
                        if prepared.0 {
                            if current_command == "new_group" {
                                draft.set(prepared.2.clone());
                            } else if current_command == "rename_group" {
                                draft.set(prepared.2.clone());
                            }
                            if !prepared.3.is_empty() {
                                focus.set(prepared.3);
                            }
                            model.set(prepared.1);
                            revision.increment();
                            if !prepared.4.is_empty() {
                                frozen.set(prepared.4.clone());
                                let result = apply(account, prepared.4.clone()).await;
                                let merged = finish(account, model.get(), prepared.4, result).await;
                                if !merged.1.is_empty() {
                                    focus.set(merged.1);
                                }
                                model.set(merged.0);
                                revision.increment();
                                frozen.set("".to_owned());
                            }
                        } else {
                            error.set(prepared.1);
                        }
                    }
                    busy.set(false);
                };
                raw!(
                    "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}()).catch(()=>${_unknown}());",
                    ()
                );
            }
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        format!("data-topcoat-on:{event_name}"),
        handler.into_evaluated_and_js().1,
    );
    attributes
}

pub(super) fn open_menu(
    cx: &Cx,
    state: &Signals,
    kind: &str,
    id: i64,
    event_name: &str,
) -> Attributes {
    let kind = kind.to_owned();
    let keyboard = event_name == "keydown";
    let menu = state.menu_kind.clone();
    let selected = state.menu_id.clone();
    let x = state.menu_x.clone();
    let y = state.menu_y.clone();
    let focus = state.focus.clone();
    let handler = expr!(|event: Event| {
        let allowed = if keyboard {
            if event.key == "ContextMenu" {
                true
            } else {
                if event.shift_key {
                    event.key == "F10"
                } else {
                    false
                }
            }
        } else {
            true
        };
        if allowed {
            event.prevent_default();
            event.stop_propagation();
            let _target_id = event.current_target.id;
            let context_menu = event.event_type == "contextmenu";
            let measured = raw!(
                "cx.hydrate((()=>{const r=document.getElementById(${_target_id}.toString()).getBoundingClientRect();return [r.left,r.right,r.bottom];})())",
                (0.0, 0.0, 0.0)
            );
            let mobile = raw!(
                "cx.hydrate(window.matchMedia('(max-width:767px)').matches)",
                false
            );
            x.set(if context_menu {
                event.client_x
            } else {
                if mobile {
                    measured.0
                } else {
                    if kind == "create" {
                        measured.0
                    } else {
                        measured.1
                    }
                }
            });
            y.set(if context_menu {
                event.client_y
            } else {
                measured.2
            });
            focus.set(_target_id);
            selected.set(id);
            menu.set(kind.clone());
            raw!(
                "requestAnimationFrame(()=>document.querySelector('[data-native-sidebar-menu] button:not(:disabled),[data-native-sidebar-menu] a')?.focus());",
                ()
            );
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        format!("data-topcoat-on:{event_name}"),
        handler.into_evaluated_and_js().1,
    );
    attributes
}

/// Browser code only measures/focuses nodes; placement and key decisions stay Rust.
pub(super) fn menu_attributes(cx: &Cx, state: &Signals) -> Attributes {
    let x = state.menu_x.clone();
    let y = state.menu_y.clone();
    let kind = state.menu_kind.clone();
    let focus = state.focus.clone();
    let menu = state.menu_kind.clone();
    let key_focus = state.focus.clone();
    let mount = expr!(|_event: Event| {
        let measured = raw!(
            "cx.hydrate((()=>{const r=document.getElementById('native-sidebar-menu').getBoundingClientRect();return [r.width,r.height,innerWidth,innerHeight];})())",
            (0.0, 0.0, 0.0, 0.0)
        );
        let width = measured.0;
        let height = measured.1;
        let right = measured.2 - 8.0;
        let bottom = measured.3 - 8.0;
        let left = if x.get() + width > right {
            x.get() - width
        } else {
            x.get()
        };
        let top = if y.get() + height > bottom {
            y.get() - height
        } else {
            y.get()
        };
        let max_left = right - width;
        let max_top = bottom - height;
        let left = if left > max_left { max_left } else { left };
        let top = if top > max_top { max_top } else { top };
        x.set(if left < 8.0 { 8.0 } else { left });
        y.set(if top < 8.0 { 8.0 } else { top });
        let _close = |restore: topcoat::runtime::BoolSurrogate| {
            kind.set("".to_owned());
            if restore {
                let _id = focus.get();
                raw!("document.getElementById(${_id}.toString())?.focus();", ());
            }
        };
        let _keys = |event: Event| {
            if menu.get().is_empty() {
                return;
            }
            if event.key == "Escape" {
                event.prevent_default();
                event.stop_immediate_propagation();
                menu.set("".to_owned());
                let _id = key_focus.get();
                raw!("document.getElementById(${_id}.toString())?.focus();", ());
            } else if event.key == "Tab" {
                event.prevent_default();
                event.stop_immediate_propagation();
                menu.set("".to_owned());
                let _id = key_focus.get();
                raw!("document.getElementById(${_id}.toString())?.focus();", ());
            } else {
                let arrow = if event.key == "ArrowDown" {
                    true
                } else {
                    if event.key == "ArrowUp" {
                        true
                    } else {
                        if event.key == "Home" {
                            true
                        } else {
                            event.key == "End"
                        }
                    }
                };
                if arrow {
                    let inside = raw!(
                        "cx.hydrate(document.getElementById('native-sidebar-menu').contains(document.activeElement))",
                        false
                    );
                    if !inside {
                        return;
                    }
                    event.prevent_default();
                    event.stop_immediate_propagation();
                    let measured = raw!(
                        "cx.hydrate((()=>{const n=[...document.querySelectorAll('[data-native-sidebar-menu] button:not(:disabled),[data-native-sidebar-menu] a')];return [n.length,n.indexOf(document.activeElement)];})())",
                        (0_i64, -1_i64)
                    );
                    if measured.0 > 0_i64 {
                        let last = measured.0 - 1_i64;
                        let _index = if event.key == "Home" {
                            0_i64
                        } else {
                            if event.key == "End" {
                                last
                            } else {
                                if event.key == "ArrowDown" {
                                    if measured.1 >= last {
                                        0_i64
                                    } else {
                                        measured.1 + 1_i64
                                    }
                                } else {
                                    if measured.1 <= 0_i64 {
                                        last
                                    } else {
                                        measured.1 - 1_i64
                                    }
                                }
                            }
                        };
                        raw!(
                            "[...document.querySelectorAll('[data-native-sidebar-menu] button:not(:disabled),[data-native-sidebar-menu] a')][Number(${_index}.toString())]?.focus();",
                            ()
                        );
                    }
                }
            }
        };
        raw!(
            r#"(()=>{
            const menu=document.getElementById('native-sidebar-menu');
            menu.querySelector('button:not(:disabled),a')?.focus();
            const click=e=>{if(cx.abortSignal.aborted)return;if(!menu.contains(e.target))${_close}(cx.hydrate(document.activeElement===document.body||menu.contains(document.activeElement)));};
            const scroll=e=>{if(cx.abortSignal.aborted||menu.contains(e.target))return;${_close}(cx.hydrate(true));};
            window.addEventListener('keydown',event=>${_keys}(cx.event(event)),{capture:true,signal:cx.abortSignal});
            document.addEventListener('click',click);document.addEventListener('scroll',scroll,true);window.addEventListener('resize',scroll);
            cx.abortSignal.addEventListener('abort',()=>{document.removeEventListener('click',click);document.removeEventListener('scroll',scroll,true);window.removeEventListener('resize',scroll);},{once:true});
        })();"#,
            ()
        );
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(cx, "data-topcoat-on:mount", mount.into_evaluated_and_js().1);
    attributes
}

/// Consume return focus only after the owning shard adopts its actual visible trigger.
pub(super) fn restore_focus(cx: &Cx, state: &Signals, id: String, ready: bool) -> Attributes {
    focus_on_mount(cx, state, id, ready, false)
}
pub(super) fn restore_editor_focus(
    cx: &Cx,
    state: &Signals,
    id: String,
    ready: bool,
) -> Attributes {
    focus_on_mount(cx, state, id, ready, true)
}
fn focus_on_mount(cx: &Cx, state: &Signals, id: String, ready: bool, initial: bool) -> Attributes {
    let focus = state.focus.clone();
    let dom = state.dom.clone();
    let handler = expr!(|_event: Event| {
        if ready {
            if focus.get() == id {
                let _snapshot = dom.get();
                let restored = raw!(
                    "cx.hydrate((()=>{const node=document.getElementById(${id}.toString());if(!node?.getClientRects().length||node.closest('[hidden],[inert]'))return false;node.focus();try{const saved=JSON.parse(${_snapshot}.toString());if(saved.id===node.id&&saved.start!==null)node.setSelectionRange(saved.start,saved.end,saved.direction);for(const region of saved.regions??[]){const actual=document.getElementById(region.id);if(actual){actual.scrollTop=region.top;actual.scrollLeft=region.left;}}}catch{}return document.activeElement===node;})())",
                    false
                );
                if restored {
                    focus.set("".to_owned());
                }
            } else {
                if initial {
                    raw!(
                        "if(!document.activeElement?.closest('[data-native-sidebar-projects]'))requestAnimationFrame(()=>{const node=document.getElementById(${id}.toString());if(node?.offsetParent&&!node.closest('[hidden],[inert]')){node.focus();node.select();}});",
                        ()
                    );
                }
            }
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
    attributes
}
