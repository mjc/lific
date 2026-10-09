use super::super::session;
use super::actions::{change_password, profile_session, sign_out, sign_out_all};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};

pub(super) fn section<'a>(cx: &'a Cx, account: i64, tools_revision: Signal<usize>) -> BoxView<'a> {
    view! {
        cx =>
        native_settings_security(account: account, tools_revision: tools_revision)
    }
    .boxed()
}

#[shard("/__native_settings/security")]
async fn native_settings_security(
    cx: &Cx,
    account: i64,
    tools_revision: Signal<usize>,
) -> topcoat::Result<impl View> {
    let _caller = session::read(cx, super::actions::same_account(cx, account))?;
    Ok(render_section(cx, account, tools_revision))
}

#[derive(Clone)]
struct PasswordState {
    current: Signal<String>,
    next: Signal<String>,
    busy: Signal<bool>,
    error: Signal<String>,
    visible: Signal<bool>,
    generation: Signal<usize>,
}

fn password_attrs(
    cx: &Cx,
    account: i64,
    state: PasswordState,
    tools_revision: Signal<usize>,
) -> Attributes {
    let PasswordState {
        current,
        next,
        busy,
        error,
        visible: success,
        generation,
    } = state;
    let destination = super::super::transport::mounted_url(cx, "/");
    let unavailable: Result<Option<String>, String> =
        Err("Unable to verify the current session.".to_owned());
    let refresh_tools = tools_revision;
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let handler = expr!(|_event: Event| {
        if !busy.get() {
            error.set("".to_owned());
            success.set(false);
            generation.increment();
            let old = current.get();
            let new = next.get();
            if new.len() < 8 {
                error.set("New password must be at least 8 characters.".to_owned());
            } else {
                busy.set(true);
                let _failed = || {
                    if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                        return;
                    }
                    failed_busy.set(false);
                    failed_error.set("Couldn't update your password. Try again.".to_owned());
                };
                let _run = async || {
                    let result = change_password(account, old, new).await;
                    if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                        return;
                    }
                    let _read = async || profile_session(account).await;
                    let fresh = raw!(
                        "await Promise.resolve(${_read}()).catch(()=>${unavailable})",
                        Result::<Option<String>, String>::Err(String::new())
                    );
                    if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                        return;
                    }
                    let authorized = if fresh.is_ok() {
                        fresh.unwrap().is_some()
                    } else {
                        false
                    };
                    if !authorized {
                        current.set("".to_owned());
                        next.set("".to_owned());
                        raw!("cx.redirect(${destination}.toString())", ());
                        return;
                    }
                    if result.0 {
                        refresh_tools.set(refresh_tools.get() + 1);
                        current.set("".to_owned());
                        next.set("".to_owned());
                        success.set(true);
                        let current_generation = generation.get();
                        let _expire = || {
                            if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                                if generation.get() == current_generation {
                                    success.set(false);
                                }
                            }
                        };
                        raw!(
                            "const cancel=()=>clearTimeout(timer);const timer=setTimeout(()=>{cx.abortSignal.removeEventListener('abort',cancel);${_expire}();},6000);cx.abortSignal.addEventListener('abort',cancel,{once:true});",
                            ()
                        );
                    } else {
                        error.set(result.1);
                    }
                    busy.set(false);
                };
                raw!(
                    "cx.withSessionChange(cx.abortSignal,()=>${_run}()).catch(()=>${_failed}());",
                    ()
                );
            }
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

