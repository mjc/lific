use super::{
    actions::{confirm_connect, connect, profile_session},
    templates::{API_KEY_MARKER, GENERIC_TEMPLATE, TOOL_TEMPLATES, ToolTemplate},
    tool_setup,
};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

#[derive(Clone)]
pub(super) struct ToolDialogState {
    pub(super) open: Signal<bool>,
    pub tool: Signal<String>,
    pub name: Signal<String>,
    password: Signal<String>,
    needs_password: Signal<bool>,
    busy: Signal<bool>,
    key: Signal<String>,
    revealed: Signal<bool>,
    copied: Signal<bool>,
    copy_failed: Signal<bool>,
    error: Signal<String>,
    os: Signal<String>,
    pub(super) setup_template: Signal<String>,
    revision: Signal<usize>,
}

impl ToolDialogState {
    pub(super) fn new(cx: &Cx, revision: Signal<usize>) -> Self {
        Self {
            open: signal(cx, || false),
            tool: signal(cx, String::new),
            name: signal(cx, String::new),
            password: signal(cx, String::new),
            needs_password: signal(cx, || false),
            busy: signal(cx, || false),
            key: signal(cx, String::new),
            revealed: signal(cx, || false),
            copied: signal(cx, || false),
            copy_failed: signal(cx, || false),
            error: signal(cx, String::new),
            os: signal(cx, || "macos".to_owned()),
            setup_template: signal(cx, || GENERIC_TEMPLATE.id.to_owned()),
            revision,
        }
    }
}

pub(super) fn template_card<'a>(
    cx: &'a Cx,
    account: i64,
    template: ToolTemplate,
    state: &ToolDialogState,
) -> BoxView<'a> {
    let attrs = connect_attrs(
        cx,
        account,
        Some((template.id.to_owned(), template.name.to_owned())),
        None,
        state,
    );
    view! { cx =>
        <div class="flex min-w-0 items-center gap-3 rounded-lg border border-[var(--border)] bg-[var(--surface)] p-3" data-settings-tool-template=(template.id)>
            <div class="grid size-9 shrink-0 place-items-center rounded-md bg-[var(--bg-subtle)] text-[var(--text-muted)]">(super::super::icons::ui_icon(cx, super::super::icons::UiIcon::OpenExternal, 17))</div>
            <div class="min-w-0 flex-1">
                <div class="text-body-sm font-medium text-[var(--text)]">(template.name)</div>
                <p class="truncate text-caption text-[var(--text-muted)]">(template.description)</p>
            </div>
            <button type="button" class=(format!("{} border border-[var(--border)] text-[var(--text)] hover:bg-[var(--bg-subtle)]", super::BUTTON))
                data-native-tool-connect=(template.id) (attrs)>"Connect"</button>
        </div>
    }.boxed()
}

pub(super) fn custom_trigger<'a>(
    cx: &'a Cx,
    account: i64,
    state: &ToolDialogState,
    custom_tool: Signal<String>,
    custom_name: Signal<String>,
    reconnect_trigger: bool,
) -> BoxView<'a> {
    let mut attrs = connect_attrs(cx, account, None, Some((custom_tool, custom_name)), state);
    attrs.insert(
        cx,
        if reconnect_trigger {
            "data-native-tool-reconnect-trigger"
        } else {
            "data-native-tool-custom-connect"
        },
        "",
    );
    let class = if reconnect_trigger {
        "hidden"
    } else {
        "mt-3 bg-[var(--btn-success)] text-[var(--btn-success-text)] hover:bg-[var(--btn-success-hover)]"
    };
    view! { cx =>
        <button type="button" class=(format!("{} {class}", super::BUTTON)) (attrs)>
            if !reconnect_trigger { "Connect custom tool" }
        </button>
    }
    .boxed()
}

