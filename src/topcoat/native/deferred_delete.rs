//! Shared account-level writes and notifications across native navigation.
//! Four slots retain deadlines and one-shot Undo across native navigation.
use super::super::runtime::{
    procedure::ProcedureKeepaliveExt,
    signal_vec::{SignalVecExt, VecPositionExt},
};
use super::icons::UiIcon;
use super::issue_edit::labels::{
    LabelReply, LabelReplyValue, LabelRequestValue, create_label, update_labels,
};
use super::issue_edit::module_assignment::{ModuleAssignmentReply, ModuleRequest, assign_module};
use super::pages::labels_action::{
    Reply as PageLabelReply, ReplyValue as PageLabelReplyValue,
    RequestValue as PageLabelRequestValue, update_labels as update_page_labels,
};
use super::{issue_edit::delete::commit_delete, transport};
use topcoat::{
    context::Cx,
    runtime::{
        BoolSurrogate, Event, I64Surrogate, Js, Signal, StringSurrogate, Surrogated,
        UsizeSurrogate, expr, record, shard, signal,
    },
    view::{BoxView, ViewExt, view},
};

pub(crate) const TOAST_CAPACITY: usize = 4;

#[record]
#[derive(Clone)]
pub(crate) struct ToastRequest {
    pub account_id: i64,
    pub message: String,
}

pub(crate) type ToastErrorRequest = ToastRequest;
type ToastRequestSurrogate = <ToastRequest as Surrogated>::Surrogate;

type OwnerHandlesSurrogate<'a> = <&'a OwnerHandles as Surrogated>::Surrogate;
type ModuleRequestSurrogate = <ModuleRequest as Surrogated>::Surrogate;
type ModuleAssignmentReplySurrogate = <ModuleAssignmentReply as Surrogated>::Surrogate;
type ToastErrorRequestSurrogate = ToastRequestSurrogate;

// Each owner supplies handles once; the shared factory contains the Rust-authored actions.
#[record]
#[derive(Clone)]
struct OwnerHandles {
    activated: Signal<bool>,
    id: [Signal<usize>; TOAST_CAPACITY],
    issue: [Signal<i64>; TOAST_CAPACITY],
    claimed: [Signal<bool>; TOAST_CAPACITY],
    identifier: [Signal<String>; TOAST_CAPACITY],
    detail: [Signal<String>; TOAST_CAPACITY],
    message: [Signal<String>; TOAST_CAPACITY],
    kind: [Signal<String>; TOAST_CAPACITY],
    undo: [Signal<bool>; TOAST_CAPACITY],
    remaining: [Signal<f64>; TOAST_CAPACITY],
    started: [Signal<f64>; TOAST_CAPACITY],
    hovered: [Signal<bool>; TOAST_CAPACITY],
    focused: [Signal<bool>; TOAST_CAPACITY],
    module_request: [Signal<Option<ModuleRequest>>; TOAST_CAPACITY],
    pending_slot: Signal<usize>,
    next_id: Signal<usize>,
    choose_index: Signal<usize>,
    choose_best: Signal<usize>,
    choose_oldest: Signal<usize>,
    remove_index: Signal<usize>,
    pending_issues: Signal<Vec<i64>>,
    module_pending: Signal<Vec<i64>>,
    label_pending: Signal<Vec<i64>>,
    page_label_pending: Signal<Vec<i64>>,
    mount: String,
}

pub(crate) struct ToastSlot {
    pub id: Signal<usize>,
    pub issue: Signal<i64>,
    pub claimed: Signal<bool>,
    pub identifier: Signal<String>,
    pub detail: Signal<String>,
    pub message: Signal<String>,
    pub kind: Signal<String>,
    pub undo: Signal<bool>,
    pub remaining: Signal<f64>,
    pub started: Signal<f64>,
    pub hovered: Signal<bool>,
    pub focused: Signal<bool>,
    pub module_request: Signal<Option<ModuleRequest>>,
}
impl ToastSlot {
    fn new(cx: &Cx) -> Self {
        Self {
            id: signal(cx, || 0_usize),
            issue: signal(cx, || 0_i64),
            claimed: signal(cx, || true),
            identifier: signal(cx, String::new),
            detail: signal(cx, String::new),
            message: signal(cx, String::new),
            kind: signal(cx, || "info".into()),
            undo: signal(cx, || false),
            remaining: signal(cx, || 0.),
            started: signal(cx, || 0.),
            hovered: signal(cx, || false),
            focused: signal(cx, || false),
            module_request: signal(cx, || None::<ModuleRequest>),
        }
    }
}

pub(crate) fn slots(cx: &Cx) -> [ToastSlot; TOAST_CAPACITY] {
    std::array::from_fn(|index| ToastSlot::new(&cx.keyed(index)))
}

