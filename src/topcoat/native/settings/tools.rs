use super::super::{
    context,
    icons::{UiIcon, ui_icon},
    session,
};
use super::{
    actions::bot_action,
    templates::{GENERIC_TEMPLATE, TOOL_TEMPLATES},
    tool_dialog::{self, ToolDialogState},
};
use crate::db::models::Bot;
use topcoat::{
    context::Cx,
    runtime::{
        BoolSurrogate, Event, I64Surrogate, Signal, StringSurrogate, Surrogated, expr, shard,
        signal,
    },
    view::{Attributes, BoxView, View, ViewExt, view},
};

pub(super) fn section(cx: &Cx, account: i64) -> BoxView<'_> {
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
    let reconnect_tool = state.custom_tool.clone();
    let reconnect_name = state.custom_name.clone();
    let reconnect_template = state.dialog.setup_template.clone();
    let handler = expr!(|_event: Event| {
        let _reconnect = |tool: StringSurrogate,
                          name: StringSurrogate,
                          template: StringSurrogate| {
            reconnect_tool.set(tool.to_owned());
            reconnect_name.set(name.to_owned());
            reconnect_template.set(template.to_owned());
            raw!(
                "${_event}.inner.currentTarget.querySelector('[data-native-tool-reconnect-trigger]')?.click()",
                ()
            );
        };
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
            const reconnect=target?.closest('button[data-native-tool-reconnect]');
            if(reconnect&&${_event}.inner.currentTarget.contains(reconnect)){try{const args=JSON.parse(reconnect.getAttribute('data-native-tool-reconnect'));
            ${_reconnect}(cx.hydrate(args[0]),cx.hydrate(args[1]),cx.hydrate(args[2]));}catch{}}
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

fn bot_action_wire(id: i64, remove: bool) -> String {
    serde_json::to_string(&(id, remove).into_surrogate())
        .expect("bot actions contain scalar surrogate arguments")
}

fn render_section<'a>(cx: &'a Cx, account: i64) -> BoxView<'a> {
    let owner = cx.keyed((account, "tools"));
    let revision = signal(&owner, || 0_usize);
    let state = ToolsState {
        revision: revision.clone(),
        dialog: ToolDialogState::new(&owner, revision),
        custom_tool: signal(&owner, String::new),
        custom_name: signal(&owner, String::new),
        bot_busy: signal(&owner, || 0_i64),
        bot_error: signal(&owner, String::new),
    };
    let cards = TOOL_TEMPLATES
        .map(|template| tool_dialog::template_card(cx, account, template, &state.dialog));
    let custom = tool_dialog::custom_trigger(
        cx,
        account,
        &state.dialog,
        state.custom_tool.clone(),
        state.custom_name.clone(),
        false,
    );
    let reconnect_trigger = tool_dialog::custom_trigger(
        cx,
        account,
        &state.dialog,
        state.custom_tool.clone(),
        state.custom_name.clone(),
        true,
    );
    let custom_id = state.custom_tool.clone();
    let custom_name = state.custom_name.clone();
    let dialog_open = state.dialog.open.clone();
    let mut action_mount = delegated_bot_actions(cx, account, &state);
    action_mount.insert(cx, "data-native-tools-actions", "");
    let connections = view! {
        cx =>
        native_settings_connections(
            account: account,
            revision: state.revision.clone(),
            bot_busy: state.bot_busy.clone()
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
            (reconnect_trigger)
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
) -> topcoat::Result<impl View> {
    let _ = revision.get();
    let _caller = session::read(cx, super::actions::same_account(cx, account))?;
    let bots = session::read(cx, {
        let conn = context::db(cx).read()?;
        crate::db::queries::users::list_bots(&conn, account)
    })?;
    let cards = bots
        .into_iter()
        .map(|bot| connection_card(cx, bot, bot_busy.clone()))
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

fn connection_card<'a>(cx: &'a Cx, bot: Bot, bot_busy: Signal<i64>) -> BoxView<'a> {
    let bot_id = bot.id;
    let tool_id = bot.tool_id.clone().unwrap_or_else(|| bot.username.clone());
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
                        data-native-tool-reconnect=(reconnect)
                        data-native-tool-reconnect-template=(template)
                    >
                        "Reconnect"
                    </button>
                }
            </div>
        </div>
    }.boxed()
}
