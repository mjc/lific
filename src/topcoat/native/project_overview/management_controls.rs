//! Shared inline grant confirmation for members and project lead.
use super::management::{attempt as manage_attempt, confirm as manage_confirm};
use super::management_model::Continuation;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};
// Locked, busy, open, command kind/user/role, note, error, previous role, label.
pub(super) type GrantSignals = (
    Signal<bool>,
    Signal<bool>,
    Signal<bool>,
    Signal<String>,
    Signal<Option<i64>>,
    Signal<String>,
    Signal<String>,
    Signal<String>,
    Signal<String>,
    Signal<String>,
);

#[derive(Clone)]
pub(super) struct Pending {
    pub(super) open: Signal<bool>,
    pub(super) locked: Signal<bool>,
    pub(super) busy: Signal<bool>,
    pub(super) error: Signal<String>,
    pub(super) automatic_note: Signal<String>,
    kind: Signal<String>,
    label: Signal<String>,
    user: Signal<Option<i64>>,
    role: Signal<String>,
    previous: Signal<String>,
    password: Signal<String>,
}
impl Pending {
    pub(super) fn new(cx: &Cx, continuation: Option<&Continuation>, label: String) -> Self {
        let (kind, user, role, previous) = continuation.map_or_else(
            || ("", None, String::new(), String::new()),
            |value| value.command.wire(),
        );
        Self {
            open: signal(cx, || continuation.is_some()),
            locked: signal(cx, || continuation.is_some()),
            busy: signal(cx, || false),
            error: signal(cx, || {
                continuation
                    .map(|value| value.error.clone())
                    .unwrap_or_default()
            }),
            automatic_note: signal(cx, || {
                continuation
                    .map(|value| value.automatic_note.clone())
                    .unwrap_or_default()
            }),
            kind: signal(cx, || kind.into()),
            label: signal(cx, || label),
            user: signal(cx, || user),
            role: signal(cx, || role),
            previous: signal(cx, || previous),
            password: signal(cx, String::new),
        }
    }
    pub(super) fn kind_for_rows(&self) -> Signal<String> {
        self.kind.clone()
    }
    pub(super) fn user_for_rows(&self) -> Signal<Option<i64>> {
        self.user.clone()
    }
    pub(super) fn role_for_rows(&self) -> Signal<String> {
        self.role.clone()
    }
    pub(super) fn previous_for_rows(&self) -> Signal<String> {
        self.previous.clone()
    }
    pub(super) fn label_for_rows(&self) -> Signal<String> {
        self.label.clone()
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_rows(
        cx: &Cx,
        open: Signal<bool>,
        locked: Signal<bool>,
        busy: Signal<bool>,
        kind: Signal<String>,
        user: Signal<Option<i64>>,
        role: Signal<String>,
        previous: Signal<String>,
        label: Signal<String>,
        automatic_note: Signal<String>,
        error: Signal<String>,
    ) -> Self {
        Self {
            open,
            locked,
            busy,
            kind,
            user,
            role,
            previous,
            label,
            automatic_note,
            error,
            password: signal(cx, String::new),
        }
    }
}
pub(super) fn role_number(value: &str) -> i64 {
    match value {
        "lead" => 2,
        "maintainer" => 1,
        _ => 0,
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn attempt(
    cx: &Cx,
    pending: &Pending,
    account: i64,
    project: i64,
    kind: &str,
    user: Signal<Option<i64>>,
    role: Signal<Option<i64>>,
    previous: String,
    label: String,
    rollback: Option<i64>,
    revision: Signal<usize>,
    error: Signal<String>,
    error_target: Option<Signal<i64>>,
    event: &str,
) -> Attributes {
    let kind = kind.to_owned();
    let open = pending.open.clone();
    let locked = pending.locked.clone();
    let busy = pending.busy.clone();
    let pending_kind = pending.kind.clone();
    let pending_label = pending.label.clone();
    let pending_user = pending.user.clone();
    let pending_role = pending.role.clone();
    let pending_previous = pending.previous.clone();
    let pending_error = pending.error.clone();
    let note = pending.automatic_note.clone();
    let failed_locked = locked.clone();
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let target_role = role.clone();
    let rollback_enabled = kind == "member_role";
    let is_lead = kind == "lead";
    let targeted = error_target.unwrap_or_else(|| signal(cx, || 0_i64));
    let failed_revision = revision.clone();
    let failed_user = user.clone();
    let failed_role = target_role.clone();
    // Each move closure owns its captured rollback and command values.
    let failed_rollback = rollback;
    let failed_kind = kind.clone();
    let no_user: Option<i64> = None;
    let default_role = Some(0_i64);
    let handler = expr!(|_event: Event| {
        if !locked.get() {
            let selected = user.get();
            let role_value = role.get();
            let number = if role_value.is_some() {
                role_value.unwrap()
            } else {
                0_i64
            };
            let original_role = if number == 2_i64 {
                "lead".to_owned()
            } else if number == 1_i64 {
                "maintainer".to_owned()
            } else {
                "viewer".to_owned()
            };
            let unchanged = if is_lead {
                let selected_number = if selected.is_some() {
                    selected.clone().unwrap()
                } else {
                    -1_i64
                };
                let rollback_number = if rollback.is_some() {
                    rollback.clone().unwrap()
                } else {
                    -1_i64
                };
                selected_number == rollback_number
            } else if rollback_enabled {
                original_role == previous
            } else {
                false
            };
            if !unchanged {
                // Original IDs/roles are captured before any await. Subsequent
                // confirmation reads only the frozen pending signals below.
                locked.set(true);
                busy.set(true);
                error.set("".to_owned());
                if if kind == "member_role" {
                    true
                } else {
                    kind == "member_remove"
                } {
                    let target = if selected.is_some() {
                        selected.clone().unwrap()
                    } else {
                        0_i64
                    };
                    targeted.set(target);
                } else {
                    targeted.set(0_i64);
                }
                let _failed = || {
                    failed_locked.set(false);
                    failed_busy.set(false);
                    failed_error.set("The change did not complete. Try again.".to_owned());
                    if rollback_enabled {
                        failed_role.set(failed_rollback.clone());
                    }
                    if is_lead {
                        failed_user.set(failed_rollback.clone());
                    }
                    if if rollback_enabled {
                        true
                    } else if is_lead {
                        true
                    } else {
                        failed_kind == "member_remove"
                    } {
                        failed_revision.increment();
                    }
                };
                let _run = async || {
                    let result = manage_attempt(
                        account,
                        project,
                        kind.clone(),
                        selected.clone(),
                        original_role.clone(),
                        previous.clone(),
                        true,
                    )
                    .await;
                    busy.set(false);
                    if result.0 == "saved" {
                        locked.set(false);
                        revision.increment();
                        if kind == "member_add" {
                            user.set(no_user.clone());
                            target_role.set(default_role.clone());
                        }
                    } else if if result.0 == "navigate" {
                        true
                    } else {
                        result.0 == "resume"
                    } {
                        let _destination = result.1;
                        raw!("cx.navigate(${_destination}.toString());", ());
                    } else if result.0 == "reauth" {
                        open.set(true);
                        pending_kind.set(kind.clone());
                        pending_label.set(label);
                        pending_user.set(selected);
                        pending_role.set(original_role);
                        pending_previous.set(previous);
                        pending_error.set(result.1);
                        note.set(result.2);
                    } else {
                        locked.set(false);
                        error.set(result.1);
                        if rollback_enabled {
                            target_role.set(rollback.clone());
                        }
                        if is_lead {
                            user.set(rollback.clone());
                        }
                        if if rollback_enabled {
                            true
                        } else if is_lead {
                            true
                        } else {
                            kind == "member_remove"
                        } {
                            revision.increment();
                        }
                    }
                };
                raw!(
                    "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                    ()
                );
            }
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        format!("data-topcoat-on:{event}"),
        handler.into_evaluated_and_js().1,
    );
    attrs
}
pub(super) fn prompt<'a>(
    cx: &'a Cx,
    pending: &Pending,
    account: i64,
    project: i64,
    revision: Signal<usize>,
    lead: bool,
) -> BoxView<'a> {
    let open = pending.open.clone();
    let locked = pending.locked.clone();
    let busy = pending.busy.clone();
    let error = pending.error.clone();
    let note = pending.automatic_note.clone();
    let password = pending.password.clone();
    let kind = pending.kind.clone();
    let label = pending.label.clone();
    let user = pending.user.clone();
    let role = pending.role.clone();
    let previous = pending.previous.clone();
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let confirm = expr!(|event: Event| {
        if if event.event_type == "click" {
            true
        } else {
            event.key == "Enter"
        } {
            event.prevent_default();
            if if !busy.get() {
                !password.get().is_empty()
            } else {
                false
            } {
                busy.set(true);
                error.set("".to_owned());
                let frozen_kind = kind.get();
                let frozen_user = user.get();
                let frozen_role = role.get();
                let frozen_previous = previous.get();
                let entered = password.get();
                let _failed = || {
                    failed_busy.set(false);
                    failed_error.set("Confirmation did not complete. Try again.".to_owned());
                };
                let _confirm = async || {
                    let result = manage_confirm(
                        account,
                        project,
                        frozen_kind,
                        frozen_user,
                        frozen_role,
                        frozen_previous,
                        entered,
                    )
                    .await;
                    busy.set(false);
                    password.set("".to_owned());
                    if if result.0 == "navigate" {
                        true
                    } else {
                        result.0 == "resume"
                    } {
                        let _destination = result.1;
                        raw!("cx.navigate(${_destination}.toString());", ());
                    } else {
                        error.set(result.1);
                    }
                };
                raw!(
                    "Promise.resolve().then(()=>${_confirm}()).catch(()=>${_failed}());",
                    ()
                );
            }
        }
    })
    .into_evaluated_and_js()
    .1;
    let mut confirmation_keys = Attributes::with_capacity(1);
    confirmation_keys.insert(cx, "data-topcoat-on:keydown", confirm.clone());
    let mut confirmation_click = Attributes::with_capacity(1);
    confirmation_click.insert(cx, "data-topcoat-on:click", confirm);
    view!{
        cx =>
        <div class="native-overview__grant" :hidden=$(!open.get())>
            <p>
                if lead {
                    "Verify it's you to make that person the project lead. It grants them lead access to this project, and you have been signed in for a while."
                } else {
                    if kind.get() == "member_add" {
                        "Verify it's you to add this person as "
                        $(role.get())
                        "."
                    } else {
                        "Verify it's you to make "
                        <span class="native-overview__grant-target">
                            $(label.get())
                        </span>
                        " a "
                        $(role.get())
                        "."
                    }
                    " It grants them access to this project, and you have been signed in for a while."
                }
            </p>
            <p role="status" :hidden=$(note.get().is_empty())>
                "Signing you in automatically did not work ("
                $(note.get())
                ")."
            </p>
            <p role="alert" :hidden=$(error.get().is_empty())>$(error.get())</p>
            <div>
                <label>
                    <span class="sr-only">"Your current password"</span>
                    <input
                        type="password"
                        autocomplete="current-password"
                        placeholder="your current password"
                        :disabled=$(busy.get())
                        :value=$(password.get())
                        @input=$(|event: Event| password.set(event.target.value))
                        (confirmation_keys)
                    />
                </label>
                <button
                    type="button"
                    class="native-overview__success"
                    :disabled=$(if busy.get() {
                        true
                    } else {
                        password.get().is_empty()
                    })
                    (confirmation_click)
                >
                    $(if busy.get() { "Verifying…" } else { "Confirm and continue" })
                </button>
                <button
                    type="button"
                    :disabled=$(busy.get())
                    @click=$(|_event: Event| {
                        open.set(false);
                        locked.set(false);
                        password.set("".to_owned());
                        error.set("".to_owned());
                        note.set("".to_owned());
                        revision.increment();
                    })
                >
                    "Cancel"
                </button>
            </div>
        </div>
    }.boxed()
}