pub(crate) fn owner<'a>(
    cx: &'a Cx,
    state_cx: &Cx,
    account_id: i64,
    project: &str,
    needs_notifications: bool,
    pending_issues: Signal<Vec<i64>>,
) -> BoxView<'a> {
    let owner_key = format!("{account_id}:{project}");
    // Carry only a small account sentinel until a route needs notifications.
    // Afterwards activation keeps pending work and Undo alive on common routes.
    let activated = signal(state_cx, || false);
    if project.is_empty() && !needs_notifications && !activated.get_untracked() {
        return view! {
            cx =>
            <div
                id="native-deferred-delete-owner"
                data-native-delete-owner=(owner_key)
                data-native-action-account=(account_id.to_string())
            />
        }
        .boxed();
    }
    let slots = slots(state_cx);
    let pending_slot = signal(state_cx, || 4_usize);
    let next_id = signal(state_cx, || 0_usize);
    let choose_index = signal(state_cx, || 0_usize);
    let choose_best = signal(state_cx, || 0_usize);
    let choose_oldest = signal(state_cx, || usize::MAX);
    let remove_index = signal(state_cx, || 0_usize);
    let mount = transport::trusted_mount(cx).unwrap_or_default().to_owned();
    let id = slots.each_ref().map(|slot| slot.id.clone());
    let issue = slots.each_ref().map(|slot| slot.issue.clone());
    let claimed = slots.each_ref().map(|slot| slot.claimed.clone());
    let identifier = slots.each_ref().map(|slot| slot.identifier.clone());
    let detail = slots.each_ref().map(|slot| slot.detail.clone());
    let message = slots.each_ref().map(|slot| slot.message.clone());
    let kind = slots.each_ref().map(|slot| slot.kind.clone());
    let undo = slots.each_ref().map(|slot| slot.undo.clone());
    let remaining = slots.each_ref().map(|slot| slot.remaining.clone());
    let started = slots.each_ref().map(|slot| slot.started.clone());
    let hovered = slots.each_ref().map(|slot| slot.hovered.clone());
    let focused = slots.each_ref().map(|slot| slot.focused.clone());
    let module_request = slots.each_ref().map(|slot| slot.module_request.clone());
    let module_pending = signal(state_cx, Vec::<i64>::new);
    let label_pending = signal(state_cx, Vec::<i64>::new);
    let page_label_pending = signal(state_cx, Vec::<i64>::new);
    let handles = OwnerHandles {
        activated,
        id,
        issue,
        claimed,
        identifier,
        detail,
        message,
        kind,
        undo,
        remaining,
        started,
        hovered,
        focused,
        module_request,
        pending_slot,
        next_id,
        choose_index,
        choose_best,
        choose_oldest,
        remove_index,
        pending_issues,
        module_pending,
        label_pending,
        page_label_pending,
        mount,
    };
    let captured_handles = serde_json::to_string(&handles.into_surrogate())
        .expect("durable action signal handles serialize");
    let key = format!("{}#durable-actions", super::home_shell::handler_url());
    let arguments = Js::builder()
        .raw("[cx.hydrate(JSON.parse(event.inner.target.dataset.nativeActionHandles)),")
        .surrogate(&account_id.into_surrogate())
        .raw("]")
        .build();
    let attributes = super::handler_asset::mount(cx, &key, arguments);
    let rows = slots
        .into_iter()
        .enumerate()
        .map(|(index, slot)| (index, slot.id, slot.message, slot.kind, slot.undo))
        .collect::<Vec<_>>();
    view! {
        cx =>
        <div
            id="native-deferred-delete-owner"
            data-native-delete-owner=(owner_key)
            data-native-action-account=(account_id.to_string())
            data-native-action-handles=(captured_handles)
            (attributes)
        >
            #[key(index)]
            for (index, id, message, kind, undo) in rows {
                <div
                    class="native-toast"
                    data-native-toast-slot=(index.to_string())
                    :data-native-toast-kind=$(kind.get())
                    :data-native-toast-id=$(id.get())
                    :hidden=$(id.get() == 0)
                    :style=$({
                        let _order = id.get();
                        let style = raw!(
                            "cx.hydrate('order:'+${_order}.toString())",
                            String::new(),
                        );
                        style
                    })
                    :role=$(if kind.get() == "error" { "alert" } else { "status" })
                    :aria-live=$(if kind.get() == "error" {
                        "assertive"
                    } else {
                        "polite"
                    })
                    aria-atomic="true"
                >
                    native_toast_icon(kind: $(kind.get()))
                    <p>$(message.get())</p>
                    <button
                        data-native-toast-undo=""
                        type="button"
                        :hidden=$(!undo.get())
                    >
                        "Undo"
                    </button>
                    <button
                        data-native-toast-close=""
                        type="button"
                        aria-label="Dismiss notification"
                        title="Dismiss"
                    >
                        (super::icons::ui_icon(cx, UiIcon::Close, 13))
                    </button>
                </div>
            }
        </div>
    }
    .boxed()
}

