use super::super::{
    context,
    icons::{UiIcon, ui_icon},
    session,
};
use super::{
    actions::{bot_action, connect},
    templates::{TOOL_TEMPLATES, ToolTemplate},
};
use crate::db::models::Bot;
use topcoat::{
    context::Cx,
    runtime::{BoolSurrogate, Event, I64Surrogate, Signal, Surrogated, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};

pub(super) fn section<'a>(cx: &'a Cx, account: i64) -> BoxView<'a> {
    view! { cx => native_settings_tools(account: account) }.boxed()
}

#[shard("/__native_settings/tools")]
async fn native_settings_tools(cx: &Cx, account: i64) -> topcoat::Result<impl View> {
    let _caller = session::read(cx, super::actions::same_account(cx, account))?;
    Ok(render_section(cx, account))
}

#[derive(Clone)]
struct ToolsState {
    revision: Signal<usize>,
    id: Signal<String>,
    name: Signal<String>,
    password: Signal<String>,
    reauth: Signal<bool>,
    busy: Signal<bool>,
    key: Signal<String>,
    error: Signal<String>,
    bot_busy: Signal<i64>,
    bot_error: Signal<String>,
}

fn select_tool_attrs(
    cx: &Cx,
    id: Signal<String>,
    name: Signal<String>,
    next_id: String,
    next_name: String,
) -> Attributes {
    let handler = expr!(|_event: Event| {
        id.set(next_id.clone());
        name.set(next_name.clone());
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

fn template_card<'a>(
    cx: &'a Cx,
    template: ToolTemplate,
    tool_id: Signal<String>,
    tool_name: Signal<String>,
) -> BoxView<'a> {
    let select = select_tool_attrs(
        cx,
        tool_id,
        tool_name,
        template.id.to_owned(),
        template.name.to_owned(),
    );
    view! {
        cx =>
        <div
            class="flex min-w-0 items-center gap-3 rounded-lg border border-[var(--border)] bg-[var(--surface)] p-3"
            data-settings-tool-template=(template.id)
        >
            <div
                class="grid size-9 shrink-0 place-items-center rounded-md bg-[var(--bg-subtle)] text-[var(--text-muted)]"
            >
                (ui_icon(cx, UiIcon::OpenExternal, 17))
            </div>
            <div class="min-w-0 flex-1">
                <div class="text-body-sm font-medium text-[var(--text)]">
                    (template.name)
                </div>
                <p class="truncate text-caption text-[var(--text-muted)]">
                    (template.description)
                </p>
            </div>
            <button
                type="button"
                class=(format!(
                    "{} border border-[var(--border)] text-[var(--text)] hover:bg-[var(--bg-subtle)]",
                    super::BUTTON,
                ))
                (select)
            >
                "Use template"
            </button>
        </div>
    }.boxed()
}

fn delegated_bot_actions(cx: &Cx, account: i64, state: &ToolsState) -> Attributes {
    let revision = state.revision.clone();
    let busy = state.bot_busy.clone();
    let error = state.bot_error.clone();
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let handler = expr!(|_event: Event| {
        let _dispatch = |id: I64Surrogate, remove: BoolSurrogate| {
            if busy.get() == 0_i64 {
                busy.set(id);
                error.set("".to_owned());
                let _failed = || {
                    if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                        return;
                    }
                    failed_busy.set(0_i64);
                    failed_error.set("Couldn't update this connection. Try again.".to_owned());
                };
                let _run = async || {
                    let result = bot_action(account, id, remove).await;
                    if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                        return;
                    }
                    if result.0 {
                        revision.set(revision.get() + 1);
                    } else {
                        error.set(result.1);
                    }
                    busy.set(0_i64);
                };
                raw!(
                    "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                    ()
                );
            }
        };
        raw!(
            r#"const target=${_event}.inner.target instanceof Element?${_event}.inner.target:${_event}.inner.target?.parentElement;
            const button=target?.closest('button[data-native-bot-action]');
            if(button&&${_event}.inner.currentTarget.contains(button)){
                try{const args=JSON.parse(button.getAttribute('data-native-bot-action'));
                ${_dispatch}(cx.hydrate(args[0]),cx.hydrate(args[1]));}catch{}
            }"#,
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

fn bot_action_wire(id: i64, remove: bool) -> String {
    serde_json::to_string(&(id, remove).into_surrogate())
        .expect("bot actions contain scalar surrogate arguments")
}

fn connect_attrs(cx: &Cx, account: i64, state: &ToolsState) -> Attributes {
    let tool = state.id.clone();
    let display_name = state.name.clone();
    let password = state.password.clone();
    let reauth = state.reauth.clone();
    let key = state.key.clone();
    let busy = state.busy.clone();
    let error = state.error.clone();
    let revision = state.revision.clone();
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let handler = expr!(|_event: Event| {
        if !busy.get() {
            busy.set(true);
            key.set("".to_owned());
            error.set("".to_owned());
            let requested_tool = tool.get();
            let requested_name = display_name.get();
            let requested_password = if reauth.get() {
                Some(password.get())
            } else {
                None
            };
            let _failed = || {
                if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    return;
                }
                failed_busy.set(false);
                failed_error.set("Couldn't connect this tool. Try again.".to_owned());
            };
            let _run = async || {
                let result =
                    connect(account, requested_tool, requested_name, requested_password).await;
                if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    return;
                }
                if result.0 {
                    key.set(result.1);
                    revision.set(revision.get() + 1);
                    password.set("".to_owned());
                    reauth.set(false);
                } else if result.1 == "reauthentication required" {
                    reauth.set(true);
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

fn render_section<'a>(cx: &'a Cx, account: i64) -> BoxView<'a> {
    let state_cx = cx.keyed((account, "tools"));
    let state = ToolsState {
        revision: signal(&state_cx, || 0_usize),
        id: signal(&state_cx, String::new),
        name: signal(&state_cx, String::new),
        password: signal(&state_cx, String::new),
        reauth: signal(&state_cx, || false),
        busy: signal(&state_cx, || false),
        key: signal(&state_cx, String::new),
        error: signal(&state_cx, String::new),
        bot_busy: signal(&state_cx, || 0_i64),
        bot_error: signal(&state_cx, String::new),
    };
    let cards = TOOL_TEMPLATES
        .map(|template| template_card(cx, template, state.id.clone(), state.name.clone()));
    let connect_button_attrs = connect_attrs(cx, account, &state);
    let tool_id = state.id.clone();
    let tool_name = state.name.clone();
    let password = state.password.clone();
    let reauth = state.reauth.clone();
    let busy = state.busy.clone();
    let key = state.key.clone();
    let error = state.error.clone();
    let bot_error = state.bot_error.clone();
    let list_revision = state.revision.clone();
    let list_tool_id = state.id.clone();
    let list_tool_name = state.name.clone();
    let list_bot_busy = state.bot_busy.clone();
    let mut action_mount = delegated_bot_actions(cx, account, &state);
    action_mount.insert(cx, "data-native-tools-actions", "");
    let connections = view! {
        cx =>
        native_settings_connections(
            account: account,
            revision: list_revision,
            tool_id: list_tool_id,
            tool_name: list_tool_name,
            bot_busy: list_bot_busy
        )
    }
    .boxed();
    view! {
        cx =>
        <section class="mt-10" (action_mount)>
            <h2
                class="mb-1 flex items-center gap-2 text-[1rem] font-semibold text-[var(--text)]"
            >
                (ui_icon(cx, UiIcon::Plug, 16))
                "Connected tools"
            </h2>
            <p class="mb-5 text-body leading-relaxed text-[var(--text-muted)]">
                "Link an AI coding tool to Lific over MCP. Each connection mints a bot identity that acts on your behalf; disconnect any time."
            </p>
            <div class="mb-4 grid gap-2 sm:grid-cols-2">
                for card in cards {
                    (card)
                }
            </div>
            (connections)
            <p class="mt-3 text-caption text-[var(--error)]" role="alert">
                $(bot_error.get())
            </p>
            <div class="mt-4 max-w-lg rounded-xl bg-[var(--surface)] p-4">
                <h3 class="mb-3 text-body font-semibold text-[var(--text)]">
                    "Add custom or named connection"
                </h3>
                <label class="mb-3 block text-body-sm">
                    "Connection ID"
                    <input
                        class=(super::INPUT)
                        maxlength="48"
                        placeholder="codex-laptop"
                        :value=$(tool_id.get())
                        @input=$(|event: Event| tool_id.set(
                                event.target.value.to_owned(),
                            ))
                    />
                </label>
                <label class="mb-3 block text-body-sm">
                    "Display name"
                    <input
                        class=(super::INPUT)
                        maxlength="80"
                        placeholder="Codex on my laptop"
                        :value=$(tool_name.get())
                        @input=$(|event: Event| tool_name.set(
                                event.target.value.to_owned(),
                            ))
                    />
                </label>
                <button
                    type="button"
                    class=(format!(
                        "{} bg-[var(--btn-success)] text-[var(--btn-success-text)] hover:bg-[var(--btn-success-hover)]",
                        super::BUTTON,
                    ))
                    (connect_button_attrs)
                >
                    $(if busy.get() { "Connecting…" } else { "Connect agent" })
                </button>
                <label class="mt-3 block text-body-sm" :hidden=$(!reauth.get())>
                    "Current password"
                    <input
                        type="password"
                        autocomplete="current-password"
                        class=(super::INPUT)
                        :value=$(password.get())
                        @input=$(|event: Event| password.set(
                                event.target.value.to_owned(),
                            ))
                    />
                </label>
                <p class="mt-2 text-caption text-[var(--error)]" role="alert">
                    $(error.get())
                </p>
                <div
                    class="mt-3 rounded-lg border border-[var(--btn-success)] p-3"
                    :hidden=$(key.get().is_empty())
                >
                    <p class="mb-1 text-body-sm font-medium text-[var(--text)]">
                        "Copy your API key now"
                    </p>
                    <code
                        class="block break-all font-mono text-caption text-[var(--text)]"
                    >
                        $(key.get())
                    </code>
                    <p class="mt-1 text-caption text-[var(--text-muted)]">
                        "The key is shown only this once. Configure your MCP client to use the Lific MCP URL and this key as a Bearer token."
                    </p>
                </div>
            </div>
        </section>
    }.boxed()
}

#[shard("/__native_settings/connections")]
async fn native_settings_connections(
    cx: &Cx,
    account: i64,
    revision: Signal<usize>,
    tool_id: Signal<String>,
    tool_name: Signal<String>,
    bot_busy: Signal<i64>,
) -> topcoat::Result<impl View> {
    let _ = revision.get();
    let _caller = session::read(cx, super::actions::same_account(cx, account))?;
    let bots = session::read(
        cx,
        (|| {
            let conn = context::db(cx).read()?;
            crate::db::queries::users::list_bots(&conn, account)
        })(),
    )?;
    let cards = bots
        .into_iter()
        .map(|bot| connection_card(cx, bot, &tool_id, &tool_name, &bot_busy))
        .collect::<Vec<_>>();
    Ok(view! {
        cx =>
        <div class="grid gap-2.5 sm:grid-cols-2" data-settings-connections-list="">
            for card in cards {
                (card)
            }
        </div>
    })
}

fn connection_card<'a>(
    cx: &'a Cx,
    bot: Bot,
    tool_id: &Signal<String>,
    tool_name: &Signal<String>,
    bot_busy: &Signal<i64>,
) -> BoxView<'a> {
    let bot_id = bot.id;
    let tool_id = Signal::clone(tool_id);
    let tool_name = Signal::clone(tool_name);
    let bot_busy = Signal::clone(bot_busy);
    view! {
        cx =>
        <div
            class="flex flex-wrap items-center gap-3.5 rounded-xl bg-[var(--surface)] p-3.5 shadow-[0_1px_2px_rgba(0,0,0,0.06)]"
            data-connection-id=(bot
                .tool_id
                .clone()
                .unwrap_or_else(|| bot.username.clone()))
        >
            <div
                class="grid size-10 shrink-0 place-items-center rounded-lg bg-[var(--bg-subtle)] text-[var(--text)]"
            >
                (ui_icon(cx, UiIcon::OpenExternal, 17))
            </div>
            <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-2">
                    <span class="break-all text-body font-medium text-[var(--text)]">
                        (bot.display_name.clone())
                    </span>
                    if bot.connected {
                        <span
                            class="rounded-full bg-[var(--success-bg)] px-1.5 py-0.5 text-micro font-semibold uppercase tracking-wide text-[var(--success)]"
                        >
                            "● Connected"
                        </span>
                    } else {
                        <span
                            class="rounded-full px-1.5 py-0.5 text-micro font-semibold uppercase tracking-wide text-[var(--warn)]"
                        >
                            "Disconnected"
                        </span>
                    }
                </div>
                <p class="mt-0.5 truncate text-caption text-[var(--text-muted)]">
                    (bot.tool_id.clone().unwrap_or_else(|| bot.username.clone()))
                </p>
            </div>
            <div class="flex shrink-0 items-center gap-1.5">
                if bot.connected {
                    <button
                        type="button"
                        class=(format!(
                            "{} text-[var(--text-muted)] hover:text-[var(--error)]",
                            super::BUTTON,
                        ))
                        data-native-bot-action=(bot_action_wire(bot.id, false))
                        :disabled=$(bot_busy.get() == bot_id)
                    >
                        "Disconnect"
                    </button>
                } else {
                    <button
                        type="button"
                        class=(format!(
                            "{} text-[var(--text-faint)] hover:text-[var(--error)]",
                            super::BUTTON,
                        ))
                        data-native-bot-action=(bot_action_wire(bot.id, true))
                        :disabled=$(bot_busy.get() == bot_id)
                    >
                        "Remove"
                    </button>
                    <button
                        type="button"
                        class=(format!(
                            "{} bg-[var(--btn-success)] text-[var(--btn-success-text)] hover:bg-[var(--btn-success-hover)]",
                            super::BUTTON,
                        ))
                        (select_tool_attrs(
                            cx,
                            tool_id.clone(),
                            tool_name.clone(),
                            bot.tool_id.clone().unwrap_or_else(|| bot.username.clone()),
                            bot.display_name.clone(),
                        ))
                    >
                        "Reconnect"
                    </button>
                }
            </div>
        </div>
    }
    .boxed()
}
