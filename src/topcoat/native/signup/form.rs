//! Main's signup form, validation feedback and completion state in Rust.
use super::super::super::runtime::whitespace::StrEcmaTrimExt;
use super::super::icons::UiIcon;
use super::super::{icons, transport};
use super::{actions::sign_up, validation};
use topcoat::{
    context::Cx,
    runtime::{Event, Expr, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

use super::super::auth_shell::{BUTTON, INPUT};

#[derive(Clone)]
struct Form {
    username: Signal<String>,
    email: Signal<String>,
    password: Signal<String>,
    error: Signal<String>,
    loading: Signal<bool>,
    username_touched: Signal<bool>,
    email_touched: Signal<bool>,
    password_touched: Signal<bool>,
    visible: Signal<bool>,
    account_done: Signal<bool>,
}

fn submit(cx: &Cx, form: Form) -> Attributes {
    let Form {
        username,
        email,
        password,
        error,
        loading,
        username_touched,
        email_touched,
        password_touched,
        account_done,
        ..
    } = form;
    let username_ok = validation::username_valid(expr!(username.get()));
    let email_ok = validation::email_valid(expr!(email.get()));
    let (password_ok, _, _) = validation::password_rules(expr!(password.get()));
    let failed_loading = loading.clone();
    let failed_error = error.clone();
    let handler = expr!(|event: Event| {
        event.prevent_default();
        if !loading.get() {
            error.set("".to_owned());
            username_touched.set(true);
            email_touched.set(true);
            password_touched.set(true);
            let ready = if !username_ok {
                raw!("document.getElementById('signup-username').focus();", ());
                false
            } else if !email_ok {
                raw!("document.getElementById('signup-email').focus();", ());
                false
            } else if !password_ok {
                raw!("document.getElementById('signup-password').focus();", ());
                false
            } else {
                true
            };
            if ready {
                loading.set(true);
                let sent_username = username.get();
                let sent_email = email.get();
                let sent_password = password.get();
                let _failed = || {
                    failed_loading.set(false);
                    failed_error.set("Unable to create your account. Try again.".to_owned());
                };
                let _run = async || {
                    let result = sign_up(sent_username, sent_email, sent_password).await;
                    if result.0 {
                        account_done.set(true);
                        let _destination = result.1;
                        raw!(
                            "setTimeout(() => window.location.assign(${_destination}.toString()), 750);",
                            ()
                        );
                    } else {
                        loading.set(false);
                        error.set(result.1);
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
        "data-topcoat-on:submit",
        handler.into_evaluated_and_js().1,
    );
    attributes
}

fn requirement<'a>(cx: &'a Cx, label: &'static str, met: Expr<bool>) -> BoxView<'a> {
    view! {cx=>
        <li class="flex items-center gap-2 text-caption transition-colors duration-200" :style=$(if met {"color:var(--success)"}else{"color:var(--text-faint)"})>
            <span class="flex items-center justify-center size-4 rounded-full shrink-0 transition-all duration-200" :style=$(if met {"background-color:color-mix(in srgb, var(--success) 18%, transparent)"}else{"background-color:var(--bg-subtle)"})>
                <svg :hidden=$(!met) width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m20 6-11 11-5-5"></path></svg>
                <span :hidden=$(met) class="size-1 rounded-full bg-[var(--text-faint)]"></span>
            </span>(label)
        </li>
    }.boxed()
}

pub(super) fn content(cx: &Cx, open: bool) -> BoxView<'_> {
    if !open {
        let login = transport::mounted_url(cx, "/login");
        return view! {cx=> <div class="flex flex-col gap-5">
            <div class="flex items-start gap-2.5 text-body-sm text-[var(--text-muted)] bg-[var(--bg-subtle)] px-3.5 py-3 rounded-lg"><span class="shrink-0 mt-0.5 inline-flex">(icons::ui_icon(cx,UiIcon::Restricted,15))</span><span>"New accounts on this instance are created by whoever runs it. Ask them to add you, then come back and sign in."</span></div>
            <button type="button" class=(BUTTON) @click=$(|_event:Event|{raw!("window.location.assign(${login}.toString());",());})>"Go to sign in"</button>
        </div>}.boxed();
    }
    let form = Form {
        username: signal(cx, String::new),
        email: signal(cx, String::new),
        password: signal(cx, String::new),
        error: signal(cx, String::new),
        loading: signal(cx, || false),
        username_touched: signal(cx, || false),
        email_touched: signal(cx, || false),
        password_touched: signal(cx, || false),
        visible: signal(cx, || false),
        account_done: signal(cx, || false),
    };
    let command = submit(cx, form.clone());
    let Form {
        username,
        email,
        password,
        error,
        loading,
        username_touched,
        email_touched,
        password_touched,
        visible,
        account_done,
    } = form;
    let username_ok = validation::username_valid(expr!(username.get()));
    let email_ok = validation::email_valid(expr!(email.get()));
    let (minimum, mixed, symbol) = validation::password_rules(expr!(password.get()));
    let username_error = expr!(if username_touched.get() {
        if username.get().trim_ecmascript().is_empty() {
            "Pick a username."
        } else if !username_ok {
            "Use at least 2 letters, numbers, dashes or underscores."
        } else {
            ""
        }
    } else {
        ""
    });
    let email_error = expr!(if email_touched.get() {
        if email.get().is_empty() {
            "Enter your email."
        } else if !email_ok {
            "That does not look like an email address."
        } else {
            ""
        }
    } else {
        ""
    });
    let met_count = expr!(
        (if minimum { 1_usize } else { 0_usize })
            + (if mixed { 1_usize } else { 0_usize })
            + (if symbol { 1_usize } else { 0_usize })
    );
    let requirements = [
        ("At least 8 characters", minimum.clone()),
        ("A lowercase and uppercase letter", mixed),
        ("A number or symbol", symbol),
    ]
    .into_iter()
    .map(|(label, met)| requirement(cx, label, met))
    .collect::<Vec<_>>();
    let meters=(0..3_usize).map(|index| {
        let count=met_count.clone();
        view! {cx=> <span class="h-1 grow rounded-full transition-colors duration-300" :style=$(if count<=1_usize {if index==0_usize {"background-color:var(--signup-weak)"}else{"background-color:var(--border)"}}else if index>=count {"background-color:var(--border)"}else if count==2_usize {"background-color:var(--signup-fair)"}else{"background-color:var(--success)"})></span>}.boxed()
    }).collect::<Vec<_>>();
    let password_input = expr!(|event: Event| {
        password.set(event.target.value);
        password_touched.set(true);
    });
    let mut password_attributes = Attributes::with_capacity(1);
    password_attributes.insert(
        cx,
        "data-topcoat-on:input",
        password_input.into_evaluated_and_js().1,
    );
    let password_field = super::super::auth_form::password_field(
        cx,
        "signup-password",
        "new-password",
        visible,
        Some("signup-password-reqs"),
        password_attributes,
    );
    view! {cx=>
        <form class="flex flex-col gap-5 [--signup-weak:#c2492f] dark:[--signup-weak:#e06a50] [--signup-fair:#c97a17] dark:[--signup-fair:#e0a54a]" novalidate="novalidate" (command)>
            <div aria-live="polite"><div role="alert" :hidden=$(error.get().is_empty()) class="[&[hidden]]:hidden flex items-start gap-2.5 text-body-sm text-[var(--error)] bg-[var(--tc-error-bg)] px-3.5 py-3 rounded-lg"><span class="shrink-0 mt-0.5 inline-flex">(icons::ui_icon(cx,UiIcon::Warning,15))</span><span>$(error.get())</span></div></div>
            <div class="flex flex-col gap-1.5">
                <label for="signup-username" class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]">"Username"</label>
                <input id="signup-username" type="text" placeholder="jane" autocomplete="username" autocapitalize="none" spellcheck="false" class=(INPUT)
                    :value=$(username.get()) @input=$(|event:Event|username.set(event.target.value)) @blur=$(|_event:Event|username_touched.set(true))
                    :aria-invalid=$(if username_error.is_empty(){"false"}else{"true"}) :aria-describedby=$(if username_error.is_empty(){""}else{"signup-username-err"})/>
                <p id="signup-username-err" class="m-0 text-caption text-[var(--error)]" :hidden=$(username_error.is_empty())>$(username_error)</p>
            </div>
            <div class="flex flex-col gap-1.5">
                <label for="signup-email" class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]">"Email"</label>
                <input id="signup-email" type="email" placeholder="you@example.com" autocomplete="email" autocapitalize="none" spellcheck="false" class=(INPUT)
                    :value=$(email.get()) @input=$(|event:Event|email.set(event.target.value)) @blur=$(|_event:Event|email_touched.set(true))
                    :aria-invalid=$(if email_error.is_empty(){"false"}else{"true"}) :aria-describedby=$(if email_error.is_empty(){""}else{"signup-email-err"})/>
                <p id="signup-email-err" class="m-0 text-caption text-[var(--error)]" :hidden=$(email_error.is_empty())>$(email_error)</p>
            </div>
            <div class="flex flex-col gap-1.5">
                <label for="signup-password" class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]">"Password"</label>
                (password_field)
                <div id="signup-password-reqs" class="[&[hidden]]:hidden mt-2 flex flex-col gap-2.5" :hidden=$(if password_touched.get(){password.get().is_empty()}else{true})>
                    <div class="flex items-center gap-2"><div class="flex gap-1 grow">for meter in meters {(meter)}</div><span class="text-micro font-medium tabular-nums w-[3.25rem] text-right" :style=$(if met_count<=1_usize {"color:var(--signup-weak)"}else if met_count==2_usize {"color:var(--signup-fair)"}else{"color:var(--success)"})>$(if met_count<=1_usize {"Weak"}else if met_count==2_usize {"Fair"}else{"Strong"})</span></div>
                    <ul class="flex flex-col gap-1 list-none m-0 p-0">for requirement in requirements {(requirement)}</ul>
                </div>
            </div>
            <button type="submit" class=(format!("mt-1 {BUTTON}")) :disabled=$(if loading.get(){true}else if !username_ok {true}else if !email_ok {true}else{!minimum})>$(if account_done.get(){"Welcome aboard…"}else if loading.get(){"Creating account…"}else{"Create account"})</button>
        </form>
        <div class="mt-8 pt-6 border-0 border-t border-solid border-[var(--border)]">
            <p class="mt-0 text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-2.5">"Getting started"</p>
            <ul class="rounded-lg border border-solid border-[var(--border)] bg-[var(--surface)] overflow-hidden list-none m-0 p-0">
                <li class="flex items-center gap-2.5 px-3 py-2.5"><span class="[&[hidden]]:hidden inline-flex" :hidden=$(account_done.get())>(icons::status_icon(cx,crate::db::models::Status::Active,15))</span><span class="[&[hidden]]:hidden inline-flex" :hidden=$(!account_done.get())>(icons::status_icon(cx,crate::db::models::Status::Done,15))</span><span class="flex-1 text-body-sm" :class=$(if account_done.get(){"flex-1 text-body-sm line-through text-[var(--text-muted)]"}else{"flex-1 text-body-sm text-[var(--text)]"})>"Create your account"</span><span class="text-micro font-medium capitalize tabular-nums" :style=$(if account_done.get(){"color:var(--success)"}else{"color:var(--accent)"})>$(if account_done.get(){"done"}else{"active"})</span></li>
                <li class="flex items-center gap-2.5 px-3 py-2.5 border-0 border-t border-solid border-[var(--border)]">(icons::status_icon(cx,crate::db::models::Status::Todo,15))<span class="flex-1 text-body-sm text-[var(--text)]">"Start your first project"</span><span class="text-micro font-medium capitalize tabular-nums text-[var(--text-muted)]">"todo"</span></li>
                <li class="flex items-center gap-2.5 px-3 py-2.5 border-0 border-t border-solid border-[var(--border)]">(icons::status_icon(cx,crate::db::models::Status::Backlog,15))<span class="flex-1 text-body-sm text-[var(--text)]">"Connect your AI tools"</span><span class="text-micro font-medium capitalize tabular-nums text-[var(--text-faint)]">"backlog"</span></li>
            </ul>
        </div>
    }.boxed()
}