pub(crate) fn handler_factory() -> Js {
    let usize_bits = usize::BITS;
    let maximum_id = usize::MAX;
    let handler = expr!(|_mount: Event,
                         handles: OwnerHandlesSurrogate<'_>,
                         account_id: I64Surrogate| {
        handles.activated.set(true);
        let id = handles.id;
        let issue = handles.issue;
        let claimed = handles.claimed;
        let identifier = handles.identifier;
        let detail = handles.detail;
        let message = handles.message;
        let kind = handles.kind;
        let undo = handles.undo;
        let remaining = handles.remaining;
        let started = handles.started;
        let hovered = handles.hovered;
        let focused = handles.focused;
        let module_request = handles.module_request;
        let pending_slot = handles.pending_slot;
        let next_id = handles.next_id;
        let choose_index = handles.choose_index;
        let choose_best = handles.choose_best;
        let choose_oldest = handles.choose_oldest;
        let remove_index = handles.remove_index;
        let pending_issues = handles.pending_issues;
        let module_pending = handles.module_pending;
        let label_pending = handles.label_pending;
        let page_label_pending = handles.page_label_pending;
        let _mount_path = handles.mount;
        let _module_finish_request = module_request.clone();
        let _module_finish_message = message.clone();
        let _module_finish_kind = kind.clone();
        let _module_finish_undo = undo.clone();
        let _module_finish_remaining = remaining.clone();
        let _module_failure_message = message.clone();
        let _module_failure_kind = kind.clone();
        let _module_failure_remaining = remaining.clone();
        let _label_failure_message = message.clone();
        let _label_failure_kind = kind.clone();
        let _label_failure_remaining = remaining.clone();
        let _page_label_failure_message = message.clone();
        let _page_label_failure_kind = kind.clone();
        let _page_label_failure_remaining = remaining.clone();
        let _error_message = message.clone();
        let _error_kind = kind.clone();
        let _error_remaining = remaining.clone();
        let _success_message = message.clone();
        let _success_kind = kind.clone();
        let _success_remaining = remaining.clone();
        let _allocate_module_request = module_request.clone();
        let _close_module_request = module_request.clone();
        let _undo_module_request = module_request;
        let _remove_remove_index = remove_index;
        let _claim_issue = issue.clone();
        let _claim_claimed = claimed.clone();
        let _claim_identifier = identifier.clone();
        let _claim_detail = detail.clone();
        let _close_id = id.clone();
        let _timer_id = id.clone();
        let _timer_remaining = remaining.clone();
        let _timer_started = started.clone();
        let _allocate_next_id = next_id;
        let _allocate_choose_index = choose_index;
        let _allocate_choose_best = choose_best;
        let _allocate_choose_oldest = choose_oldest;
        let _allocate_id = id.clone();
        let _allocate_claimed = claimed.clone();
        let _allocate_undo = undo.clone();
        let _allocate_hovered = hovered.clone();
        let _allocate_focused = focused.clone();
        let _failure_toast_message = message.clone();
        let _failure_toast_kind = kind.clone();
        let _failure_toast_remaining = remaining.clone();
        let _schedule_issue = issue.clone();
        let _schedule_claimed = claimed.clone();
        let _schedule_identifier = identifier.clone();
        let _schedule_detail = detail.clone();
        let _schedule_message = message.clone();
        let _schedule_kind = kind.clone();
        let _schedule_undo = undo.clone();
        let _schedule_remaining = remaining.clone();
        let _undo_id = id.clone();
        let _undo_issue = issue;
        let _undo_claimed = claimed;
        let _undo_identifier = identifier;
        let _undo_detail = detail;
        let _undo_message = message;
        let _undo_kind = kind;
        let _undo_undo = undo;
        let _undo_remaining = remaining.clone();
        let _pause_id = id.clone();
        let _pause_remaining = remaining.clone();
        let _pause_started = started.clone();
        let _pause_hovered = hovered.clone();
        let _pause_focused = focused.clone();
        let _resume_id = id.clone();
        let _resume_hovered = hovered.clone();
        let _resume_focused = focused.clone();
        let _elapsed_id = id.clone();
        let _elapsed_remaining = remaining;
        let _elapsed_started = started;
        let _elapsed_hovered = hovered.clone();
        let _elapsed_focused = focused.clone();
        let _rearm_id = id;
        let _rearm_hovered = hovered;
        let _rearm_focused = focused;
        let _clear = |_index: UsizeSurrogate| {
            raw!(
                "clearTimeout(document.querySelector('[data-native-toast-slot=\"'+${_index}.toString()+'\"]')?.nativeTimer);",
                ()
            );
        };
        let _navigate = |_destination: StringSurrogate| {
            raw!(
                "void cx.navigate(${_mount_path}.toString()+${_destination}.toString());",
                ()
            );
        };
        let _remove = |target: I64Surrogate| {
            _remove_remove_index.set(0);
            while _remove_remove_index.get() < pending_issues.get().len() {
                if *pending_issues.get().index(_remove_remove_index.get()) == target {
                    pending_issues.remove(_remove_remove_index.get());
                    _remove_remove_index.set(pending_issues.get().len());
                } else {
                    _remove_remove_index.increment();
                }; // Topcoat loop bodies need statement terminators to avoid function returns.
            }
            _remove_remove_index.set(0);
        };
        let _claim = |index: UsizeSurrogate, keepalive: BoolSurrogate| {
            if index < 4 {
                if !_claim_claimed.index(index).get() {
                    _claim_claimed.index(index).set(true);
                    if pending_slot.get() == index {
                        pending_slot.set(4);
                    }
                    let target = _claim_issue.index(index).get();
                    let _label = _claim_identifier.index(index).get();
                    let _restore = _claim_detail.index(index).get();
                    let _success = || {
                        raw!(
                            "if (nativeOwnerToken.active) nativeOwnerToken.host.nativeRemove(${target});",
                            ()
                        );
                    };
                    let _failure = || {
                        raw!(
                            "if (nativeOwnerToken.active) { nativeOwnerToken.host.nativeRemove(${target}); nativeOwnerToken.host.nativeFailure(${_label}, ${_restore}); };",
                            ()
                        );
                    };
                    if keepalive {
                        let _keepalive = commit_delete.with_keepalive();
                        let _future = _keepalive(account_id, target);
                        raw!("${_future}.then(${_success}, ${_failure});", ());
                    } else {
                        let _run = async || {
                            commit_delete(account_id, target).await;
                            raw!("${_success}();", ());
                        };
                        raw!("${_run}().catch(${_failure});", ());
                    }
                }
            }
        };
        let _close = |index: UsizeSurrogate, expected: UsizeSurrogate| {
            if _close_id.index(index).get() == expected {
                if expected != 0 {
                    raw!("${_clear}(${index});", ());
                    raw!("${_claim}(${index},cx.hydrate(false));", ());
                    _close_id.index(index).set(0);
                    _close_module_request.index(index).set(None);
                }
            }
        };
        let _timer = |index: UsizeSurrogate| {
            raw!("${_clear}(${index});", ());
            let _expected = _timer_id.index(index).get();
            let _delay = _timer_remaining.index(index).get();
            _timer_started
                .index(index)
                .set(raw!("cx.hydrate(performance.now())", 0.0_f64));
            raw!(
                "document.querySelector('[data-native-toast-slot=\"'+${index}.toString()+'\"]').nativeTimer=setTimeout(()=>${_close}(${index},${_expected}),Math.max(0,Number(${_delay}.toString())));",
                ()
            );
        };
        let _allocate = || {
            _allocate_choose_index.set(0);
            _allocate_choose_best.set(0);
            _allocate_choose_oldest.set(maximum_id);
            while _allocate_choose_index.get() < 4 {
                let candidate = _allocate_id.index(_allocate_choose_index.get()).get();
                if candidate == 0 {
                    _allocate_choose_best.set(_allocate_choose_index.get());
                    _allocate_choose_index.set(4);
                } else {
                    if candidate < _allocate_choose_oldest.get() {
                        _allocate_choose_oldest.set(candidate);
                        _allocate_choose_best.set(_allocate_choose_index.get());
                    }
                    _allocate_choose_index.increment();
                }; // Topcoat loop bodies need statement terminators to avoid function returns.
            }
            let index = _allocate_choose_best.get();
            let _previous = _allocate_id.index(index).get();
            if _previous != 0 {
                // Retire focus before reusing a presentation slot, as original keyed toasts do.
                raw!(
                    "const slot=document.querySelector('[data-native-toast-slot=\"'+${index}.toString()+'\"]'); if(slot?.contains(document.activeElement))document.activeElement.blur();",
                    ()
                );
            }
            raw!("${_close}(${index},${_previous});", ());
            _allocate_next_id.increment();
            _allocate_id.index(index).set(_allocate_next_id.get());
            _allocate_claimed.index(index).set(true);
            _allocate_undo.index(index).set(false);
            _allocate_hovered.index(index).set(false);
            _allocate_focused.index(index).set(false);
            _allocate_module_request.index(index).set(None);
            index
        };
        let _module_release = |target: I64Surrogate| {
            let position = module_pending.get().position(target);
            if position.is_some() {
                module_pending.remove(position.unwrap());
            }
        };
        let _module_finish = |request: ModuleRequestSurrogate,
                              _reply: ModuleAssignmentReplySurrogate,
                              restoring: BoolSurrogate| {
            if !restoring {
                raw!("${_module_release}(${request}.issue_id);", ());
            }
            let index = raw!("${_allocate}()", 0_usize);
            if restoring {
                _module_finish_message
                    .index(index)
                    .set("Restored ".to_owned());
                _module_finish_message
                    .index(index)
                    .push_str(request.identifier.clone());
                _module_finish_remaining.index(index).set(3_000.0_f64);
            } else {
                _module_finish_message
                    .index(index)
                    .set(request.identifier.clone());
                _module_finish_message.index(index).push_str(" → ");
                let label = if _reply.module_id.is_none() {
                    "No module".to_owned()
                } else {
                    _reply.module_label.clone()
                };
                _module_finish_message.index(index).push_str(label);
                _module_finish_remaining.index(index).set(5_000.0_f64);
                _module_finish_request.index(index).set(Some(request));
                _module_finish_undo.index(index).set(true);
            }
            _module_finish_kind.index(index).set(if restoring {
                "info".to_owned()
            } else {
                "success".to_owned()
            });
            raw!(
                "window.dispatchEvent(new CustomEvent('lific:native-issue-module-applied',{detail:${_reply}})); ${_timer}(${index});",
                ()
            );
        };
        let _module_failure = |request: ModuleRequestSurrogate,
                               restoring: BoolSurrogate,
                               message: StringSurrogate| {
            if !restoring {
                raw!("${_module_release}(${request}.issue_id);", ());
            }
            let index = raw!("${_allocate}()", 0_usize);
            _module_failure_message.index(index).set(if restoring {
                "Couldn't undo ".to_owned()
            } else {
                "Couldn't update ".to_owned()
            });
            _module_failure_message
                .index(index)
                .push_str(request.identifier.clone());
            _module_failure_message.index(index).push_str(": ");
            _module_failure_message.index(index).push_str(message);
            _module_failure_kind.index(index).set("error".to_owned());
            _module_failure_remaining.index(index).set(8_000.0_f64);
            raw!("${_timer}(${index});", ());
        };
        let _module_run = |request: ModuleRequestSurrogate, _restoring: BoolSurrogate| {
            let _success = |_reply: ModuleAssignmentReplySurrogate| {
                if _reply.status.is_ok() {
                    raw!(
                        "if(nativeOwnerToken.active) nativeOwnerToken.host.nativeModuleFinish(${request},${_reply},${_restoring});",
                        ()
                    );
                } else {
                    let _message = _reply.status.clone().unwrap_err();
                    raw!(
                        "if(nativeOwnerToken.active) nativeOwnerToken.host.nativeModuleFailure(${request},${_restoring},${_message});",
                        ()
                    );
                }
            };
            let _keepalive = assign_module.with_keepalive();
            let _future = _keepalive(request.clone());
            raw!(
                "${_future}.then(${_success},()=>{if(nativeOwnerToken.active)nativeOwnerToken.host.nativeModuleFailure(${request},${_restoring},cx.hydrate(\"Couldn't reach the server. Check your connection and try again.\"));});",
                ()
            );
        };
        let _module_accept = |request: ModuleRequestSurrogate| {
            if request.account_id != account_id {
                false
            } else if request.issue_id <= 0_i64 {
                false
            } else {
                let pending = module_pending.get().position(request.issue_id.clone());
                if pending.is_some() {
                    false
                } else {
                    module_pending.push(request.issue_id.clone());
                    raw!("${_module_run}(${request},cx.hydrate(false));", ());
                    true
                }
            }
        };
        let _label_release = |target: I64Surrogate| {
            let position = label_pending.get().position(target);
            if position.is_some() {
                label_pending.remove(position.unwrap());
            }
        };
        let _label_finish = |request: LabelRequestValue, reply: LabelReplyValue| {
            raw!("${_label_release}(${request}.issue_id);", ());
            // Catalog creation can succeed before attachment fails. Forward both
            // outcomes so the live picker can refresh its catalog independently.
            raw!(
                "window.dispatchEvent(new CustomEvent('lific:native-issue-label-applied',{detail:${reply}}));",
                ()
            );
            if reply.status.is_err() {
                let index = raw!("${_allocate}()", 0_usize);
                let create_failed = if request.mode == "create".to_owned() {
                    reply.catalog_item.is_none()
                } else {
                    false
                };
                if create_failed {
                    _label_failure_message
                        .index(index)
                        .set("Couldn't create label: ".to_owned());
                } else {
                    _label_failure_message
                        .index(index)
                        .set("Couldn't save ".to_owned());
                    _label_failure_message
                        .index(index)
                        .push_str(request.identifier);
                    _label_failure_message.index(index).push_str(": ");
                }
                _label_failure_message
                    .index(index)
                    .push_str(reply.status.unwrap_err());
                _label_failure_kind.index(index).set("error".to_owned());
                _label_failure_remaining.index(index).set(8_000.0_f64);
                raw!("${_timer}(${index});", ());
            }
        };
        let _label_network_failure = |request: LabelRequestValue| {
            let _reply = LabelReply {
                status: Err(
                    "Couldn't reach the server. Check your connection and try again.".to_owned(),
                ),
                account_id: request.account_id.clone(),
                issue_id: request.issue_id.clone(),
                seq: 0_i64,
                labels: raw!("cx.hydrate([])", Vec::<String>::new()),
                canonical: None,
                catalog_item: None,
            };
            raw!("${_label_finish}(${request},${_reply});", ());
        };
        let _label_run = |request: LabelRequestValue| {
            let _success = |_reply: LabelReplyValue| {
                raw!(
                    "if(nativeOwnerToken.active)nativeOwnerToken.host.nativeLabelFinish(${request},${_reply});",
                    ()
                );
            };
            let _failure = || {
                raw!(
                    "if(nativeOwnerToken.active)nativeOwnerToken.host.nativeLabelNetworkFailure(${request});",
                    ()
                );
            };
            if request.mode == "create".to_owned() {
                let _keepalive = create_label.with_keepalive();
                let _future = _keepalive(request.clone());
                raw!("${_future}.then(${_success},${_failure});", ());
            } else {
                let _keepalive = update_labels.with_keepalive();
                let _future = _keepalive(request.clone());
                raw!("${_future}.then(${_success},${_failure});", ());
            }
        };
        let _label_accept = |request: LabelRequestValue| {
            let mode_valid = if request.mode == "create".to_owned() {
                true
            } else if request.mode == "attach".to_owned() {
                true
            } else {
                request.mode == "remove".to_owned()
            };
            if request.account_id != account_id {
                false
            } else if request.issue_id <= 0_i64 {
                false
            } else if !mode_valid {
                false
            } else if label_pending
                .get()
                .position(request.issue_id.clone())
                .is_some()
            {
                false
            } else {
                label_pending.push(request.issue_id.clone());
                raw!("${_label_run}(${request});", ());
                true
            }
        };
        let _page_label_release = |target: I64Surrogate| {
            let position = page_label_pending.get().position(target);
            if position.is_some() {
                page_label_pending.remove(position.unwrap());
            }
        };
        let _page_label_finish = |request: PageLabelRequestValue, reply: PageLabelReplyValue| {
            raw!("${_page_label_release}(${request}.page_id);", ());
            raw!(
                "window.dispatchEvent(new CustomEvent('lific:native-page-label-applied',{detail:${reply}}));",
                ()
            );
            if reply.status.is_err() {
                let index = raw!("${_allocate}()", 0_usize);
                _page_label_failure_message
                    .index(index)
                    .set("Couldn't save ".to_owned());
                _page_label_failure_message
                    .index(index)
                    .push_str(request.identifier);
                _page_label_failure_message.index(index).push_str(": ");
                _page_label_failure_message
                    .index(index)
                    .push_str(reply.status.unwrap_err());
                _page_label_failure_kind
                    .index(index)
                    .set("error".to_owned());
                _page_label_failure_remaining.index(index).set(8_000.0_f64);
                raw!("${_timer}(${index});", ());
            }
        };
        let _page_label_network_failure = |request: PageLabelRequestValue| {
            let _reply = PageLabelReply {
                status: Err(
                    "Couldn't reach the server. Check your connection and try again.".to_owned(),
                ),
                account_id: request.account_id.clone(),
                page_id: request.page_id.clone(),
                canonical: None,
            };
            raw!("${_page_label_finish}(${request},${_reply});", ());
        };
        let _page_label_run = |request: PageLabelRequestValue| {
            let _success = |_reply: PageLabelReplyValue| {
                raw!(
                    "if(nativeOwnerToken.active)nativeOwnerToken.host.nativePageLabelFinish(${request},${_reply});",
                    ()
                );
            };
            let _failure = || {
                raw!(
                    "if(nativeOwnerToken.active)nativeOwnerToken.host.nativePageLabelNetworkFailure(${request});",
                    ()
                );
            };
            let _keepalive = update_page_labels.with_keepalive();
            let _future = _keepalive(request.clone());
            raw!("${_future}.then(${_success},${_failure});", ());
        };
        let _page_label_accept = |request: PageLabelRequestValue| {
            if request.account_id != account_id {
                false
            } else if request.page_id <= 0_i64 {
                false
            } else if page_label_pending
                .get()
                .position(request.page_id.clone())
                .is_some()
            {
                false
            } else {
                page_label_pending.push(request.page_id.clone());
                raw!("${_page_label_run}(${request});", ());
                true
            }
        };
        let _error_accept = |request: ToastErrorRequestSurrogate| {
            if request.account_id != account_id {
                false
            } else {
                let index = raw!("${_allocate}()", 0_usize);
                _error_message.index(index).set(request.message);
                _error_kind.index(index).set("error".to_owned());
                _error_remaining.index(index).set(8_000.0_f64);
                raw!("${_timer}(${index});", ());
                true
            }
        };
        let _success_accept = |request: ToastRequestSurrogate| {
            if request.account_id != account_id {
                false
            } else {
                let index = raw!("${_allocate}()", 0_usize);
                _success_message.index(index).set(request.message);
                _success_kind.index(index).set("success".to_owned());
                _success_remaining.index(index).set(5_000.0_f64);
                raw!("${_timer}(${index});", ());
                true
            }
        };
        let _failure_toast = |label: StringSurrogate, _restore: StringSurrogate| {
            let index = raw!("${_allocate}()", 0_usize);
            _failure_toast_message
                .index(index)
                .set("Couldn't delete ".to_owned());
            _failure_toast_message.index(index).push_str(label.clone());
            _failure_toast_message.index(index).push_str(" — restored");
            _failure_toast_kind.index(index).set("error".to_owned());
            _failure_toast_remaining.index(index).set(8_000.0_f64);
            raw!("${_timer}(${index});", ());
            raw!("${_navigate}(${_restore});", ());
        };
        let _schedule = |requested_account: I64Surrogate,
                         target: I64Surrogate,
                         label: StringSurrogate,
                         _back: StringSurrogate,
                         restore: StringSurrogate| {
            if requested_account != account_id {
                false
            } else if target <= 0_i64 {
                false
            } else {
                let _previous = pending_slot.get();
                raw!("${_claim}(${_previous},cx.hydrate(false));", ());
                let index = raw!("${_allocate}()", 0_usize);
                _schedule_issue.index(index).set(target);
                _schedule_identifier.index(index).set(label.clone());
                _schedule_detail.index(index).set(restore);
                _schedule_claimed.index(index).set(false);
                _schedule_undo.index(index).set(true);
                pending_slot.set(index);
                pending_issues.push(target);
                _schedule_message.index(index).set("Deleted ".to_owned());
                _schedule_message.index(index).push_str(label.clone());
                _schedule_kind.index(index).set("info".to_owned());
                _schedule_remaining.index(index).set(5_000.0_f64);
                raw!("${_navigate}(${_back}); ${_timer}(${index});", ());
                true
            }
        };
        let _undo = |index: UsizeSurrogate| {
            let _expected = _undo_id.index(index).get();
            if _expected != 0 {
                let module_request = _undo_module_request.index(index).get();
                if module_request.is_some() {
                    let request = module_request.unwrap();
                    _undo_module_request.index(index).set(None);
                    raw!("${_close}(${index},${_expected});", ());
                    let _inverse = ModuleRequest {
                        account_id: request.account_id.clone(),
                        issue_id: request.issue_id.clone(),
                        identifier: request.identifier.clone(),
                        previous_module_id: request.next_module_id.clone(),
                        next_module_id: request.previous_module_id.clone(),
                    };
                    raw!("${_module_run}(${_inverse},cx.hydrate(true));", ());
                } else {
                    let was_claimed = _undo_claimed.index(index).get();
                    // Original actions dismiss their toast even when replacement already committed it.
                    // Claim locally before dismissing so a genuine Undo cannot start transport.
                    _undo_claimed.index(index).set(true);
                    raw!("${_close}(${index},${_expected});", ());
                    if !was_claimed {
                        _undo_undo.index(index).set(false);
                        if pending_slot.get() == index {
                            pending_slot.set(4);
                        }
                        let _target = _undo_issue.index(index).get();
                        let _restore = _undo_detail.index(index).get();
                        let label = _undo_identifier.index(index).get();
                        raw!("${_remove}(${_target}); ${_navigate}(${_restore});", ());
                        let restored = raw!("${_allocate}()", 0_usize);
                        _undo_message.index(restored).set("Restored ".to_owned());
                        _undo_message.index(restored).push_str(label.clone());
                        _undo_kind.index(restored).set("info".to_owned());
                        _undo_remaining.index(restored).set(3_000.0_f64);
                        raw!("${_timer}(${restored});", ());
                    }
                }
            }
        };
        let _pause = |index: UsizeSurrogate, mouse: BoolSurrogate| {
            if !_pause_hovered.index(index).get() {
                if !_pause_focused.index(index).get() {
                    if _pause_id.index(index).get() != 0 {
                        let now = raw!("cx.hydrate(performance.now())", 0.0_f64);
                        let next = _pause_remaining.index(index).get()
                            - (now - _pause_started.index(index).get());
                        _pause_remaining.index(index).set(if next > 0.0_f64 {
                            next
                        } else {
                            0.0_f64
                        });
                        raw!("${_clear}(${index});", ());
                    }
                }
            }
            if mouse {
                _pause_hovered.index(index).set(true);
            } else {
                _pause_focused.index(index).set(true);
            }
        };
        let _resume = |index: UsizeSurrogate, mouse: BoolSurrogate| {
            if mouse {
                _resume_hovered.index(index).set(false);
            } else {
                _resume_focused.index(index).set(false);
            }
            if !_resume_hovered.index(index).get() {
                if !_resume_focused.index(index).get() {
                    if _resume_id.index(index).get() != 0 {
                        raw!("${_timer}(${index});", ());
                    }
                }
            }
        };
        let _pagehide = || {
            let _index = pending_slot.get();
            raw!("${_claim}(${_index},cx.hydrate(true));", ());
        };
        // Native page commits transfer the signals, but each new scope owns
        // fresh timers. Account for running time before rearming on that scope.
        let _elapsed = |index: UsizeSurrogate| {
            if _elapsed_id.index(index).get() != 0 {
                if !_elapsed_hovered.index(index).get() {
                    if !_elapsed_focused.index(index).get() {
                        let now = raw!("cx.hydrate(performance.now())", 0.0_f64);
                        let next = _elapsed_remaining.index(index).get()
                            - (now - _elapsed_started.index(index).get());
                        _elapsed_remaining.index(index).set(if next > 0.0_f64 {
                            next
                        } else {
                            0.0_f64
                        });
                        _elapsed_started.index(index).set(now);
                    }
                }
            }
        };
        let _rearm = |index: UsizeSurrogate, mouse: BoolSurrogate, focus: BoolSurrogate| {
            raw!("${_elapsed}(${index});", ());
            _rearm_hovered.index(index).set(mouse.clone());
            _rearm_focused.index(index).set(focus.clone());
            if _rearm_id.index(index).get() != 0 {
                if !mouse {
                    if !focus {
                        raw!("${_timer}(${index});", ());
                    }
                }
            }
        };
        raw!(
            r#"
            const owner = document.getElementById('native-deferred-delete-owner');
            const transferred = document.nativeDeferredDeleteTransfer;
            delete document.nativeDeferredDeleteTransfer;
            const nativeOwnerToken = transferred?.active && transferred.key === owner.dataset.nativeActionAccount
                ? transferred : {active:true,key:owner.dataset.nativeActionAccount,host:owner};
            nativeOwnerToken.host = owner;
            let transferring = false;
            owner.nativeRemove = ${_remove};
            owner.nativeFailure = ${_failure_toast};
            owner.nativeModuleFinish = ${_module_finish};
            owner.nativeModuleFailure = ${_module_failure};
            owner.nativeLabelFinish = ${_label_finish};
            owner.nativeLabelNetworkFailure = ${_label_network_failure};
            owner.nativePageLabelFinish = ${_page_label_finish};
            owner.nativePageLabelNetworkFailure = ${_page_label_network_failure};
            window.addEventListener('lific:native-toast-error',event=>{
                if (${_error_accept}(event.detail).toString()==='true') event.preventDefault();
            },{signal:cx.abortSignal});
            window.addEventListener('lific:native-toast-success',event=>{
                if (${_success_accept}(event.detail).toString()==='true') event.preventDefault();
            },{signal:cx.abortSignal});
            window.addEventListener('lific:native-issue-label-request',event=>{
                if (${_label_accept}(event.detail).toString()==='true') event.preventDefault();
            },{signal:cx.abortSignal});
            window.addEventListener('lific:native-page-label-request',event=>{
                if (${_page_label_accept}(event.detail).toString()==='true') event.preventDefault();
            },{signal:cx.abortSignal});
            window.addEventListener('lific:native-issue-module-request',event=>{
                if (${_module_accept}(event.detail).toString()==='true') event.preventDefault();
            },{signal:cx.abortSignal});
            window.addEventListener('lific:native-issue-delete-request', event => {
                const v=event.detail;
                const accepted=${_schedule}(cx.hydrate({t:'i64',bits:64,v:String(v.account_id)}),cx.hydrate({t:'i64',bits:64,v:String(v.issue_id)}),cx.hydrate(v.identifier),cx.hydrate(v.list_path),cx.hydrate(v.detail_path));
                if(accepted.toString()==='true')event.preventDefault();
            },{signal:cx.abortSignal});
            window.addEventListener('pagehide',${_pagehide},{signal:cx.abortSignal});
            document.addEventListener('topcoat:before-page-replace',event=>{
                const next = event.detail.nextDocument.getElementById('native-deferred-delete-owner');
                transferring = next?.getAttribute('data-native-action-account') === owner.dataset.nativeActionAccount;
                if(next?.getAttribute('data-native-delete-owner') !== owner.dataset.nativeDeleteOwner) ${_pagehide}();
                if(transferring) {
                    document.nativeDeferredDeleteTransfer = nativeOwnerToken;
                    for(const toast of owner.querySelectorAll('[data-native-toast-slot]')) {
                        ${_elapsed}(cx.hydrate({t:'usize',bits:Number(${usize_bits}.toString()),v:toast.dataset.nativeToastSlot}));
                    }
                } else {
                    ${_pagehide}();
                }
            },{signal:cx.abortSignal});
            for(const toast of owner.querySelectorAll('[data-native-toast-slot]')) {
                const i=cx.hydrate({t:'usize',bits:Number(${usize_bits}.toString()),v:toast.dataset.nativeToastSlot});
                toast.addEventListener('mouseenter',()=>${_pause}(i,cx.hydrate(true)),{signal:cx.abortSignal});
                toast.addEventListener('mouseleave',()=>${_resume}(i,cx.hydrate(true)),{signal:cx.abortSignal});
                toast.addEventListener('focusin',()=>${_pause}(i,cx.hydrate(false)),{signal:cx.abortSignal});
                toast.addEventListener('focusout',e=>{if(!toast.contains(e.relatedTarget))${_resume}(i,cx.hydrate(false));},{signal:cx.abortSignal});
                toast.querySelector('[data-native-toast-undo]').addEventListener('click',()=>${_undo}(i),{signal:cx.abortSignal});
                toast.querySelector('[data-native-toast-close]').addEventListener('click',()=>${_close}(i, cx.hydrate({t:'usize',bits:Number(${usize_bits}.toString()),v:toast.dataset.nativeToastId})),{signal:cx.abortSignal});
                clearTimeout(toast.nativeTimer);
                ${_rearm}(i,cx.hydrate(toast.matches(':hover')),cx.hydrate(toast.contains(document.activeElement)));
            }
            cx.abortSignal.addEventListener('abort',()=>{
                for(const toast of owner.querySelectorAll('[data-native-toast-slot]'))clearTimeout(toast.nativeTimer);
                if(!transferring) {
                    nativeOwnerToken.active=false;
                    delete owner.nativeRemove;
                    delete owner.nativeFailure;
                    delete owner.nativeModuleFinish;
                    delete owner.nativeModuleFailure;
                    delete owner.nativeLabelFinish;
                    delete owner.nativeLabelNetworkFailure;
                    delete owner.nativePageLabelFinish;
                    delete owner.nativePageLabelNetworkFailure;
                }
            },{once:true});
        "#,
            ()
        );
    });
    handler.into_evaluated_and_js().1
}