fn signout_attrs(
    cx: &Cx,
    account: i64,
    everywhere: bool,
    error: Signal<String>,
    busy: Signal<bool>,
) -> Attributes {
    let failed_error = error.clone();
    let failed_busy = busy.clone();
    let handler = expr!(|_event: Event| {
        if !busy.get() {
            busy.set(true);
            error.set("".to_owned());
            let _failed = || {
                if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    return;
                }
                failed_busy.set(false);
                failed_error.set("Couldn't sign out. Try again.".to_owned());
            };
            let _run = async || {
                let result = if everywhere {
                    sign_out_all(account).await
                } else {
                    sign_out(account).await
                };
                if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    return;
                }
                if result.0 {
                    let _destination = result.1;
                    raw!("window.location.assign(${_destination}.toString())", ());
                } else {
                    error.set(result.1);
                }
                busy.set(false);
            };
            raw!(
                "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                ()
            );
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

fn render_section(cx: &Cx, account: i64, tools_revision: Signal<usize>) -> BoxView<'_> {
    let state_cx = cx.keyed((account, "security"));
    let current = signal(&state_cx, String::new);
    let next = signal(&state_cx, String::new);
    let busy = signal(&state_cx, || false);
    let error = signal(&state_cx, String::new);
    let state = PasswordState {
        current: current.clone(),
        next: next.clone(),
        busy: busy.clone(),
        error: error.clone(),
        visible: signal(&state_cx, || false),
        generation: signal(&state_cx, || 0_usize),
    };
    let success = state.visible.clone();
    let signout_error = signal(&state_cx, String::new);
    let signout_busy = signal(&state_cx, || false);
    let confirm_all = signal(&state_cx, || false);
    let password = password_attrs(cx, account, state, tools_revision);
    let signout_everywhere = signout_attrs(
        cx,
        account,
        true,
        signout_error.clone(),
        signout_busy.clone(),
    );
    let signout = signout_attrs(cx, account, false, signout_error.clone(), signout_busy);
    view! {
        cx =>
        <section class="mt-8 border-t border-[var(--border)] pt-6">
            <h3 class="mb-3.5 text-body-lg font-semibold text-[var(--text)]">
                "Password"
            </h3>
            <div class="flex max-w-[480px] flex-col gap-2.5">
                <input
                    type="password"
                    autocomplete="current-password"
                    placeholder="Current password"
                    class=(super::INPUT)
                    :value=$(current.get())
                    @input=$(|event: Event| current.set(event.target.value.to_owned()))
                />
                <input
                    type="password"
                    autocomplete="new-password"
                    placeholder="New password (min 8 chars)"
                    class=(super::INPUT)
                    :value=$(next.get())
                    @input=$(|event: Event| next.set(event.target.value.to_owned()))
                />
            </div>
            <p
                class="mt-2.5 max-w-[480px] text-caption leading-relaxed text-[var(--text-muted)]"
            >
                "Changing your password signs you out on every other device and revokes this account's API keys, OAuth sessions and connected tools. You stay signed in here. Every tool you use with Lific has to be reconnected afterwards."
            </p>
            <p class="mt-2 text-caption text-[var(--error)]" role="alert">
                $(error.get())
            </p>
            <div class="mt-3 flex items-center gap-3">
                <button
                    type="button"
                    class=(format!(
                        "{} bg-[var(--btn-success)] text-[var(--btn-success-text)] hover:bg-[var(--btn-success-hover)] disabled:opacity-40",
                        super::BUTTON,
                    ))
                    (password)
                >
                    $(if busy.get() { "Updating…" } else { "Change password" })
                </button>
                <span
                    role="status"
                    class="text-body-sm text-[var(--success)]"
                    :hidden=$(!success.get())
                >
                    "✓ Password changed. Connected tools were revoked; reconnect them below."
                </span>
            </div>
        </section>
        <section class="mt-8 border-t border-[var(--border)] pt-6">
            <h3 class="mb-1 text-body-lg font-semibold text-[var(--text)]">
                "Sessions"
            </h3>
            <p class="mb-3.5 text-body-sm leading-relaxed text-[var(--text-muted)]">
                "Sign out of this device, or revoke access everywhere at once."
            </p>
            <div class="flex flex-wrap items-center gap-2">
                <button
                    type="button"
                    class=(format!(
                        "{} border border-[var(--border)] text-[var(--text)] hover:bg-[var(--bg-subtle)]",
                        super::BUTTON,
                    ))
                    (signout)
                >
                    "Sign out"
                </button>
                <button
                    type="button"
                    class=(format!(
                        "{} border border-[var(--error)] text-[var(--error)] hover:bg-[var(--error-bg)]",
                        super::BUTTON,
                    ))
                    @click=$(|_event: Event| confirm_all.set(!confirm_all.get()))
                >
                    "Sign out everywhere and revoke access"
                </button>
            </div>
            <p class="mt-2 text-caption text-[var(--error)]" role="alert">
                $(signout_error.get())
            </p>
            <div
                class="mt-3 flex max-w-[480px] flex-col gap-2 rounded-md border border-[var(--error)] bg-[var(--error-bg)] px-3 py-2.5"
                role="alert"
                :hidden=$(!confirm_all.get())
            >
                <p class="text-caption leading-relaxed text-[var(--text)]">
                    "This signs out every device, including this one, and revokes this account's API keys, OAuth sessions and connected tools. Every tool you use with Lific has to be reconnected afterwards. This cannot be undone."
                </p>
                <button
                    type="button"
                    class=(format!(
                        "{} bg-[var(--error)] text-[var(--error-text)]",
                        super::BUTTON,
                    ))
                    (signout_everywhere)
                >
                    "Confirm sign out everywhere"
                </button>
            </div>
        </section>
    }.boxed()
}
