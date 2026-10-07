use super::super::{
    context,
    icons::{UiIcon, ui_icon},
    session,
};
use super::{
    actions::bot_action,
    templates::{GENERIC_TEMPLATE, TOOL_TEMPLATES, ToolTemplate},
    tool_dialog::{self, ToolDialogState},
};
use crate::db::models::Bot;
use topcoat::{
    context::Cx,
    runtime::{BoolSurrogate, Event, I64Surrogate, Signal, Surrogated, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};

pub(super) fn section(cx: &Cx, account: i64, revision: Signal<usize>) -> BoxView<'_> {
    view! { cx => native_settings_tools(account: account, revision: revision) }.boxed()
}

#[shard("/__native_settings/tools")]
async fn native_settings_tools(
    cx: &Cx,
    account: i64,
    revision: Signal<usize>,
) -> topcoat::Result<impl View> {
    let _caller = session::read(cx, super::actions::same_account(cx, account))?;
    Ok(render_section(cx, account, revision))
}

#[derive(Clone)]
struct ToolsState {
    revision: Signal<usize>,
    dialog: ToolDialogState,
    custom_tool: Signal<String>,
    custom_name: Signal<String>,
    bot_busy: Signal<i64>,
    bot_error: Signal<String>,
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
            if(button&&${_event}.inner.currentTarget.contains(button)){try{const args=JSON.parse(button.getAttribute('data-native-bot-action'));
            ${_dispatch}(cx.hydrate(args[0]),cx.hydrate(args[1]));}catch{}}"#,
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

fn dispatch_connect_attrs(
    cx: &Cx,
    identity: (String, String),
    template: String,
    dialog: &ToolDialogState,
) -> Attributes {
    let busy = dialog.busy.clone();
    let tool = dialog.tool.clone();
    let name = dialog.name.clone();
    let setup_template = dialog.setup_template.clone();
    let handler = expr!(|_event: Event| {
        if !busy.get() {
            tool.set(identity.0.clone());
            name.set(identity.1.clone());
            setup_template.set(template.clone());
            raw!(
                "document.querySelector('[data-native-tool-launch]')?.click()",
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

fn bot_action_wire(id: i64, remove: bool) -> String {
    serde_json::to_string(&(id, remove).into_surrogate())
        .expect("bot actions contain scalar surrogate arguments")
}

fn render_section<'a>(cx: &'a Cx, account: i64, revision: Signal<usize>) -> BoxView<'a> {
    let owner = cx.keyed((account, "tools"));
    let state = ToolsState {
        revision: revision.clone(),
        dialog: ToolDialogState::new(&owner, revision.clone()),
        custom_tool: signal(&owner, String::new),
        custom_name: signal(&owner, String::new),
        bot_busy: signal(&owner, || 0_i64),
        bot_error: signal(&owner, String::new),
    };
    let custom = tool_dialog::custom_trigger(
        cx,
        account,
        &state.dialog,
        state.custom_tool.clone(),
        state.custom_name.clone(),
    );
    let launch = tool_dialog::connect_attrs(
        cx,
        account,
        None,
        Some((state.dialog.tool.clone(), state.dialog.name.clone())),
        None,
        &state.dialog,
    );
    let custom_id = state.custom_tool.clone();
    let custom_name = state.custom_name.clone();
    let dialog_open = state.dialog.open.clone();
    let mut action_mount = delegated_bot_actions(cx, account, &state);
    action_mount.insert(cx, "data-native-tools-actions", "");
    let connections_revision = state.revision.clone();
    let connections_busy = state.bot_busy.clone();
    let connections_dialog = state.dialog.clone();
    let connections = view! {
        cx =>
        native_settings_connections(
            account: account,
            revision: connections_revision,
            bot_busy: connections_busy,
            dialog: connections_dialog
        )
    }
    .boxed();
    let dialog = tool_dialog::view(cx, account, &state.dialog);
    let bot_error = state.bot_error.clone();
    view! {
        cx =>
        <section class="mt-10" (action_mount)>
            <div :inert=$(dialog_open.get())>
                <h2
                    class="mb-1 flex items-center gap-2 text-[1rem] font-semibold text-[var(--text)]"
                >
                    (ui_icon(cx, UiIcon::Plug, 16))
                    "Connected tools"
                </h2>
                <p class="mb-5 text-body leading-relaxed text-[var(--text-muted)]">
                    "Link an AI coding tool to Lific over MCP. Each connection mints a bot identity that acts on your behalf; disconnect any time."
                </p>
                (connections)
                <p class="mt-3 text-caption text-[var(--error)]" role="alert">
                    $(bot_error.get())
                </p>
                <div class="mt-4 max-w-lg rounded-xl bg-[var(--surface)] p-4">
                    <h3 class="mb-3 text-body font-semibold text-[var(--text)]">
                        "Add custom or named connection"
                    </h3>
                    <p
                        class="mb-3 text-caption leading-relaxed text-[var(--text-muted)]"
                    >
                        "Give each agent or machine its own connection ID. Disconnecting one leaves the others connected."
                    </p>
                    <label class="mb-3 block text-body-sm">
                        "Connection ID"
                        <input
                            class=(super::INPUT)
                            maxlength="48"
                            placeholder="codex-laptop"
                            data-native-tool-custom-id=""
                            :value=$(custom_id.get())
                            @input=$(|event: Event| custom_id.set(
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
                            data-native-tool-custom-name=""
                            :value=$(custom_name.get())
                            @input=$(|event: Event| custom_name.set(
                                    event.target.value.to_owned(),
                                ))
                        />
                    </label>
                    (custom)
                </div>
            </div>
            <button type="button" class="hidden" data-native-tool-launch="" (launch)>
                "Connect"
            </button>
            (dialog)
        </section>
    }.boxed()
}

#[shard("/__native_settings/connections")]
async fn native_settings_connections(
    cx: &Cx,
    account: i64,
    revision: Signal<usize>,
    bot_busy: Signal<i64>,
    dialog: ToolDialogState,
) -> topcoat::Result<impl View> {
    let _ = revision.get();
    let _caller = session::read(cx, super::actions::same_account(cx, account))?;
    let bots = session::read(cx, {
        let conn = context::db(cx).read()?;
        crate::db::queries::users::list_bots(&conn, account)
    })?;
    let mut cards = TOOL_TEMPLATES
        .iter()
        .map(|template| {
            let bot = bots
                .iter()
                .find(|bot| connection_id(bot) == template.id)
                .cloned();
            template_connection_card(cx, *template, bot, bot_busy.clone(), &dialog)
        })
        .collect::<Vec<_>>();
    cards.extend(
        bots.into_iter()
            .filter(|bot| {
                !TOOL_TEMPLATES
                    .iter()
                    .any(|template| connection_id(bot) == template.id)
            })
            .map(|bot| connection_card(cx, bot, bot_busy.clone(), &dialog)),
    );
    Ok(view! {
        cx =>
        <div class="mb-4 grid gap-2.5 sm:grid-cols-2" data-settings-connections-list="">
            for card in cards {
                (card)
            }
        </div>
    })
}

fn connection_id(bot: &Bot) -> &str {
    bot.tool_id.as_deref().unwrap_or_else(|| {
        TOOL_TEMPLATES
            .iter()
            .find(|template| bot.username.starts_with(&format!("{}-", template.id)))
            .map_or(&bot.username, |template| template.id)
    })
}

fn template_connection_card<'a>(
    cx: &'a Cx,
    template: ToolTemplate,
    bot: Option<Bot>,
    bot_busy: Signal<i64>,
    dialog: &ToolDialogState,
) -> BoxView<'a> {
    let mut connect = dispatch_connect_attrs(
        cx,
        (template.id.to_owned(), template.name.to_owned()),
        template.id.to_owned(),
        dialog,
    );
    connect.insert(cx, "data-native-tool-connect", template.id);
    let bot_id = bot.as_ref().map(|bot| bot.id).unwrap_or_default();
    let name = bot
        .as_ref()
        .map(|bot| bot.display_name.clone())
        .unwrap_or_else(|| template.name.to_owned());
    let detail = bot
        .as_ref()
        .map(|bot| connection_id(bot).to_owned())
        .unwrap_or_else(|| template.description.to_owned());
    let reconnect = bot.as_ref().map(|bot| {
        let id = connection_id(bot).to_owned();
        let template_id = TOOL_TEMPLATES
            .iter()
            .find(|candidate| id == candidate.id || id.starts_with(&format!("{}-", candidate.id)))
            .map_or(GENERIC_TEMPLATE.id, |candidate| candidate.id);
        let wire =
            serde_json::to_string(&(id, bot.display_name.clone(), template_id).into_surrogate())
                .expect("reconnect identity contains strings");
        (wire, template_id)
    });
    let mut reconnect_attrs = Attributes::with_capacity(4);
    if let (Some(bot), Some((wire, template_id))) = (bot.as_ref(), reconnect.as_ref()) {
        reconnect_attrs = dispatch_connect_attrs(
            cx,
            (connection_id(bot).to_owned(), bot.display_name.clone()),
            (*template_id).to_owned(),
            dialog,
        );
        reconnect_attrs.insert(cx, "data-native-tool-reconnect", wire);
        reconnect_attrs.insert(cx, "data-native-tool-reconnect-template", *template_id);
    }
    view! {
        cx =>
        <div
            class="flex min-w-0 flex-wrap items-center gap-3.5 rounded-xl bg-[var(--surface)] p-3.5 shadow-[0_1px_2px_rgba(0,0,0,0.06)]"
            data-settings-tool-template=(template.id)
            data-connection-id=(template.id)
        >
            <div
                class="grid size-10 shrink-0 place-items-center rounded-lg bg-[var(--bg-subtle)] text-[var(--text)]"
            >
                (ui_icon(cx, UiIcon::OpenExternal, 17))
            </div>
            <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-2">
                    <span class="break-all text-body font-medium text-[var(--text)]">
                        (name)
                    </span>
                    if let Some(bot) = bot.as_ref() {
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
                    }
                </div>
                <p class="mt-0.5 truncate text-caption text-[var(--text-muted)]">
                    (detail)
                </p>
            </div>
            <div class="flex shrink-0 items-center gap-1.5">
                if let Some(bot) = bot.as_ref() {
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
                            (reconnect_attrs)
                        >
                            "Reconnect"
                        </button>
                    }
                } else {
                    <button
                        type="button"
                        class=(format!(
                            "{} bg-[var(--btn-success)] text-[var(--btn-success-text)] hover:bg-[var(--btn-success-hover)]",
                            super::BUTTON,
                        ))
                        (connect)
                    >
                        "Connect"
                    </button>
                }
            </div>
        </div>
    }.boxed()
}

fn connection_card<'a>(
    cx: &'a Cx,
    bot: Bot,
    bot_busy: Signal<i64>,
    dialog: &ToolDialogState,
) -> BoxView<'a> {
    let bot_id = bot.id;
    let tool_id = connection_id(&bot).to_owned();
    let name = bot.display_name.clone();
    let template = TOOL_TEMPLATES
        .iter()
        .find(|template| {
            tool_id == template.id || tool_id.starts_with(&format!("{}-", template.id))
        })
        .map_or(GENERIC_TEMPLATE.id, |template| template.id);
    let reconnect =
        serde_json::to_string(&(tool_id.clone(), name.clone(), template).into_surrogate())
            .expect("reconnect identity contains strings");
    let mut reconnect_attrs = dispatch_connect_attrs(
        cx,
        (tool_id.clone(), name.clone()),
        template.to_owned(),
        dialog,
    );
    reconnect_attrs.insert(cx, "data-native-tool-reconnect", reconnect);
    reconnect_attrs.insert(cx, "data-native-tool-reconnect-template", template);
    view! {
        cx =>
        <div
            class="flex flex-wrap items-center gap-3.5 rounded-xl bg-[var(--surface)] p-3.5 shadow-[0_1px_2px_rgba(0,0,0,0.06)]"
            data-connection-id=(tool_id.clone())
        >
            <div
                class="grid size-10 shrink-0 place-items-center rounded-lg bg-[var(--bg-subtle)] text-[var(--text)]"
            >
                (ui_icon(cx, UiIcon::OpenExternal, 17))
            </div>
            <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-2">
                    <span class="break-all text-body font-medium text-[var(--text)]">
                        (name)
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
                    (tool_id.clone())
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
                        (reconnect_attrs)
                    >
                        "Reconnect"
                    </button>
                }
            </div>
        </div>
    }.boxed()
}