fn connect_attrs(
    cx: &Cx,
    account: i64,
    fixed_identity: Option<(String, String)>,
    custom_identity: Option<(Signal<String>, Signal<String>)>,
    state: &ToolDialogState,
) -> Attributes {
    let is_template = fixed_identity.is_some();
    let fixed_tool = fixed_identity
        .as_ref()
        .map(|identity| identity.0.clone())
        .unwrap_or_default();
    let fixed_name = fixed_identity
        .as_ref()
        .map(|identity| identity.1.clone())
        .unwrap_or_default();
    let (draft_tool, draft_name) =
        custom_identity.unwrap_or_else(|| (state.tool.clone(), state.name.clone()));
    let open = state.open.clone();
    let tool = state.tool.clone();
    let name = state.name.clone();
    let setup_template = state.setup_template.clone();
    let password = state.password.clone();
    let needs_password = state.needs_password.clone();
    let busy = state.busy.clone();
    let key = state.key.clone();
    let revealed = state.revealed.clone();
    let copied = state.copied.clone();
    let error = state.error.clone();
    let revision = state.revision.clone();
    let mounted = super::super::transport::mounted_url(cx, "/");
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let handler = expr!(|_event: Event| {
        if !busy.get() {
            let requested_tool = if is_template {
                fixed_tool.clone()
            } else {
                let _submitted = draft_tool.get();
                raw!(
                    "cx.hydrate(${_submitted}.trim().toLowerCase())",
                    String::new()
                )
            };
            let requested_name = if is_template {
                fixed_name.clone()
            } else {
                let _submitted = draft_name.get();
                raw!("cx.hydrate(${_submitted}.trim())", String::new())
            };
            if raw!(
                "cx.hydrate(!/^[a-z0-9][a-z0-9_-]{0,47}$/.test(${requested_tool}.toString()))",
                false
            ) {
                error.set(
                    "Use 1–48 lowercase letters, numbers, hyphens, or underscores for the ID."
                        .to_owned(),
                );
                return;
            }
            if requested_name.len() > 80 {
                error.set("Display names must be 80 characters or fewer.".to_owned());
                return;
            }
            let requested_name = if requested_name.is_empty() {
                requested_tool.clone()
            } else {
                requested_name
            };
            tool.set(requested_tool.clone());
            name.set(requested_name.clone());
            if is_template {
                setup_template.set(requested_tool.clone());
            }
            open.set(true);
            key.set("".to_owned());
            password.set("".to_owned());
            needs_password.set(false);
            revealed.set(false);
            copied.set(false);
            error.set("".to_owned());
            busy.set(true);
            let _failed = || {
                if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    failed_busy.set(false);
                    failed_error.set("Couldn't connect this tool. Try again.".to_owned());
                }
            };
            let _run = async || {
                let response =
                    connect(account, requested_tool.clone(), requested_name.clone()).await;
                if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    return;
                }
                if response.is_ok() {
                    let issued = response.unwrap();
                    let fresh = profile_session(account).await;
                    if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                        return;
                    }
                    if fresh.is_ok() {
                        let authority = fresh.unwrap();
                        if authority.is_some() {
                            key.set(issued);
                            password.set("".to_owned());
                            error.set("".to_owned());
                            revision.set(revision.get() + 1);
                        } else {
                            key.set("".to_owned());
                            password.set("".to_owned());
                            raw!("cx.redirect(${mounted}.toString())", ());
                        }
                    } else {
                        key.set("".to_owned());
                        password.set("".to_owned());
                        raw!("cx.redirect(${mounted}.toString())", ());
                    }
                } else {
                    let failure = response.unwrap_err();
                    error.set(failure.message.clone());
                    if failure.requires_confirmation {
                        if failure.automatic_confirmation {
                            let _confirm = async || {
                                let retry = confirm_connect(
                                    account,
                                    requested_tool.clone(),
                                    requested_name.clone(),
                                    None,
                                )
                                .await;
                                if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                                    return;
                                }
                                if retry.is_ok() {
                                    let issued = retry.unwrap();
                                    let fresh = profile_session(account).await;
                                    if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                                        return;
                                    }
                                    if fresh.is_ok() {
                                        let authority = fresh.unwrap();
                                        if authority.is_some() {
                                            key.set(issued);
                                            password.set("".to_owned());
                                            error.set("".to_owned());
                                            revision.set(revision.get() + 1);
                                        } else {
                                            key.set("".to_owned());
                                            password.set("".to_owned());
                                            raw!("cx.redirect(${mounted}.toString())", ());
                                        }
                                    } else {
                                        key.set("".to_owned());
                                        password.set("".to_owned());
                                        raw!("cx.redirect(${mounted}.toString())", ());
                                    }
                                } else {
                                    let retry_failure = retry.unwrap_err();
                                    error.set(retry_failure.message);
                                    needs_password.set(retry_failure.requires_confirmation);
                                }
                            };
                            raw!(
                                "await cx.withSessionChange(cx.abortSignal,()=>${_confirm}())",
                                ()
                            );
                        } else {
                            needs_password.set(true);
                        }
                    }
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

pub(super) fn view<'a>(cx: &'a Cx, account: i64, state: &ToolDialogState) -> BoxView<'a> {
    let open = state.open.clone();
    let busy = state.busy.clone();
    let key = state.key.clone();
    let password = state.password.clone();
    let needs_password = state.needs_password.clone();
    let revealed = state.revealed.clone();
    let copied = state.copied.clone();
    let copy_failed = state.copy_failed.clone();
    let error = state.error.clone();
    let os = state.os.clone();
    let setup_template = state.setup_template.clone();
    let close = expr!(|_event: Event| {
        if !busy.get() {
            open.set(false);
            key.set("".to_owned());
            password.set("".to_owned());
            error.set("".to_owned());
            needs_password.set(false);
            revealed.set(false);
        }
    });
    let reveal = expr!(|_event: Event| revealed.set(!revealed.get()));
    let key_source = signal(cx, || API_KEY_MARKER.to_owned());
    let key_copy = tool_setup::copy_button(
        cx,
        "Copy key",
        key_source,
        key.clone(),
        copied.clone(),
        copy_failed,
        "data-native-tool-key-copy",
    );
    let confirm = confirm_attrs(cx, account, state);
    let mut close_attrs = Attributes::with_capacity(1);
    close_attrs.insert(cx, "data-topcoat-on:click", close.into_evaluated_and_js().1);
    let mut reveal_attrs = Attributes::with_capacity(1);
    reveal_attrs.insert(
        cx,
        "data-topcoat-on:click",
        reveal.into_evaluated_and_js().1,
    );
    let panels = TOOL_TEMPLATES
        .map(|template| setup_panel(cx, template, &setup_template, &open, &key, &revealed, &os));
    let generic_panel = setup_panel(
        cx,
        GENERIC_TEMPLATE,
        &setup_template,
        &open,
        &key,
        &revealed,
        &os,
    );
    let mount_os = os.clone();
    let mount = expr!(|_event: Event| {
        let detected = raw!(
            "cx.hydrate((/Win|Windows/i.test(navigator.platform)?'windows':(/Linux|X11/i.test(navigator.platform)?'linux':'macos')))",
            String::new()
        );
        mount_os.set(detected);
    });
    let mut mount_attrs = Attributes::with_capacity(1);
    mount_attrs.insert(cx, "data-topcoat-on:mount", mount.into_evaluated_and_js().1);
    view! { cx =>
        <div class="fixed inset-0 z-50 grid place-items-center bg-black/50 p-4" (mount_attrs) :hidden=$(!open.get()) data-native-tool-dialog="" role="dialog" aria-modal="true" aria-labelledby="native-tool-dialog-title">
            <div class="max-h-[90vh] w-full max-w-2xl overflow-y-auto rounded-xl bg-[var(--surface)] p-5 shadow-xl">
                <div class="flex items-center justify-between">
                    <h2 id="native-tool-dialog-title" class="text-body font-semibold text-[var(--text)]">"Connect tool"</h2>
                    <button type="button" (close_attrs) :disabled=$(busy.get()) aria-label="Close">"×"</button>
                </div>
                <p class="mt-2 text-caption text-[var(--error)]" role="alert">$(error.get())</p>
                <label class="mt-3 block text-body-sm" :hidden=$(!needs_password.get())>"Current password"
                    <input type="password" autocomplete="current-password" class=(super::INPUT) :value=$(password.get())
                        @input=$(|event: Event| password.set(event.target.value.to_owned())) />
                </label>
                <button type="button" class=(format!("{} mt-2", super::BUTTON)) data-native-tool-confirm="" (confirm) :hidden=$(!needs_password.get()) :disabled=$(busy.get())>
                    $(if busy.get() { "Confirming…" } else { "Confirm and connect" })
                </button>
                <label class="mt-3 block text-body-sm">"Setup instructions"
                    <select class=(super::INPUT) data-native-tool-template-choice="" :value=$(setup_template.get())
                        @change=$(|event: Event| setup_template.set(event.target.value.to_owned()))>
                        <option value=(GENERIC_TEMPLATE.id)>(GENERIC_TEMPLATE.name)</option>
                        for template in TOOL_TEMPLATES { <option value=(template.id)>(template.name)</option> }
                    </select>
                </label>
                <div class="mt-3" :hidden=$(key.get().is_empty())>
                    <p class="font-medium text-[var(--text)]">"Copy your API key now"</p>
                    <code class="block break-all font-mono text-caption text-[var(--text)]">
                        $(if revealed.get() { key.get() } else { "••••••••••••••••".to_owned() })
                    </code>
                    <button type="button" class=(super::BUTTON) data-native-tool-key-reveal="" (reveal_attrs)>
                        $(if revealed.get() { "Hide key" } else { "Reveal key" })
                    </button>
                    (key_copy)
                </div>
                for panel in panels { (panel) }
                (generic_panel)
            </div>
        </div>
    }.boxed()
}

