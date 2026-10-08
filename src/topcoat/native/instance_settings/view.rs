//! Native instance settings and member roster initial document.

use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, record, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};

use super::super::super::runtime::whitespace::StrEcmaTrimExt;

use super::super::{context, session};
use super::actions::{confirm_member_action, confirm_name, mutate_member, save_text};

#[derive(Clone)]
struct NameState {
    name: Signal<String>,
    saved_name: Signal<String>,
    saving: Signal<bool>,
    saved: Signal<bool>,
    queued_name: Signal<Option<String>>,
    last_submission: Signal<String>,
    error: Signal<String>,
    parked_name: Signal<Option<String>>,
    needs_confirmation: Signal<bool>,
    password: Signal<String>,
    confirming: Signal<bool>,
    confirmation_error: Signal<String>,
}

#[derive(Clone)]
struct RosterState {
    admin_count: Signal<usize>,
    settings_saving: Signal<bool>,
    settings_confirmation: Signal<bool>,
    pending_id: Signal<i64>,
    pending_action: Signal<String>,
    busy_id: Signal<i64>,
    row_error_id: Signal<i64>,
    row_error: Signal<String>,
    reauth_id: Signal<i64>,
    reauth_action: Signal<String>,
    reauth_password: Signal<String>,
    reauth_busy: Signal<bool>,
    reauth_error: Signal<String>,
}

#[record]
#[derive(Clone)]
struct RosterEntry {
    id: i64,
    username: String,
    display_name: String,
    initials: String,
    is_admin: bool,
    is_active: bool,
    created_at: String,
}

pub(super) fn content(cx: &Cx, account: i64, is_admin: bool) -> BoxView<'_> {
    let tabs =
        super::super::settings_tabs::view(cx, super::super::settings_tabs::Tab::Instance, is_admin);
    let back_to_account = super::super::navigation::attrs(cx, "/settings");
    if !is_admin {
        return view! {
            cx =>
            <div class="flex-1 overflow-y-auto">
                <div class="mx-auto w-full max-w-[1000px] px-6 py-10 md:py-12">
                    (tabs)
                    <section
                        class="mx-auto flex max-w-[440px] flex-col items-center py-20 text-center"
                    >
                        <h1 class="text-[1rem] font-semibold text-[var(--text)]">
                            "Admins only"
                        </h1>
                        <p class="mt-1 text-body text-[var(--text-muted)]">
                            "Instance settings are visible to administrators of this instance."
                        </p>
                        <a
                            class="mt-5 rounded-md bg-[var(--btn-success)] px-3 py-1.5 text-body-sm font-medium text-[var(--btn-success-text)] hover:bg-[var(--btn-success-hover)]"
                            (back_to_account)
                        >
                            "Back to account"
                        </a>
                    </section>
                </div>
            </div>
        }
        .boxed();
    }
    view! { cx => native_instance_settings(account: account) }.boxed()
}

