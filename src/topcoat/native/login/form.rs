//! Main's login fields and completion state expressed in the Rust runtime.
use super::super::super::runtime::whitespace::StrEcmaTrimExt;
use super::{
    super::icons,
    actions::{automatic, sign_in},
};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

fn submit(
    cx: &Cx,
    identity: Signal<String>,
    password: Signal<String>,
    error: Signal<String>,
    loading: Signal<bool>,
    touched: Signal<bool>,
    auto: bool,
) -> Attributes {
    let failed_loading = loading.clone();
    let failed_error = error.clone();
    let handler = expr!(|event: Event| {
        event.prevent_default();
        if !loading.get() {
            touched.set(!auto);
            error.set("".to_owned());
            let ready = if auto {
                true
            } else {
                if identity.get().trim_ecmascript().is_empty() {
                    raw!("document.getElementById('login-identity')?.focus();", ());
                    false
                } else if password.get().is_empty() {
                    raw!("document.getElementById('login-password')?.focus();", ());
                    false
                } else {
                    true
                }
            };
            if ready {
                loading.set(true);
                let sent_identity = identity.get();
                let sent_password = password.get();
                let _failed = || {
                    failed_loading.set(false);
                    failed_error.set("Unable to sign in. Try again.".to_owned());
                    if !auto {
                        raw!("document.getElementById('login-password')?.focus();", ());
                    }
                };
                let _run = async || {
                    let result = if auto {
                        automatic().await
                    } else {
                        sign_in(sent_identity, sent_password).await
                    };
                    if result.0 {
                        let _destination = result.1;
                        raw!("window.location.assign(${_destination}.toString());", ());
                    } else {
                        loading.set(false);
                        error.set(result.1);
                        if !auto {
                            raw!("document.getElementById('login-password')?.focus();", ());
                        }
                    }
                };
                raw!(
                    "Promise.resolve().then(() => ${_run}()).catch(() => ${_failed}());",
                    ()
                );
            }
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        if auto {
            "data-topcoat-on:mount"
        } else {
            "data-topcoat-on:submit"
        },
        handler.into_evaluated_and_js().1,
    );
    attributes
}

pub(super) fn content(cx: &Cx, auto: bool) -> BoxView<'_> {
    let identity = signal(cx, String::new);
    let password = signal(cx, String::new);
    let error = signal(cx, String::new);
    let loading = signal(cx, || false);
    let touched = signal(cx, || false);
    let visible = signal(cx, || false);
    let invalid = expr!(if touched.get() {
        identity.get().trim_ecmascript().is_empty()
    } else {
        false
    });
    let command = submit(
        cx,
        identity.clone(),
        password.clone(),
        error.clone(),
        loading.clone(),
        touched.clone(),
        false,
    );
    let automatic_mount = if auto {
        Some(submit(
            cx,
            identity.clone(),
            password.clone(),
            error.clone(),
            loading.clone(),
            touched.clone(),
            true,
        ))
    } else {
        None
    };
    view! {cx =>
        if let Some(mount)=automatic_mount {<span hidden="hidden" (mount)></span>}
        <form class="flex flex-col gap-5" novalidate="novalidate" (command)>
            <div aria-live="polite"><div role="alert" :hidden=$(error.get().is_empty()) class="[&[hidden]]:hidden flex items-start gap-2.5 text-body-sm text-[var(--error)] bg-[var(--tc-error-bg)] px-3.5 py-3 rounded-lg">(icons::project_icon(cx,Some("lucide:AlertTriangle"),15))<span>$(error.get())</span></div></div>
            <div class="flex flex-col gap-1.5">
                <label for="login-identity" class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]">"Username or email"</label>
                <input id="login-identity" type="text" placeholder="jane" autocomplete="username" autocapitalize="none" spellcheck="false" class="font-body text-[var(--tc-text)] bg-[var(--tc-surface)] border border-solid border-[var(--tc-border)] outline-none transition-[border-color,box-shadow] duration-200 placeholder:text-[var(--tc-faint)] focus:border-[var(--tc-accent)] focus:shadow-[0_0_0_3px_var(--tc-accent-subtle)] [@media(pointer:coarse)]:text-[16px]! aria-invalid:border-[var(--tc-danger)] rounded-lg px-3.5 py-2.5 text-body-lg"
                    :value=$(identity.get()) @input=$(|event:Event| identity.set(event.target.value)) @blur=$(|_event:Event| touched.set(true))
                    :aria-invalid=$(if invalid {"true"}else{"false"})
                    :aria-describedby=$(if invalid {"login-identity-err"}else{""})/>
                <p id="login-identity-err" class="text-caption text-[var(--error)]" :hidden=$(!invalid)>"Enter your username or email."</p>
            </div>
            <div class="flex flex-col gap-1.5">
                <label for="login-password" class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]">"Password"</label>
                <div class="relative">
                    <input id="login-password" :type=$(if visible.get(){"text"}else{"password"}) autocomplete="current-password" class="font-body text-[var(--tc-text)] bg-[var(--tc-surface)] border border-solid border-[var(--tc-border)] outline-none transition-[border-color,box-shadow] duration-200 placeholder:text-[var(--tc-faint)] focus:border-[var(--tc-accent)] focus:shadow-[0_0_0_3px_var(--tc-accent-subtle)] [@media(pointer:coarse)]:text-[16px]! w-full rounded-lg pl-3.5 pr-11 py-2.5 text-body-lg"
                        @input=$(|event:Event| password.set(event.target.value))/>
                    <button type="button" tabindex="-1" :aria-pressed=$(visible.get()) :aria-label=$(if visible.get(){"Hide password"}else{"Show password"}) :title=$(if visible.get(){"Hide password"}else{"Show password"})
                        class="absolute inset-y-0 right-0 flex items-center px-3 text-[var(--text-faint)] hover:text-[var(--text-muted)] transition-colors focus-visible:outline-none focus-visible:text-[var(--accent)] bg-transparent border-0"
                        @click=$(|_event:Event| visible.set(!visible.get()))>
                        <span :hidden=$(visible.get())>(icons::project_icon(cx,Some("lucide:Eye"),17))</span><span :hidden=$(!visible.get())>(icons::project_icon(cx,Some("lucide:EyeOff"),17))</span>
                    </button>
                </div>
            </div>
            <button type="submit" :disabled=$(if loading.get(){true}else if identity.get().trim_ecmascript().is_empty(){true}else{password.get().is_empty()})
                class="mt-1 rounded-lg bg-[var(--tc-btn-success)] text-[var(--tc-btn-success-text)] text-body-lg font-medium py-2.5 px-5 transition-all duration-200 hover:bg-[#2ed673] dark:hover:bg-[#54c97e] motion-safe:active:scale-[0.98] focus-visible:ring-2 focus-visible:ring-[var(--tc-btn-success)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--bg)] disabled:opacity-55 disabled:cursor-not-allowed disabled:hover:bg-[var(--tc-btn-success)] dark:disabled:hover:bg-[var(--tc-btn-success)] border-0">$(if loading.get(){"Signing in…"}else{"Sign in"})</button>
        </form>
    }.boxed()
}
