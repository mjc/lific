//! Four-slot presentation storage for deferred_delete.rs.
//! Rust expression callbacks must share this storage; no per-slot controller.
use super::super::runtime::{procedure::ProcedureKeepaliveExt, signal_vec::SignalVecExt};
use super::icons::UiIcon;
use super::{issue_edit::delete::commit_delete, transport};
use topcoat::{
    context::Cx,
    runtime::{
        BoolSurrogate, Event, I64Surrogate, Signal, StringSurrogate, UsizeSurrogate, expr, shard,
        signal,
    },
    view::{Attributes, BoxView, ViewExt, view},
};

pub(crate) const TOAST_CAPACITY: usize = 4;

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
        }
    }
}

pub(crate) fn slots(cx: &Cx) -> [ToastSlot; TOAST_CAPACITY] {
    std::array::from_fn(|index| ToastSlot::new(&cx.keyed(index)))
}

pub(crate) fn owner<'a>(
    cx: &'a Cx,
    account_id: i64,
    project: &str,
    pending_issues: Signal<Vec<i64>>,
) -> BoxView<'a> {
    let owner_key = format!("{account_id}:{project}");
    let slots = slots(cx);
    let pending_slot = signal(cx, || 4_usize);
    let next_id = signal(cx, || 0_usize);
    let choose_index = signal(cx, || 0_usize);
    let choose_best = signal(cx, || 0_usize);
    let choose_oldest = signal(cx, || usize::MAX);
    let remove_index = signal(cx, || 0_usize);
    let usize_bits = usize::BITS;
    let maximum_id = usize::MAX;
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
    let _remove_remove_index = remove_index;
    let _remove_pending_issues = pending_issues.clone();
    let _claim_pending_slot = pending_slot.clone();
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
    let _schedule_pending_slot = pending_slot.clone();
    let _schedule_pending_issues = pending_issues;
    let _schedule_issue = issue.clone();
    let _schedule_claimed = claimed.clone();
    let _schedule_identifier = identifier.clone();
    let _schedule_detail = detail.clone();
    let _schedule_message = message.clone();
    let _schedule_kind = kind.clone();
    let _schedule_undo = undo.clone();
    let _schedule_remaining = remaining.clone();
    let _undo_pending_slot = pending_slot.clone();
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
    let _pagehide_pending_slot = pending_slot;
    let handler = expr!(|_mount: Event| {
        let _clear = |_index: UsizeSurrogate| {
            raw!(
                "clearTimeout(document.querySelector('[data-native-toast-slot=\"'+${_index}.toString()+'\"]')?.nativeTimer);",
                ()
            );
        };
        let _navigate = |_destination: StringSurrogate| {
            raw!(
                "void cx.navigate(${mount}.toString()+${_destination}.toString());",
                ()
            );
        };
        let _remove = |target: I64Surrogate| {
            _remove_remove_index.set(0);
            while _remove_remove_index.get() < _remove_pending_issues.get().len() {
                if *_remove_pending_issues
                    .get()
                    .index(_remove_remove_index.get())
                    == target
                {
                    _remove_pending_issues.remove(_remove_remove_index.get());
                    _remove_remove_index.set(_remove_pending_issues.get().len());
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
                    if _claim_pending_slot.get() == index {
                        _claim_pending_slot.set(4);
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
            index
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
                let _previous = _schedule_pending_slot.get();
                raw!("${_claim}(${_previous},cx.hydrate(false));", ());
                let index = raw!("${_allocate}()", 0_usize);
                _schedule_issue.index(index).set(target);
                _schedule_identifier.index(index).set(label.clone());
                _schedule_detail.index(index).set(restore);
                _schedule_claimed.index(index).set(false);
                _schedule_undo.index(index).set(true);
                _schedule_pending_slot.set(index);
                _schedule_pending_issues.push(target);
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
                let was_claimed = _undo_claimed.index(index).get();
                // Original actions dismiss their toast even when replacement already committed it.
                // Claim locally before dismissing so a genuine Undo cannot start transport.
                _undo_claimed.index(index).set(true);
                raw!("${_close}(${index},${_expected});", ());
                if !was_claimed {
                    _undo_undo.index(index).set(false);
                    if _undo_pending_slot.get() == index {
                        _undo_pending_slot.set(4);
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
            let _index = _pagehide_pending_slot.get();
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
            const nativeOwnerToken = transferred?.active && transferred.key === owner.dataset.nativeDeleteOwner
                ? transferred : {active:true,key:owner.dataset.nativeDeleteOwner,host:owner};
            nativeOwnerToken.host = owner;
            let transferring = false;
            owner.nativeRemove = ${_remove};
            owner.nativeFailure = ${_failure_toast};
            window.addEventListener('lific:native-issue-delete-request', event => {
                const v=event.detail;
                const accepted=${_schedule}(cx.hydrate({t:'i64',bits:64,v:String(v.account_id)}),cx.hydrate({t:'i64',bits:64,v:String(v.issue_id)}),cx.hydrate(v.identifier),cx.hydrate(v.list_path),cx.hydrate(v.detail_path));
                if(accepted.toString()==='true')event.preventDefault();
            },{signal:cx.abortSignal});
            window.addEventListener('pagehide',${_pagehide},{signal:cx.abortSignal});
            document.addEventListener('topcoat:before-page-replace',event=>{
                const next = event.detail.nextDocument.getElementById('native-deferred-delete-owner');
                transferring = next?.getAttribute('data-native-delete-owner') === owner.dataset.nativeDeleteOwner;
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
                }
            },{once:true});
        "#,
            ()
        );
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
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

#[shard("/__native_workspace/toast_icon")]
async fn native_toast_icon(cx: &Cx, kind: String) -> topcoat::Result<impl topcoat::view::View> {
    let icon = match kind.as_str() {
        "error" => UiIcon::Error,
        "success" => UiIcon::Success,
        _ => UiIcon::Info,
    };
    Ok(super::icons::ui_icon(cx, icon, 16))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, process::Stdio, sync::Arc};
    use topcoat::{context::CxTestBuilder, router::RemoteAddr};

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
        let cx = cx.keyed((7_i64, "ACC"));
        let pending = signal(&cx, Vec::<i64>::new);
        owner(&cx, 7, "ACC", pending)
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
                    serde_json::json!({"html":html,"mount":mount})
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