fn save_name_attrs(cx: &Cx, account: i64, state: NameState) -> Attributes {
    let NameState {
        name,
        saved_name,
        saving,
        saved,
        queued_name,
        last_submission,
        error,
        parked_name,
        needs_confirmation,
        confirming,
        ..
    } = state;
    let failed_name = name.clone();
    let failed_saved_name = saved_name.clone();
    let failed_saving = saving.clone();
    let failed_error = error.clone();
    let failed_submission = last_submission.clone();
    let recent_auth_required = crate::auth::RECENT_AUTH_REQUIRED_MESSAGE.to_owned();
    let handler = expr!(|_event: Event| {
        if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
            return;
        }
        let requested = name.get().trim_ecmascript().to_owned();
        name.set(requested.clone());
        let baseline = saved_name.get();
        if confirming.get() {
            queued_name.set(Some(requested));
        } else if needs_confirmation.get() {
            parked_name.set(Some(requested));
        } else if saving.get() {
            queued_name.set(Some(requested));
        } else if requested != baseline {
            queued_name.set(Some(requested));
            saving.set(true);
            saved.set(false);
            error.set("".to_owned());
            let _live = || !raw!("cx.hydrate(cx.abortSignal.aborted)", false);
            let _failed = || {
                if raw!("${_live}()", true) {
                    failed_saving.set(false);
                    if failed_name.get().trim_ecmascript().to_owned() == failed_submission.get() {
                        failed_name.set(failed_saved_name.get());
                    }
                    failed_error.set("Couldn't save the instance name. Try again.".to_owned());
                }
            };
            let _save = async || {
                while queued_name.get().is_some() {
                    if !raw!("${_live}()", false) {
                        return;
                    }
                    let submission = queued_name.get().unwrap();
                    queued_name.set(None);
                    let previous = saved_name.get();
                    if submission != previous {
                        saved.set(false);
                        error.set("".to_owned());
                        last_submission.set(submission.clone());
                        let request_submission = submission.clone();
                        let _request = async || {
                            save_text(account, "name".to_owned(), request_submission).await
                        };
                        let _transport_error =
                            "Couldn't save the instance name. Try again.".to_owned();
                        let result = raw!(
                            "await ${_request}().catch(()=>cx.hydrate([false,${_transport_error}.toString()]))",
                            (false, String::new())
                        );
                        if !raw!("${_live}()", false) {
                            return;
                        }
                        if result.0 {
                            saved_name.set(result.1.clone());
                            if name.get().trim_ecmascript().to_owned() == submission {
                                name.set(result.1.clone());
                            }
                            saved.set(true);
                        } else if result.1 == recent_auth_required {
                            error.set(result.1.clone());
                            let pending = if queued_name.get().is_some() {
                                queued_name.get().unwrap()
                            } else {
                                submission.clone()
                            };
                            queued_name.set(None);
                            parked_name.set(Some(pending));
                            needs_confirmation.set(true);
                        } else {
                            if name.get().trim_ecmascript().to_owned() == submission {
                                name.set(previous);
                            }
                            error.set(result.1.clone());
                        }
                    }
                    let _iteration_complete = false;
                }
                saving.set(false);
            };
            raw!(
                "Promise.resolve().then(()=>cx.withSessionChange(cx.abortSignal,()=>${_save}())).catch(()=>${_failed}());",
                ()
            );
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:blur",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

fn confirmation_attrs(cx: &Cx, account: i64, state: NameState) -> Attributes {
    let NameState {
        name,
        saved_name,
        saving,
        saved,
        queued_name,
        parked_name,
        needs_confirmation,
        password,
        confirming,
        confirmation_error,
        error,
        last_submission,
        ..
    } = state;
    let failed_confirming = confirming.clone();
    let failed_error = confirmation_error.clone();
    let failed_needs_confirmation = needs_confirmation.clone();
    let failed_parked_name = parked_name.clone();
    let failed_queued_name = queued_name.clone();
    let failed_submission = last_submission.clone();
    let failed_saving = saving.clone();
    let recent_auth_required = crate::auth::RECENT_AUTH_REQUIRED_MESSAGE.to_owned();
    let confirm = expr!(|_event: Event| {
        if !needs_confirmation.get() {
            return;
        }
        if confirming.get() {
            return;
        }
        if password.get().is_empty() {
            return;
        }
        let submission = if parked_name.get().is_some() {
            parked_name.get().unwrap()
        } else {
            name.get().trim_ecmascript().to_owned()
        };
        let current_password = password.get();
        last_submission.set(submission.clone());
        parked_name.set(None);
        confirming.set(true);
        confirmation_error.set("".to_owned());
        error.set("".to_owned());
        saved.set(false);
        let _live = || !raw!("cx.hydrate(cx.abortSignal.aborted)", false);
        let _failed = || {
            if raw!("${_live}()", true) {
                failed_saving.set(false);
                failed_confirming.set(false);
                failed_error.set("Couldn't confirm your password. Try again.".to_owned());
                failed_needs_confirmation.set(true);
                let pending = if failed_queued_name.get().is_some() {
                    failed_queued_name.get().unwrap()
                } else {
                    failed_submission.get()
                };
                failed_parked_name.set(Some(pending));
                failed_queued_name.set(None);
            }
        };
        let _confirm = async || {
            let result = confirm_name(account, submission.clone(), current_password).await;
            if !raw!("${_live}()", false) {
                return;
            }
            if result.0 {
                saved_name.set(result.1.clone());
                if name.get().trim_ecmascript().to_owned() == submission {
                    name.set(result.1.clone());
                }
                needs_confirmation.set(false);
                confirmation_error.set("".to_owned());
                password.set("".to_owned());
                error.set("".to_owned());
                saved.set(true);
                while queued_name.get().is_some() {
                    let queued = queued_name.get().unwrap();
                    queued_name.set(None);
                    let previous = saved_name.get();
                    if queued != previous {
                        saved.set(false);
                        saving.set(true);
                        last_submission.set(queued.clone());
                        let result = save_text(account, "name".to_owned(), queued.clone()).await;
                        if !raw!("${_live}()", false) {
                            return;
                        }
                        if result.0 {
                            saved_name.set(result.1.clone());
                            parked_name.set(None);
                            if name.get().trim_ecmascript().to_owned() == queued {
                                name.set(result.1.clone());
                            }
                            saved.set(true);
                        } else if result.1 == recent_auth_required {
                            let pending = if queued_name.get().is_some() {
                                queued_name.get().unwrap()
                            } else {
                                queued
                            };
                            queued_name.set(None);
                            parked_name.set(Some(pending));
                            needs_confirmation.set(true);
                            error.set(result.1.clone());
                            queued_name.set(None);
                        } else {
                            if name.get().trim_ecmascript().to_owned() == queued {
                                name.set(previous);
                            }
                            error.set(result.1.clone());
                            if queued_name.get().is_some() {
                                parked_name.set(Some(queued_name.get().unwrap()));
                            }
                            queued_name.set(None);
                        }
                    }
                    let _iteration_complete = false;
                }
                saving.set(false);
            } else {
                let pending = if queued_name.get().is_some() {
                    queued_name.get().unwrap()
                } else {
                    submission.clone()
                };
                queued_name.set(None);
                parked_name.set(Some(pending));
                needs_confirmation.set(true);
                confirmation_error.set(result.1);
            }
            confirming.set(false);
        };
        raw!(
            "Promise.resolve().then(()=>cx.withSessionChange(cx.abortSignal,()=>${_confirm}())).catch(()=>${_failed}());",
            ()
        );
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        confirm.into_evaluated_and_js().1,
    );
    attrs
}

fn cancel_confirmation_attrs(cx: &Cx, state: NameState) -> Attributes {
    let NameState {
        name,
        saved_name,
        queued_name,
        parked_name,
        needs_confirmation,
        password,
        confirming,
        confirmation_error,
        error,
        saved,
        ..
    } = state;
    let cancel = expr!(|_event: Event| {
        if !confirming.get() {
            name.set(saved_name.get());
            queued_name.set(None);
            parked_name.set(None);
            needs_confirmation.set(false);
            password.set("".to_owned());
            confirmation_error.set("".to_owned());
            error.set("".to_owned());
            saved.set(false);
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        cancel.into_evaluated_and_js().1,
    );
    attrs
}

fn roster_action_attrs(
    cx: &Cx,
    account: i64,
    user_id: i64,
    action: &str,
    is_admin: Signal<bool>,
    is_active: Signal<bool>,
    state: &RosterState,
) -> Attributes {
    let admin_count = state.admin_count.clone();
    let settings_saving = state.settings_saving.clone();
    let settings_confirmation = state.settings_confirmation.clone();
    let pending_id = state.pending_id.clone();
    let pending_action = state.pending_action.clone();
    let busy_id = state.busy_id.clone();
    let row_error_id = state.row_error_id.clone();
    let row_error = state.row_error.clone();
    let reauth_id = state.reauth_id.clone();
    let reauth_action = state.reauth_action.clone();
    let reauth_password = state.reauth_password.clone();
    let reauth_error = state.reauth_error.clone();
    let recent_auth_required = crate::auth::RECENT_AUTH_REQUIRED_MESSAGE.to_owned();
    let failure_busy_id = busy_id.clone();
    let failure_row_error_id = row_error_id.clone();
    let failure_row_error = row_error.clone();
    let action = action.to_owned();
    let handler = expr!(|_event: Event| {
        let operation = if action == "pending" {
            pending_action.get()
        } else {
            action.clone()
        };
        let grants_access = if operation == "promote" {
            true
        } else {
            operation == "reactivate"
        };
        if busy_id.get() != 0_i64 {
            return;
        }
        if grants_access {
            if reauth_id.get() != 0_i64 {
                return;
            }
            if settings_saving.get() {
                return;
            }
            if settings_confirmation.get() {
                return;
            }
        }
        busy_id.set(user_id);
        pending_id.set(0_i64);
        row_error.set("".to_owned());
        let _live = || !raw!("cx.hydrate(cx.abortSignal.aborted)", false);
        let _failed = || {
            if raw!("${_live}()", true) {
                failure_busy_id.set(0_i64);
                failure_row_error_id.set(user_id);
                failure_row_error.set("Couldn't update this member. Try again.".to_owned());
            }
        };
        let _save = async || {
            let result = mutate_member(account, user_id, operation.clone()).await;
            if !raw!("${_live}()", false) {
                return;
            }
            busy_id.set(0_i64);
            if result.0 {
                let was_active_admin = if is_admin.get() {
                    is_active.get()
                } else {
                    false
                };
                let is_active_admin = if result.1 { result.2 } else { false };
                if was_active_admin != is_active_admin {
                    admin_count.set(if is_active_admin {
                        admin_count.get() + 1_usize
                    } else {
                        admin_count.get() - 1_usize
                    });
                }
                is_admin.set(result.1);
                is_active.set(result.2);
                if grants_access {
                    reauth_id.set(0_i64);
                    reauth_password.set("".to_owned());
                    reauth_error.set("".to_owned());
                }
                row_error.set("".to_owned());
            } else {
                if result.3 == recent_auth_required {
                    if operation == "promote" {
                        reauth_id.set(user_id);
                        reauth_action.set(operation.clone());
                        reauth_password.set("".to_owned());
                        reauth_error.set("".to_owned());
                    } else if operation == "reactivate" {
                        reauth_id.set(user_id);
                        reauth_action.set(operation.clone());
                        reauth_password.set("".to_owned());
                        reauth_error.set("".to_owned());
                    } else {
                        row_error_id.set(user_id);
                        row_error.set(result.3);
                    }
                } else {
                    row_error_id.set(user_id);
                    row_error.set(result.3);
                }
            }
        };
        raw!(
            "Promise.resolve().then(()=>cx.withSessionChange(cx.abortSignal,()=>${_save}())).catch(()=>${_failed}());",
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

fn request_confirmation_attrs(
    cx: &Cx,
    user_id: i64,
    action: &str,
    state: &RosterState,
) -> Attributes {
    let pending_id = state.pending_id.clone();
    let pending_action = state.pending_action.clone();
    let busy_id = state.busy_id.clone();
    let row_error = state.row_error.clone();
    let row_error_id = state.row_error_id.clone();
    let action = action.to_owned();
    let handler = expr!(|_event: Event| {
        if busy_id.get() != 0_i64 {
            return;
        }
        pending_id.set(user_id);
        pending_action.set(action.clone());
        row_error_id.set(0_i64);
        row_error.set("".to_owned());
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

fn confirm_member_reauth_attrs(
    cx: &Cx,
    account: i64,
    user_id: i64,
    is_admin: Signal<bool>,
    is_active: Signal<bool>,
    state: &RosterState,
) -> Attributes {
    let admin_count = state.admin_count.clone();
    let reauth_id = state.reauth_id.clone();
    let reauth_action = state.reauth_action.clone();
    let reauth_password = state.reauth_password.clone();
    let reauth_busy = state.reauth_busy.clone();
    let reauth_error = state.reauth_error.clone();
    let row_error = state.row_error.clone();
    let recent_auth_required = crate::auth::RECENT_AUTH_REQUIRED_MESSAGE.to_owned();
    let failure_reauth_id = reauth_id.clone();
    let failure_reauth_busy = reauth_busy.clone();
    let failure_reauth_error = reauth_error.clone();
    let handler = expr!(|_event: Event| {
        let action_user_id = user_id;
        let action = reauth_action.get();
        let password = reauth_password.get();
        if reauth_id.get() != action_user_id {
            return;
        }
        if action_user_id == 0_i64 {
            return;
        }
        if password.is_empty() {
            return;
        }
        if reauth_busy.get() {
            return;
        }
        reauth_busy.set(true);
        reauth_error.set("".to_owned());
        let _live = || !raw!("cx.hydrate(cx.abortSignal.aborted)", false);
        let _failed = || {
            if raw!("${_live}()", true) {
                if failure_reauth_id.get() == action_user_id {
                    failure_reauth_busy.set(false);
                    failure_reauth_error
                        .set("Couldn't confirm your password. Try again.".to_owned());
                }
            }
        };
        let _confirm = async || {
            let result =
                confirm_member_action(account, action_user_id, action.clone(), password).await;
            if !raw!("${_live}()", false) {
                return;
            }
            if reauth_id.get() != action_user_id {
                return;
            }
            reauth_busy.set(false);
            if result.0 {
                let was_active_admin = if is_admin.get() {
                    is_active.get()
                } else {
                    false
                };
                let is_active_admin = if result.1 { result.2 } else { false };
                if was_active_admin != is_active_admin {
                    admin_count.set(if is_active_admin {
                        admin_count.get() + 1_usize
                    } else {
                        admin_count.get() - 1_usize
                    });
                }
                is_admin.set(result.1);
                is_active.set(result.2);
                reauth_id.set(0_i64);
                reauth_password.set("".to_owned());
                reauth_error.set("".to_owned());
                row_error.set("".to_owned());
            } else if result.3 == recent_auth_required {
                reauth_error.set(
                    "That still was not accepted. Sign out and sign back in, then try again."
                        .to_owned(),
                );
            } else {
                reauth_error.set(result.3);
            }
        };
        raw!(
            "Promise.resolve().then(()=>cx.withSessionChange(cx.abortSignal,()=>${_confirm}())).catch(()=>${_failed}());",
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

fn cancel_member_reauth_attrs(cx: &Cx, state: &RosterState) -> Attributes {
    let reauth_id = state.reauth_id.clone();
    let reauth_password = state.reauth_password.clone();
    let reauth_error = state.reauth_error.clone();
    let reauth_busy = state.reauth_busy.clone();
    let handler = expr!(|_event: Event| {
        if !reauth_busy.get() {
            reauth_id.set(0_i64);
            reauth_password.set("".to_owned());
            reauth_error.set("".to_owned());
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

#[shard("/__native_instance_settings/page")]
async fn native_instance_settings(cx: &Cx, account: i64) -> topcoat::Result<impl View> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return Err(crate::error::LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        )
        .into());
    }
    if !user.is_admin {
        return Err(crate::error::LificError::Forbidden("only an admin can do this".into()).into());
    }
    let db = context::db(cx);
    let settings = session::read(cx, crate::db::queries::settings::get(&*db.read()?))?;
    let users = session::read(
        cx,
        crate::services::project_form::list_leads(db, &caller.identity),
    )?;
    let name_owner = cx.keyed((account, "instance-name"));
    let initial_name = settings.instance_name.unwrap_or_default();
    let name = signal(&name_owner, || initial_name.clone());
    let saved_name = signal(&name_owner, || initial_name);
    let saving = signal(&name_owner, || false);
    let saved = signal(&name_owner, || false);
    let queued_name = signal(&name_owner, || None::<String>);
    let last_submission = signal(&name_owner, String::new);
    let error = signal(&name_owner, String::new);
    let parked_name = signal(&name_owner, || None::<String>);
    let needs_confirmation = signal(&name_owner, || false);
    let password = signal(&name_owner, String::new);
    let confirming = signal(&name_owner, || false);
    let confirmation_error = signal(&name_owner, String::new);
    let recent_auth_required = crate::auth::RECENT_AUTH_REQUIRED_MESSAGE.to_owned();
    let name_state = NameState {
        name: name.clone(),
        saved_name,
        saving: saving.clone(),
        saved: saved.clone(),
        queued_name,
        last_submission,
        error: error.clone(),
        parked_name,
        needs_confirmation: needs_confirmation.clone(),
        password: password.clone(),
        confirming: confirming.clone(),
        confirmation_error: confirmation_error.clone(),
    };
    let save_name = save_name_attrs(cx, account, name_state.clone());
    let confirm_attrs = confirmation_attrs(cx, account, name_state.clone());
    let cancel_confirmation = cancel_confirmation_attrs(cx, name_state);
    let host = topcoat::router::request::headers(cx)
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let signup_status = if settings.allow_signup {
        "Open"
    } else {
        "Closed"
    };
    let domains = if settings.signup_email_domains.is_empty() {
        "Any email domain".to_owned()
    } else {
        settings.signup_email_domains.join(", ")
    };
    let session_lifetime = format!("{} days", settings.session_lifetime_days);
    let login_message = settings.login_message.unwrap_or_else(|| "None".into());
    let single_user = if settings.web_auto_login {
        "Enabled"
    } else {
        "Sign-in required"
    };
    let authorization = if settings.authz_enforced {
        "Enforced"
    } else {
        "Legacy access"
    };
    let roster = users
        .into_iter()
        .map(|user| {
            let display_name = if user.display_name.trim().is_empty() {
                user.username.clone()
            } else {
                user.display_name.clone()
            };
            RosterEntry {
                id: user.id,
                username: user.username,
                initials: initials(&display_name),
                display_name,
                is_admin: user.is_admin,
                is_active: user.is_active,
                created_at: user.created_at,
            }
        })
        .collect::<Vec<_>>();
    let roster_cx = cx.keyed((account, "instance-roster"));
    let roster_state = RosterState {
        admin_count: signal(&roster_cx, || {
            roster
                .iter()
                .filter(|user| user.is_admin && user.is_active)
                .count()
        }),
        settings_saving: saving.clone(),
        settings_confirmation: needs_confirmation.clone(),
        pending_id: signal(&roster_cx, || 0_i64),
        pending_action: signal(&roster_cx, String::new),
        busy_id: signal(&roster_cx, || 0_i64),
        row_error_id: signal(&roster_cx, || 0_i64),
        row_error: signal(&roster_cx, String::new),
        reauth_id: signal(&roster_cx, || 0_i64),
        reauth_action: signal(&roster_cx, String::new),
        reauth_password: signal(&roster_cx, String::new),
        reauth_busy: signal(&roster_cx, || false),
        reauth_error: signal(&roster_cx, String::new),
    };
    let people_count = roster.len();
    let singular_people = people_count == 1;
    let roster_admin_count = roster_state.admin_count.clone();
    let roster_rows = roster
        .iter()
        .map(|user| roster_row(cx, account, user, &roster_state))
        .collect::<Vec<_>>();
    Ok(view! {
        cx =>
        <div class="flex-1 overflow-y-auto">
            <div
                class="mx-auto w-full max-w-[1000px] px-6 py-10 md:py-12"
                data-native-instance-settings=""
            >
                (super::super::settings_tabs::view(
                    cx,
                    super::super::settings_tabs::Tab::Instance,
                    true,
                ))
                <section class="mb-8">
                    <h1
                        class="font-display text-title tracking-tight text-[var(--text)] leading-none"
                    >
                        "Instance"
                    </h1>
                    <p class="mt-2 text-body text-[var(--text-muted)]">
                        "Settings for this Lific instance."
                    </p>
                </section>
                <section
                    class="rounded-xl bg-[var(--surface)] p-5 shadow-sm"
                    aria-labelledby="instance-settings-heading"
                >
                    <h2
                        id="instance-settings-heading"
                        class="mb-5 text-body-lg font-semibold text-[var(--text)]"
                    >
                        "Settings"
                    </h2>
                    <dl class="grid max-w-[640px] gap-4 sm:grid-cols-2">
                        <div class="sm:col-span-2">
                            <label class="block">
                                <span
                                    class="mb-1.5 block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                                >
                                    "Instance name"
                                </span>
                                <input
                                    class=(super::super::settings::INPUT)
                                    data-native-instance-name=""
                                    maxlength="60"
                                    placeholder=(host)
                                    :value=$(name.get())
                                    @input=$(|event: Event| {
                                        name.set(event.target.value.to_owned());
                                        saved.set(false);
                                        error.set("".to_owned());
                                    })
                                    (save_name)
                                />
                            </label>
                            <p class="mt-1.5 text-caption text-[var(--text-muted)]">
                                "Shown on the sign-in screen. Leave blank to use the host."
                            </p>
                            <p
                                class="mt-2 text-caption text-[var(--error)]"
                                role="alert"
                            >
                                $(error.get())
                            </p>
                            <p
                                class="mt-2 text-caption text-[var(--text-muted)]"
                                role="status"
                                aria-live="polite"
                            >
                                $(if saving.get() {
                                    "Saving…"
                                } else if saved.get() {
                                    "Saved"
                                } else if error.get().is_empty() {
                                    "Changes save automatically."
                                } else if error.get() == recent_auth_required {
                                    "That change needs a recent sign-in."
                                } else {
                                    ""
                                })
                            </p>
                            <div
                                class="mt-4 rounded-lg border border-[var(--border)] bg-[var(--bg-subtle)] p-4"
                                data-native-instance-reauth=""
                                :hidden=$(!needs_confirmation.get())
                            >
                                <p class="text-body-sm text-[var(--text)]">
                                    "Your session needs confirmation before this name can be saved."
                                </p>
                                <p
                                    class="mt-1 text-caption text-[var(--text-muted)]"
                                    role="status"
                                >
                                    "Confirm your password, then the pending name will be saved."
                                </p>
                                <label class="mt-3 block">
                                    <span class="sr-only">"Your current password"</span>
                                    <input
                                        class=(super::super::settings::INPUT)
                                        data-native-instance-reauth-password=""
                                        type="password"
                                        placeholder="your current password"
                                        autocomplete="current-password"
                                        :value=$(password.get())
                                        :disabled=$(confirming.get())
                                        @input=$(|event: Event| {
                                            password.set(event.target.value.to_owned());
                                            confirmation_error.set("".to_owned());
                                        })
                                    />
                                </label>
                                <p
                                    class="mt-2 text-caption text-[var(--error)]"
                                    role="alert"
                                    aria-live="polite"
                                >
                                    $(confirmation_error.get())
                                </p>
                                <div class="mt-3 flex items-center gap-2">
                                    <button
                                        class="rounded-md bg-[var(--btn-success)] px-3 py-1.5 text-body-sm font-medium text-[var(--btn-success-text)] hover:bg-[var(--btn-success-hover)] disabled:cursor-not-allowed disabled:opacity-50"
                                        type="button"
                                        :disabled=$(if confirming.get() {
                                            true
                                        } else if password.get().is_empty() {
                                            true
                                        } else {
                                            false
                                        })
                                        (confirm_attrs)
                                    >
                                        $(if confirming.get() {
                                            "Verifying…"
                                        } else {
                                            "Confirm and continue"
                                        })
                                    </button>
                                    <button
                                        class="rounded-md px-3 py-1.5 text-body-sm text-[var(--text-muted)] hover:bg-[var(--surface)] disabled:cursor-not-allowed disabled:opacity-50"
                                        type="button"
                                        :disabled=$(confirming.get())
                                        (cancel_confirmation)
                                    >
                                        "Cancel"
                                    </button>
                                </div>
                            </div>
                        </div>
                        <div>
                            <dt
                                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                            >
                                "Sign-ups"
                            </dt>
                            <dd class="mt-1 text-body text-[var(--text)]">
                                (signup_status)
                            </dd>
                        </div>
                        <div>
                            <dt
                                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                            >
                                "Allowed signup domains"
                            </dt>
                            <dd class="mt-1 text-body text-[var(--text)]">(domains)</dd>
                        </div>
                        <div>
                            <dt
                                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                            >
                                "Session lifetime"
                            </dt>
                            <dd class="mt-1 text-body text-[var(--text)]">
                                (session_lifetime)
                            </dd>
                        </div>
                        <div class="sm:col-span-2">
                            <dt
                                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                            >
                                "Login message"
                            </dt>
                            <dd
                                class="mt-1 whitespace-pre-wrap text-body text-[var(--text)]"
                            >
                                (login_message)
                            </dd>
                        </div>
                        <div>
                            <dt
                                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                            >
                                "Single-user mode"
                            </dt>
                            <dd class="mt-1 text-body text-[var(--text)]">
                                (single_user)
                            </dd>
                        </div>
                        <div>
                            <dt
                                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                            >
                                "Project authorization"
                            </dt>
                            <dd class="mt-1 text-body text-[var(--text)]">
                                (authorization)
                            </dd>
                        </div>
                    </dl>
                </section>
                <section class="mt-10" aria-labelledby="instance-members-heading">
                    <h2
                        id="instance-members-heading"
                        class="text-[1rem] font-semibold text-[var(--text)]"
                    >
                        "Members"
                    </h2>
                    <p class="mb-5 mt-1 text-body text-[var(--text-muted)]">
                        (people_count)
                        " "
                        if singular_people {
                            "person"
                        } else {
                            "people"
                        }
                        " on this instance · "
                        $(roster_admin_count.get())
                        " admin."
                    </p>
                    <div
                        class="overflow-hidden rounded-xl bg-[var(--surface)] shadow-sm"
                    >
                        for row in roster_rows {
                            (row)
                        }
                    </div>
                    <p
                        class="mt-2 max-w-[60ch] text-caption leading-relaxed text-[var(--text-faint)]"
                    >
                        "Deactivating an account ends its sessions and revokes its API keys and tokens. Nothing it wrote is removed. The last admin who can still sign in cannot be demoted or deactivated."
                    </p>
                </section>
            </div>
        </div>
    })
}

fn roster_row<'a>(
    cx: &'a Cx,
    account: i64,
    user: &RosterEntry,
    state: &RosterState,
) -> BoxView<'a> {
    let id = user.id;
    let display_name = user.display_name.clone();
    let username = user.username.clone();
    let initials = user.initials.clone();
    let created_at = super::super::dates::absolute(cx, &user.created_at);
    let make_admin_label = format!("make @{username} an admin.");
    let restore_label = format!("restore @{username}.");
    let remove_admin_label = format!("Remove instance admin from {display_name}");
    let promote_label = format!("Make {display_name} an instance admin");
    let deactivate_label = format!("Deactivate {display_name}");
    let reactivate_label = format!("Restore {display_name}");
    let row_cx = cx.keyed((account, id));
    let is_admin = signal(&row_cx, || user.is_admin);
    let is_active = signal(&row_cx, || user.is_active);
    let settings_saving = state.settings_saving.clone();
    let settings_confirmation = state.settings_confirmation.clone();
    let reauth_id = state.reauth_id.clone();
    let reauth_action = state.reauth_action.clone();
    let reauth_password = state.reauth_password.clone();
    let reauth_busy = state.reauth_busy.clone();
    let reauth_error = state.reauth_error.clone();
    let demote = request_confirmation_attrs(cx, id, "demote", state);
    let deactivate = request_confirmation_attrs(cx, id, "deactivate", state);
    let promote = roster_action_attrs(
        cx,
        account,
        id,
        "promote",
        is_admin.clone(),
        is_active.clone(),
        state,
    );
    let reactivate = roster_action_attrs(
        cx,
        account,
        id,
        "reactivate",
        is_admin.clone(),
        is_active.clone(),
        state,
    );
    let cancel_pending = state.pending_id.clone();
    let cancel_busy = state.busy_id.clone();
    let cancel = expr!(|_event: Event| {
        if cancel_busy.get() == 0_i64 {
            cancel_pending.set(0_i64);
        }
    });
    let mut cancel_attrs = Attributes::with_capacity(1);
    cancel_attrs.insert(
        cx,
        "data-topcoat-on:click",
        cancel.into_evaluated_and_js().1,
    );
    let confirm_pending = roster_action_attrs(
        cx,
        account,
        id,
        "pending",
        is_admin.clone(),
        is_active.clone(),
        state,
    );
    let confirm_reauth =
        confirm_member_reauth_attrs(cx, account, id, is_admin.clone(), is_active.clone(), state);
    let cancel_reauth = cancel_member_reauth_attrs(cx, state);
    let when_busy = state.busy_id.clone();
    let when_pending = state.pending_id.clone();
    let when_action = state.pending_action.clone();
    let when_reauth = state.reauth_id.clone();
    let row_error_id = state.row_error_id.clone();
    let row_error = state.row_error.clone();
    let row = view! {
        cx =>
        <div data-native-instance-member-row=(id.to_string())>
            <div class="flex items-center gap-3 px-4 py-3">
                <div
                    class="grid size-8 shrink-0 place-items-center rounded-full bg-[var(--accent)] text-micro font-semibold text-[var(--accent-text)]"
                >
                    (initials)
                </div>
                <div class="min-w-0 flex-1">
                    <div class="truncate leading-tight text-body text-[var(--text)]">
                        (display_name.clone())
                        if id == account {
                            <span class="text-caption text-[var(--text-faint)]">
                                " (you)"
                            </span>
                        }
                    </div>
                    <div
                        class="mt-0.5 truncate font-mono text-caption text-[var(--text-faint)]"
                    >
                        "@"
                        (username)
                    </div>
                </div>
                <span
                    class="shrink-0 rounded-full bg-[var(--warn-bg)] px-1.5 py-0.5 text-micro font-semibold uppercase text-[var(--warn-text)]"
                    :hidden=$(is_active.get())
                >
                    "Deactivated"
                </span>
                <span
                    class="shrink-0 rounded-full bg-[var(--accent-subtle)] px-1.5 py-0.5 text-micro font-semibold uppercase text-[var(--accent)]"
                    :hidden=$(!is_admin.get())
                >
                    "Admin"
                </span>
                <span
                    class="shrink-0 rounded-full bg-[var(--bg-subtle)] px-1.5 py-0.5 text-micro font-semibold uppercase text-[var(--text-muted)]"
                    :hidden=$(is_admin.get())
                >
                    "Member"
                </span>
                <span
                    class="hidden w-[5.5rem] shrink-0 text-right text-caption tabular-nums text-[var(--text-faint)] sm:block"
                >
                    (created_at)
                </span>
                if id != account {
                    <div
                        class="flex shrink-0 items-center gap-1"
                        :hidden=$(when_pending.get() == id)
                    >
                        <button
                            type="button"
                            title="Remove instance admin"
                            aria-label=(remove_admin_label)
                            class="rounded-md px-2 py-1 text-caption text-[var(--text-muted)] hover:bg-[var(--bg-subtle)] hover:text-[var(--warn-text)]"
                            :hidden=$(!is_admin.get())
                            :disabled=$(when_busy.get() != 0_i64)
                            (demote)
                        >
                            "Demote"
                        </button>
                        <button
                            type="button"
                            title="Make instance admin"
                            aria-label=(promote_label)
                            class="rounded-md px-2 py-1 text-caption text-[var(--text-muted)] hover:bg-[var(--accent-subtle)] hover:text-[var(--accent)] disabled:cursor-not-allowed disabled:opacity-40"
                            :hidden=$(is_admin.get())
                            :disabled=$(if when_busy.get() != 0_i64 {
                                true
                            } else if when_reauth.get() != 0_i64 {
                                true
                            } else if settings_saving.get() {
                                true
                            } else {
                                settings_confirmation.get()
                            })
                            (promote)
                        >
                            "Promote"
                        </button>
                        <button
                            type="button"
                            title="Deactivate account"
                            aria-label=(deactivate_label)
                            class="rounded-md px-2 py-1 text-caption text-[var(--text-muted)] hover:bg-[var(--error-bg)] hover:text-[var(--error)]"
                            :hidden=$(!is_active.get())
                            :disabled=$(when_busy.get() != 0_i64)
                            (deactivate)
                        >
                            "Deactivate"
                        </button>
                        <button
                            type="button"
                            title="Restore account"
                            aria-label=(reactivate_label)
                            class="rounded-md px-2 py-1 text-caption text-[var(--text-muted)] hover:bg-[var(--success-bg)] hover:text-[var(--success)] disabled:cursor-not-allowed disabled:opacity-40"
                            :hidden=$(is_active.get())
                            :disabled=$(if when_busy.get() != 0_i64 {
                                true
                            } else if when_reauth.get() != 0_i64 {
                                true
                            } else if settings_saving.get() {
                                true
                            } else {
                                settings_confirmation.get()
                            })
                            (reactivate)
                        >
                            "Restore"
                        </button>
                    </div>
                    if id != account {
                        <div
                            class="flex shrink-0 items-center gap-1.5"
                            :hidden=$(when_pending.get() != id)
                        >
                            <button
                                type="button"
                                data-native-instance-member-confirm=""
                                class="rounded-md bg-[var(--error)] px-2 py-1 text-caption font-medium text-[var(--error-text)] hover:opacity-90 disabled:opacity-40"
                                :disabled=$(when_busy.get() != 0_i64)
                                (confirm_pending)
                            >
                                $(if when_busy.get() == id {
                                    "…"
                                } else if when_action.get() == "demote" {
                                    "Demote"
                                } else {
                                    "Deactivate"
                                })
                            </button>
                            <button
                                type="button"
                                class="rounded-md px-2 py-1 text-caption text-[var(--text-muted)] hover:bg-[var(--bg-subtle)]"
                                :disabled=$(when_busy.get() != 0_i64)
                                (cancel_attrs)
                            >
                                "Cancel"
                            </button>
                        </div>
                    }
                }
            </div>
            <p
                class="px-4 pb-2.5 -mt-1 text-caption text-[var(--error)]"
                role="alert"
                :hidden=$(if row_error.get().is_empty() {
                    true
                } else {
                    row_error_id.get() != id
                })
            >
                $(row_error.get())
            </p>
            if id != account {
                <div
                    class="mx-4 mb-3 rounded-lg border border-[var(--border)] bg-[var(--bg-subtle)] p-4"
                    data-native-instance-member-reauth=""
                    :hidden=$(reauth_id.get() != id)
                >
                    <p class="text-body-sm text-[var(--text)]">
                        "Verify it's you to "
                        $(if reauth_action.get() == "promote" {
                            make_admin_label
                        } else {
                            restore_label
                        })
                        " Expanding access needs a recent sign-in, and you have been signed in for a while."
                    </p>
                    <label class="mt-3 block">
                        <span class="sr-only">"Your current password"</span>
                        <input
                            class=(super::super::settings::INPUT)
                            data-native-instance-member-reauth-password=""
                            type="password"
                            placeholder="your current password"
                            autocomplete="current-password"
                            :value=$(reauth_password.get())
                            :disabled=$(reauth_busy.get())
                            @input=$(|event: Event| {
                                reauth_password.set(event.target.value.to_owned());
                                reauth_error.set("".to_owned());
                            })
                        />
                    </label>
                    <p
                        class="mt-2 text-caption text-[var(--error)]"
                        role="alert"
                        aria-live="polite"
                    >
                        $(reauth_error.get())
                    </p>
                    <div class="mt-3 flex items-center gap-2">
                        <button
                            type="button"
                            class="rounded-md bg-[var(--btn-success)] px-3 py-1.5 text-body-sm font-medium text-[var(--btn-success-text)] hover:bg-[var(--btn-success-hover)] disabled:cursor-not-allowed disabled:opacity-50"
                            :disabled=$(if reauth_busy.get() {
                                true
                            } else {
                                reauth_password.get().is_empty()
                            })
                            (confirm_reauth)
                        >
                            $(if reauth_busy.get() {
                                "Verifying…"
                            } else {
                                "Confirm and continue"
                            })
                        </button>
                        <button
                            type="button"
                            class="rounded-md px-3 py-1.5 text-body-sm text-[var(--text-muted)] hover:bg-[var(--surface)] disabled:cursor-not-allowed disabled:opacity-50"
                            :disabled=$(reauth_busy.get())
                            (cancel_reauth)
                        >
                            "Cancel"
                        </button>
                    </div>
                </div>
            }
        </div>
    };
    row.boxed()
}

fn initials(name: &str) -> String {
    name.split([' ', '_', '-'])
        .filter_map(|word| word.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}