#[shard("/__native_workspace/toast_icon")]
async fn native_toast_icon(cx: &Cx, kind: String) -> topcoat::Result<impl topcoat::view::View> {
    let icon = match kind.as_str() {
        "error" => UiIcon::Error,
        "success" => UiIcon::Success,
        _ => UiIcon::Info,
    };
    Ok(super::icons::ui_icon(cx, icon, 16))
}

/// Mount the production owner factory and return its actual client signal values.
#[cfg(test)]
pub(crate) fn activated_snapshot(html: &str) -> serde_json::Map<String, serde_json::Value> {
    super::home_fixture::evaluate_handler(
        "src/topcoat/native/deferred_delete.test.cjs",
        &serde_json::json!({
            "html": html,
            "handler_source": super::shell_handlers::handler_source(),
            "handler_url": super::shell_handlers::handler_url(),
            "probe_only": true,
        }),
    )
    .as_object()
    .expect("mounted owner returns a signal snapshot")
    .clone()
}

#[cfg(test)]
mod tests {
    use super::super::issue_edit::labels::{LabelCatalogItem, LabelRequest};
    use super::*;
    use std::{io::Write, process::Stdio, sync::Arc};
    use topcoat::{context::CxTestBuilder, router::RemoteAddr, runtime::Surrogated};