fn setup_panel<'a>(
    cx: &'a Cx,
    template: ToolTemplate,
    active_template: &Signal<String>,
    open: &Signal<bool>,
    key: &Signal<String>,
    revealed: &Signal<bool>,
    os: &Signal<String>,
) -> BoxView<'a> {
    let id = template.id.to_owned();
    let active = active_template.clone();
    let visible = open.clone();
    let key = key.clone();
    let setup = tool_setup::view(cx, template, key.clone(), revealed.clone(), os.clone());
    view! { cx => <div class="mt-4" data-native-tool-setup="" :hidden=$(if !visible.get() { true } else if key.get().is_empty() { true } else { active.get() != id })>(setup)</div> }.boxed()
}

fn confirm_attrs(cx: &Cx, account: i64, state: &ToolDialogState) -> Attributes {
    let busy = state.busy.clone();
    let password = state.password.clone();
    let tool = state.tool.clone();
    let name = state.name.clone();
    let key = state.key.clone();
    let error = state.error.clone();
    let needs = state.needs_password.clone();
    let revision = state.revision.clone();
    let mounted = super::super::transport::mounted_url(cx, "/");
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let handler = expr!(|_event: Event| {
        if !busy.get() {
            busy.set(true);
            let pass = password.get();
            password.set("".to_owned());
            let id = tool.get();
            let display = name.get();
            let _failed = || {
                if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    failed_busy.set(false);
                    failed_error.set("Couldn't confirm connection. Try again.".to_owned());
                }
            };
            let _confirm = async || {
                let confirmed = confirm_connect(account, id, display, Some(pass)).await;
                if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    return;
                }
                if confirmed.is_ok() {
                    let issued = confirmed.unwrap();
                    let fresh = profile_session(account).await;
                    if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                        return;
                    }
                    if fresh.is_ok() {
                        let authority = fresh.unwrap();
                        if authority.is_some() {
                            key.set(issued);
                            password.set("".to_owned());
                            error.set("".to_owned());
                            revision.set(revision.get() + 1);
                            needs.set(false);
                        } else {
                            key.set("".to_owned());
                            password.set("".to_owned());
                            raw!("cx.redirect(${mounted}.toString())", ());
                        }
                    } else {
                        key.set("".to_owned());
                        password.set("".to_owned());
                        raw!("cx.redirect(${mounted}.toString())", ());
                    }
                } else {
                    let failure = confirmed.unwrap_err();
                    error.set(failure.message);
                    needs.set(failure.requires_confirmation);
                }
                busy.set(false);
            };
            raw!(
                "Promise.resolve().then(()=>cx.withSessionChange(cx.abortSignal,()=>${_confirm}())).catch(()=>${_failed}());",
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