    #[topcoat::view::component]
    async fn owner_fixture(cx: &Cx) -> topcoat::Result<impl topcoat::view::View> {
        let owner_cx = cx.keyed((7_i64, "ACC"));
        let pending = signal(&owner_cx, Vec::<i64>::new);
        Ok(view! { cx => (owner(cx, &owner_cx, 7, "ACC", false, pending)) })
    }

    async fn markup(mount: &str) -> String {
        let (mut parts, ()) = axum::http::Request::builder()
            .header("x-forwarded-prefix", mount)
            .body(())
            .unwrap()
            .into_parts();
        parts
            .extensions
            .insert(RemoteAddr("127.0.0.1:4000".parse().unwrap()));
        let proxies: Arc<[crate::ratelimit::IpNetwork]> =
            vec![crate::ratelimit::IpNetwork::parse("127.0.0.0/8").unwrap()].into();
        let cx = CxTestBuilder::new()
            .request_context(parts)
            .app_context(proxies)
            .build();
        view! { cx => owner_fixture() }
            .single()
            .await
            .unwrap()
            .render(&cx)
    }

    #[tokio::test]
    async fn native_delete_owner_transfers_undo_deadlines_and_inflight_results() {
        for mount in ["", "/app", "/ACC"] {
            let html = markup(mount).await;
            let mut child = std::process::Command::new("node")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/src/topcoat/native/deferred_delete.test.cjs"
                ))
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(
                    serde_json::json!({
                        "html":html,"mount":mount,
                        "handler_source": super::super::shell_handlers::handler_source(),
                        "handler_url": super::super::shell_handlers::handler_url(),
                        "toast_errors": ([7_i64, 8].into_iter().map(|account_id| (
                            account_id.to_string(),
                            ToastErrorRequest {
                                account_id,
                                message:"Couldn't copy to clipboard".into(),
                            }.into_surrogate(),
                        )).collect::<std::collections::BTreeMap<_,_>>()),
                        "toast_successes": ([
                            (7_i64, "File deleted."),
                            (7_i64, "File deleted, along with 1 reference."),
                            (7_i64, "File deleted, along with 4 references."),
                            (8_i64, "foreign account"),
                        ].into_iter().enumerate().map(|(index, (account_id, message))| (
                            index.to_string(), ToastErrorRequest { account_id, message:message.into() }.into_surrogate()
                        )).collect::<std::collections::BTreeMap<_,_>>()),
                        "page_label_requests": (["attach", "remove", "wrong_account", "invalid_page"].into_iter().map(|mode| (
                            mode,
                            super::super::pages::labels_action::Request {
                                account_id: if mode == "wrong_account" { 8 } else { 7 },
                                page_id: if mode == "invalid_page" { 0 } else { 42 },
                                identifier: "ACC-P42".into(), label: "bug".into(), attach: mode != "remove",
                            }.into_surrogate(),
                        )).collect::<std::collections::BTreeMap<_,_>>()),
                        "page_label_replies": ([true, false].into_iter().map(|success| (
                            if success { "saved" } else { "failed" },
                            super::super::pages::labels_action::Reply {
                                status: if success { Ok("saved".into()) } else { Err("Forbidden: insufficient project role".into()) },
                                account_id: 7, page_id: 42,
                                canonical: success.then(|| super::super::pages::labels_action::Snapshot {
                                    identifier: "ACC-P42".into(), title: "Latest title".into(), content: "Latest body".into(), seq: 13,
                                    page_status: "published".into(), pinned: true, labels: vec!["bug".into()],
                                }),
                            }.into_surrogate(),
                        )).collect::<std::collections::BTreeMap<_,_>>()),
                        "module_requests": ([42_i64, 43].into_iter().flat_map(|issue_id| {
                            [None, Some(9_i64)].into_iter().flat_map(move |next| {
                                [None, Some(9_i64)].into_iter().map(move |previous| {
                                    let key = format!("{issue_id}:{}:{}", next.map_or_else(|| "null".into(), |v|v.to_string()), previous.map_or_else(|| "null".into(), |v|v.to_string()));
                                    (key, ModuleRequest {
                                        account_id:7, issue_id, identifier:format!("ACC-{issue_id}"),
                                        previous_module_id:previous,next_module_id:next,
                                    }.into_surrogate())
                                })
                            })
                        }).collect::<std::collections::BTreeMap<_,_>>()),
                        "module_replies": ([42_i64,43].into_iter().flat_map(|issue_id| {
                            [None,Some(9_i64)].into_iter().map(move |module_id| {
                                (format!("{issue_id}:{}",module_id.map_or_else(|| "null".into(),|v|v.to_string())), super::super::issue_edit::module_assignment::ModuleAssignmentReply {
                                    status:Ok("saved".into()),
                                    canonical:None,
                                    account_id:7,issue_id,seq:12,module_id,
                                    module_label:module_id.map_or_else(||"None".into(),|_|"Release".into()),
                                }.into_surrogate())
                            })
                        }).collect::<std::collections::BTreeMap<_,_>>()),
                        "module_failure": ModuleAssignmentReply {
                            status:Err("Forbidden: insufficient project role".into()),
                            canonical:None,account_id:7,issue_id:42,seq:0,
                            module_id:None,module_label:"None".into(),
                        }.into_surrogate(),
                        "label_requests": (["attach", "remove", "create", "unknown", "wrong_account"].into_iter().map(|mode| (
                            mode,
                            LabelRequest {
                                mode: if mode == "wrong_account" { "attach" } else { mode }.into(),
                                account_id: if mode == "wrong_account" { 8 } else { 7 },
                                issue_id:42,identifier:"ACC-42".into(),
                                name:if mode == "create" { "new label" } else { "bug" }.into(),color:"#2563EB".into(),
                            }.into_surrogate(),
                        )).collect::<std::collections::BTreeMap<_,_>>()),
                        "label_replies": (["saved", "create_failed", "attach_failed"].into_iter().map(|outcome| (
                            outcome,
                            LabelReply {
                                status: if outcome == "saved" { Ok("saved".into()) } else { Err("Forbidden: insufficient project role".into()) },
                                account_id:7,issue_id:42,seq:if outcome == "saved" { 13 } else { 0 },
                                labels:vec!["bug".into()],canonical:None,
                                catalog_item: (outcome == "attach_failed").then(|| LabelCatalogItem {name:"new label".into(),color:"#2563EB".into()}),
                            }.into_surrogate(),
                        )).collect::<std::collections::BTreeMap<_,_>>()),
                    })
                        .to_string()
                        .as_bytes(),
                )
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "generated deferred owner at {mount}:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
