//! Native IssueDetail label catalog and sparse attachment writes.

use super::super::super::runtime::whitespace::trim_ecmascript;
use super::super::{browser, context, session};
use super::module_assignment::ModuleAssignmentSnapshot;
use crate::{db::models::CreateLabel, error::LificError, realtime::RealtimeHub, services};
use topcoat::{
    context::{Cx, app_context},
    runtime::{Event, Signal, expr, procedure, record, shard},
    view::{Attributes, BoxView, ViewExt, view},
};

#[record]
#[derive(Clone, Default)]
pub(crate) struct LabelRequest {
    pub mode: String,
    pub account_id: i64,
    pub issue_id: i64,
    pub identifier: String,
    pub name: String,
    pub color: String,
}

pub(crate) type LabelReplyValue = <LabelReply as topcoat::runtime::Surrogated>::Surrogate;
pub(crate) type LabelRequestValue = <LabelRequest as topcoat::runtime::Surrogated>::Surrogate;

pub(crate) type PickerState = (
    Signal<bool>,
    Signal<String>,
    Signal<String>,
    Signal<String>,
    Signal<bool>,
    Signal<String>,
    Signal<bool>,
    Signal<bool>,
);

pub(crate) struct PickerProps<'a> {
    pub account_id: i64,
    pub issue_id: i64,
    pub identifier: &'a str,
    pub catalog: &'a [crate::db::models::Label],
    pub attached: &'a [String],
    pub can_edit: bool,
    pub menus: (Signal<bool>, Signal<bool>, Signal<bool>, Signal<bool>),
    pub state: PickerState,
}

pub(crate) fn picker<'a>(cx: &'a Cx, props: PickerProps<'_>) -> BoxView<'a> {
    let PickerProps {
        account_id,
        issue_id,
        identifier,
        catalog,
        attached,
        can_edit,
        menus,
        state,
    } = props;
    let (open, query, color, error, color_open, hex_draft, hex_bad, creating) = state;
    let catalog_items = catalog
        .iter()
        .map(|label| (label.name.clone(), label.color.clone()))
        .collect::<Vec<_>>();
    let chips = attached
        .iter()
        .map(|name| {
            (
                name.clone(),
                catalog
                    .iter()
                    .find(|label| label.name == *name)
                    .map(|label| label.color.clone()),
            )
        })
        .collect::<Vec<_>>();
    let identity = (account_id, issue_id, identifier.to_owned());
    let toggle_open = open.clone();
    let status_open = menus.0;
    let header_open = menus.1;
    let priority_open = menus.2;
    let module_open = menus.3;
    let reset_query = query.clone();
    let reset_color = color.clone();
    let reset_error = error.clone();
    let reset_hex = hex_draft.clone();
    let reset_bad = hex_bad.clone();
    let reset_color_open = color_open.clone();
    let input_id = format!("native-issue-label-filter-{}", identifier);
    let browser = browser::bindings();
    let focus_open = open.clone();
    let open_handler = expr!(|event: Event| {
        if !browser.is_disposed() {
            event.prevent_default();
            event.stop_propagation();
            if !toggle_open.get() {
                status_open.set(false);
                header_open.set(false);
                priority_open.set(false);
                module_open.set(false);
                reset_query.set("".to_owned());
                reset_color.set("".to_owned());
                reset_error.set("".to_owned());
                reset_hex.set("".to_owned());
                reset_bad.set(false);
                reset_color_open.set(false);
            }
            toggle_open.set(!toggle_open.get());
            if toggle_open.get() {
                let _focus = || {
                    if !browser.is_disposed() {
                        if focus_open.get() {
                            raw!(
                                "document.getElementById(${input_id}.toString())?.focus()",
                                ()
                            );
                        }
                    }
                };
                raw!("requestAnimationFrame(()=>${_focus}());", ());
            }
        }
    });
    let mut open_attrs = Attributes::with_capacity(1);
    open_attrs.insert(
        cx,
        "data-topcoat-on:click",
        open_handler.into_evaluated_and_js().1,
    );
    let query_handle = query.clone();
    let query_handler = expr!(|event: Event| {
        if !browser.is_disposed() {
            query_handle.set(event.target.value.to_owned());
        }
    });
    let mut query_attrs = Attributes::with_capacity(1);
    query_attrs.insert(
        cx,
        "data-topcoat-on:input",
        query_handler.into_evaluated_and_js().1,
    );
    let picker_state = (
        open.clone(),
        query.clone(),
        color.clone(),
        error.clone(),
        color_open.clone(),
        hex_draft,
        hex_bad,
        creating,
    );
    let options_identity = identity.clone();
    let options_attached = attached.to_vec();
    let options_query = query.clone();
    let options = view! {
        cx =>
        native_label_options(
            identity: options_identity,
            data: (catalog_items, options_attached),
            filter: $(options_query.get()),
            selected_color_input: $(color.get()),
            state: picker_state.clone()
        )
    }
    .boxed();
    let chips_empty = chips.is_empty();
    let chip_views = chips
        .into_iter()
        .map(|(name, tint)| {
            let action = can_edit.then(|| remove_button(cx, identity.clone(), &name));
            super::super::label_chip::render_with_action(cx, name, tint.as_deref(), action)
        })
        .collect::<Vec<_>>();
    view! {
        cx =>
        <section class="relative space-y-2" data-native-issue-labels="">
            <div class="flex flex-wrap items-center gap-1.5">
                if chips_empty {
                    <span class="native-issue-detail__empty-value">"None"</span>
                }
                for chip in chip_views {
                    (chip)
                }
                if can_edit {
                    <button
                        type="button"
                        title="Add label"
                        aria-label="Add label"
                        class="touch-target flex size-5 items-center justify-center rounded border border-dashed border-[var(--border)] text-[var(--text-faint)] transition-colors hover:border-[var(--accent)] hover:text-[var(--accent)]"
                        (open_attrs)
                    >
                        "+"
                    </button>
                }
            </div>
            if can_edit {
                <div
                    class="absolute left-0 top-full z-20 mt-1 w-[240px] max-w-[calc(100vw-2rem)] rounded-md border border-[var(--border)] bg-[var(--surface)] py-1 shadow-lg"
                    :hidden=$(!open.get())
                    data-native-issue-label-picker=""
                    @click=$(|event: Event| event.stop_propagation())
                    @keydown=$(|event: Event| event.stop_propagation())
                >
                    <div class="px-2 pt-1 pb-1.5">
                        <input
                            id=(input_id)
                            type="text"
                            placeholder="Filter or create…"
                            :value=$(query.get())
                            class="w-full rounded border border-[var(--border)] bg-[var(--bg)] px-2 py-1 text-sm text-[var(--text)] outline-none placeholder:text-[var(--text-faint)] focus:border-[var(--accent)]"
                            @keydown=$(|event: Event| {
                                if !browser.is_disposed() {
                                    if event.key == "Enter" {
                                        event.prevent_default();
                                        raw!(
                                            "${event}.inner.target?.closest('[data-native-issue-label-picker]')?.querySelector('[data-native-label-enter=\"true\"]')?.click()",
                                            (),
                                        );
                                    } else if event.key == "Escape" {
                                        event.prevent_default();
                                        event.stop_propagation();
                                        open.set(false);
                                        color_open.set(false);
                                    }
                                }
                            })
                            (query_attrs)
                        />
                    </div>
                    (options)
                    <p
                        class="text-xs text-[var(--danger)]"
                        :hidden=$(error.get().is_empty())
                    >
                        $(error.get())
                    </p>
                </div>
            }
        </section>
    }.boxed()
}

#[shard("/__native_issue_edit/label_options")]
async fn native_label_options(
    cx: &Cx,
    identity: (i64, i64, String),
    data: (Vec<(String, String)>, Vec<String>),
    filter: String,
    selected_color_input: String,
    state: PickerState,
) -> topcoat::Result<impl topcoat::view::View> {
    let query_snapshot = filter.clone();
    let base_name = trim_ecmascript(&filter).to_owned();
    let filter = base_name.to_lowercase();
    let catalog = data.0;
    let attached = data.1;
    let filtered = catalog
        .iter()
        .filter(|(name, _)| name.to_lowercase().contains(&filter))
        .collect::<Vec<_>>();
    let exact = catalog
        .iter()
        .any(|(name, _)| name.to_lowercase() == filter && !filter.is_empty());
    let can_create = !filter.is_empty() && !exact;
    let catalog_empty = catalog.is_empty();
    let filtered_empty = filtered.is_empty();
    let (_open, query, picked_color, error, color_open, hex_draft, hex_bad, creating) = state;
    let option_views = filtered
        .iter()
        .map(|(name, color)| {
            option(
                cx,
                identity.clone(),
                &attached,
                name,
                color,
                !can_create && filtered.len() == 1,
                (query.clone(), query_snapshot.clone()),
            )
        })
        .collect::<Vec<_>>();
    let default_color =
        super::super::project_overview::labels_model::color_for_name(if base_name.is_empty() {
            "label"
        } else {
            &base_name
        })
        .to_owned();
    let palette = super::super::project_overview::labels_model::PALETTE.to_vec();
    let selected_color = if selected_color_input.is_empty() {
        default_color.clone()
    } else {
        super::super::project_overview::labels_model::normalize_hex(&selected_color_input)
            .unwrap_or_else(|| default_color.clone())
    };
    let palette_views = palette
        .iter()
        .map(|(name, hex)| {
            palette_button(
                cx,
                name,
                hex,
                &picked_color,
                &color_open,
                hex.eq_ignore_ascii_case(&selected_color),
            )
        })
        .collect::<Vec<_>>();
    let color_name = super::super::project_overview::labels_model::color_name(&selected_color);
    let create_text = format!("Create “{base_name}”");
    let create_request = LabelRequest {
        mode: "create".into(),
        account_id: identity.0,
        issue_id: identity.1,
        identifier: identity.2.clone(),
        name: base_name,
        color: selected_color.clone(),
    };
    let create_error = error;
    let create_busy = creating.clone();
    let create_color = picked_color.clone();
    let browser = browser::bindings();
    let create = expr!(|event: Event| {
        if !browser.is_disposed() {
            if !create_busy.get() {
                if query.get() == query_snapshot {
                    event.prevent_default();
                    let picked = create_color.get();
                    let request_color = if picked.is_empty() {
                        default_color.clone()
                    } else {
                        picked
                    };
                    let _request = LabelRequest {
                        mode: "create".to_owned(),
                        account_id: create_request.account_id.clone(),
                        issue_id: create_request.issue_id.clone(),
                        identifier: create_request.identifier.clone(),
                        name: create_request.name.clone(),
                        color: request_color,
                    };
                    let accepted = raw!(
                        "cx.hydrate(!window.dispatchEvent(new CustomEvent('lific:native-issue-label-request',{detail:${_request},cancelable:true})))",
                        false
                    );
                    if accepted {
                        create_error.set("".to_owned());
                        create_busy.set(true);
                    }
                }
            }
        }
    });
    let mut create_attrs = Attributes::with_capacity(1);
    create_attrs.insert(
        cx,
        "data-topcoat-on:click",
        create.into_evaluated_and_js().1,
    );
    let color_trigger = color_open.clone();
    let color_hex_draft = hex_draft.clone();
    let color_hex_bad = hex_bad.clone();
    let hex_initial = selected_color.trim_start_matches('#').to_owned();
    let color_menu_trigger = expr!(|event: Event| {
        if !browser.is_disposed() {
            event.prevent_default();
            event.stop_propagation();
            color_trigger.set(!color_trigger.get());
            color_hex_draft.set(hex_initial.clone());
            color_hex_bad.set(false);
        }
    });
    let mut color_trigger_attrs = Attributes::with_capacity(1);
    color_trigger_attrs.insert(
        cx,
        "data-topcoat-on:click",
        color_menu_trigger.into_evaluated_and_js().1,
    );
    let hex = hex_draft.clone();
    let input_bad = hex_bad.clone();
    let input_hex = expr!(|event: Event| {
        if !browser.is_disposed() {
            hex.set(event.target.value.to_owned());
            input_bad.set(false);
        }
    });
    let mut hex_attrs = Attributes::with_capacity(1);
    hex_attrs.insert(
        cx,
        "data-topcoat-on:input",
        input_hex.into_evaluated_and_js().1,
    );
    Ok(view! {
        cx =>
        <div>
            <div
                class="max-h-[220px] overflow-y-auto"
                role="listbox"
                aria-label="Choose labels"
            >
                if catalog_empty && !can_create {
                    <p class="px-2 py-1 text-xs text-[var(--text-faint)]">
                        "No labels defined"
                    </p>
                }
                for option in option_views {
                    (option)
                }
                if !catalog_empty && !filter.is_empty() && filtered_empty && !can_create {
                    <p class="px-2 py-1 text-xs text-[var(--text-faint)]">"No match"</p>
                }
            </div>
            if can_create {
                <div
                    class="mt-1 flex items-center gap-2 border-t border-[var(--border)] px-2.5 pt-2 pb-2"
                >
                    <div
                        class="relative inline-flex"
                        data-native-label-color-area=""
                        @keydown=$(|event: Event| {
                            if !browser.is_disposed() {
                                if event.key == "Escape" {
                                    event.prevent_default();
                                    event.stop_propagation();
                                    color_open.set(false);
                                }
                            }
                        })
                    >
                        <button
                            type="button"
                            class="size-5 shrink-0 rounded-full border border-black/10 shadow-sm transition-transform hover:scale-105 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] dark:border-white/15"
                            style=(format!("background: {selected_color}"))
                            title=(format!("{} · {}", color_name, selected_color))
                            aria-label=(format!("Color: {color_name}. Click to change."))
                            (color_trigger_attrs)
                        ></button>

                        <div
                            :hidden=$(!color_open.get())
                            class="absolute top-full left-0 z-40 mt-1.5 w-[208px] rounded-lg border border-[var(--border)] bg-[var(--surface)] p-2.5 shadow-lg"
                            @click=$(|event: Event| event.stop_propagation())
                            @keydown=$(|event: Event| event.stop_propagation())
                        >
                            <div class="mb-2.5 grid grid-cols-6 gap-1.5">
                                for color_button in palette_views {
                                    (color_button)
                                }
                            </div>
                            <div class="flex items-center gap-1.5">
                                <span class="font-mono text-sm text-[var(--text-faint)]">
                                    "#"
                                </span>
                                <input
                                    type="text"
                                    placeholder="hex"
                                    maxlength="7"
                                    spellcheck="false"
                                    @keydown=$(|event: Event| {
                                        if !browser.is_disposed() {
                                            if event.key == "Enter" {
                                                event.prevent_default();
                                                raw!(
                                                    "${event}.inner.target?.closest('[data-native-label-color-area]')?.querySelector('[data-native-label-hex-set]')?.click()",
                                                    (),
                                                );
                                            } else if event.key == "Escape" {
                                                event.prevent_default();
                                                event.stop_propagation();
                                                color_open.set(false);
                                            }
                                        }
                                    })
                                    :aria-invalid=$(if hex_bad.get() {
                                        "true"
                                    } else {
                                        "false"
                                    })
                                    :class=$(if hex_bad.get() {
                                        "min-w-0 flex-1 rounded border border-[var(--error)] bg-[var(--bg)] px-1.5 py-1 font-mono text-sm text-[var(--text)] outline-none focus:border-[var(--accent)]"
                                    } else {
                                        "min-w-0 flex-1 rounded border border-[var(--border)] bg-[var(--bg)] px-1.5 py-1 font-mono text-sm text-[var(--text)] outline-none focus:border-[var(--accent)]"
                                    })
                                    :value=$(hex_draft.get())
                                    (hex_attrs)
                                />
                                native_label_hex_actions(
                                    raw_hex: $(hex_draft.get()),
                                    state: (
                                        picked_color.clone(),
                                        hex_bad.clone(),
                                        color_open.clone(),
                                        hex_draft.clone(),
                                    )
                                )
                            </div>
                        </div>
                    </div>
                    <button
                        type="button"
                        :disabled=$(creating.get())
                        class="flex flex-1 items-center justify-center gap-1.5 rounded bg-[var(--btn-success)] px-2 py-1.5 text-sm font-medium text-[var(--btn-success-text)] transition-colors hover:bg-[var(--btn-success-hover)] disabled:opacity-50"
                        data-native-label-enter="true"
                        (create_attrs)
                    >
                        $(if creating.get() {
                            "Creating…".to_owned()
                        } else {
                            create_text.clone()
                        })
                    </button>
                </div>
            }
        </div>
    }
    .boxed())
}

#[shard("/__native_issue_edit/label_hex_actions")]
async fn native_label_hex_actions(
    cx: &Cx,
    raw_hex: String,
    state: (Signal<String>, Signal<bool>, Signal<bool>, Signal<String>),
) -> topcoat::Result<impl topcoat::view::View> {
    let normalized = super::super::project_overview::labels_model::normalize_hex(&raw_hex);
    let valid = normalized.is_some();
    let color = state.0.clone();
    let invalid = state.1.clone();
    let close = state.2.clone();
    let normalized_color = normalized.unwrap_or_default();
    let draft = state.3;
    let browser = browser::bindings();
    let apply = expr!(|event: Event| {
        if !browser.is_disposed() {
            if draft.get() == raw_hex {
                event.prevent_default();
                if valid {
                    color.set(normalized_color.clone());
                    invalid.set(false);
                    close.set(false);
                } else {
                    invalid.set(true);
                }
            }
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(cx, "data-topcoat-on:click", apply.into_evaluated_and_js().1);
    Ok(view! {
        cx =>
        <button
            type="button"
            data-native-label-hex-set=""
            class="rounded bg-[var(--bg-subtle)] px-2 py-1 text-sm font-medium text-[var(--text)] transition-colors hover:bg-[var(--border)]"
            (attrs)
        >
            "Set"
        </button>
    }.boxed())
}

fn option<'a>(
    cx: &'a Cx,
    identity: (i64, i64, String),
    attached: &[String],
    name: &str,
    color: &str,
    keyboard_default: bool,
    filter: (Signal<String>, String),
) -> BoxView<'a> {
    let selected = attached.iter().any(|label| label == name);
    let request = LabelRequest {
        mode: if selected { "remove" } else { "attach" }.into(),
        account_id: identity.0,
        issue_id: identity.1,
        identifier: identity.2,
        name: name.to_owned(),
        color: "".to_owned(),
    };
    let (query, query_snapshot) = filter;
    let browser = browser::bindings();
    let handler = expr!(|event: Event| {
        if !browser.is_disposed() {
            if query.get() == query_snapshot {
                event.prevent_default();
                raw!(
                    "window.dispatchEvent(new CustomEvent('lific:native-issue-label-request',{detail:${request},cancelable:true}))",
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
    let name = name.to_owned();
    let style = format!(
        "background-color: {}",
        super::super::project_overview::label_color(color)
    );
    view! {
        cx =>
        <button
            type="button"
            role="option"
            :aria-selected=$(if selected { "true" } else { "false" })
            data-label-name=(name.clone())
            data-native-label-enter=(if keyboard_default { "true" } else { "false" })
            class="flex w-full items-center gap-2 rounded px-2 py-1 text-left text-sm hover:bg-[var(--bg-subtle)]"
            (attrs)
        >
            <span class="size-2.5 rounded-full" style=(style)></span>
            <span>(name)</span>
            <span class="ml-auto" :hidden=$(!selected)>"✓"</span>
        </button>
    }.boxed()
}

fn remove_button<'a>(cx: &'a Cx, identity: (i64, i64, String), name: &str) -> BoxView<'a> {
    let request = LabelRequest {
        mode: "remove".into(),
        account_id: identity.0,
        issue_id: identity.1,
        identifier: identity.2,
        name: name.to_owned(),
        color: "".to_owned(),
    };
    let browser = browser::bindings();
    let handler = expr!(|event: Event| {
        if !browser.is_disposed() {
            event.prevent_default();
            event.stop_propagation();
            raw!(
                "window.dispatchEvent(new CustomEvent('lific:native-issue-label-request',{detail:${request},cancelable:true}))",
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
    let remove_label = format!("Remove {name}");
    view! {
        cx =>
        <button
            type="button"
            title="Remove label"
            aria-label=(remove_label)
            class="inline-flex size-3 items-center justify-center rounded-full opacity-60 transition-opacity hover:bg-[var(--bg-subtle)] hover:opacity-100"
            (attrs)
        >
            "×"
        </button>
    }
    .boxed()
}

fn palette_button<'a>(
    cx: &'a Cx,
    name: &str,
    hex: &str,
    picked: &Signal<String>,
    color_open: &Signal<bool>,
    is_selected: bool,
) -> BoxView<'a> {
    let choice = hex.to_owned();
    let selected = picked.clone();
    let close = color_open.clone();
    let browser = browser::bindings();
    let handler = expr!(|event: Event| {
        if !browser.is_disposed() {
            event.prevent_default();
            selected.set(choice);
            close.set(false);
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    let name = name.to_owned();
    let style = format!("background-color: {hex}");
    view! {
        cx =>
        <button
            type="button"
            title=(name.clone())
            aria-label=(name)
            class=(if is_selected {
                "grid size-6 place-items-center rounded-full ring-2 ring-[var(--text)] ring-offset-1 ring-offset-[var(--surface)] transition-transform hover:scale-110"
            } else {
                "grid size-6 place-items-center rounded-full transition-transform hover:scale-110"
            })
            style=(style)
            (attrs)
        >
            if is_selected {
                <span class="text-white drop-shadow">"✓"</span>
            }
        </button>
    }
    .boxed()
}

#[record]
#[derive(Clone)]
pub(crate) struct LabelCatalogItem {
    pub name: String,
    pub color: String,
}

#[record]
#[derive(Clone)]
pub(crate) struct LabelReply {
    pub status: Result<String, String>,
    pub account_id: i64,
    pub issue_id: i64,
    pub seq: i64,
    pub labels: Vec<String>,
    pub canonical: Option<ModuleAssignmentSnapshot>,
    pub catalog_item: Option<LabelCatalogItem>,
}

#[procedure("/__native_issue_edit/update_labels")]
pub(crate) async fn update_labels(cx: &Cx, request: LabelRequest) -> topcoat::Result<LabelReply> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = match crate::api::require_user(&caller.identity) {
        Ok(user) => user,
        Err(error) => return session::read(cx, Err(error)),
    };
    if user.id != request.account_id {
        return Ok(failed(&request, "Your account changed. Reload this page."));
    }
    let change = match request.mode.as_str() {
        "attach" => services::issues::IssueLabelChange::Attach(&request.name),
        "remove" => services::issues::IssueLabelChange::Remove(&request.name),
        _ => return Ok(failed(&request, "invalid label operation")),
    };
    let db = context::db(cx);
    let issue = match services::issues::resolve_issue(db, &caller.identity, &request.identifier) {
        Ok(issue) if issue.id == request.issue_id => issue,
        Ok(_) | Err(LificError::NotFound(_)) => return Ok(failed(&request, "not found")),
        Err(error @ LificError::Forbidden(_)) => {
            return Ok(failed(&request, error.client_message()));
        }
        Err(error) => return Err(error.into()),
    };
    let saved = match caller
        .scope(async {
            services::issues::commit_issue_label_change(
                db,
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                issue.id,
                change,
            )
        })
        .await
    {
        Ok(saved) => saved,
        Err(error @ LificError::Forbidden(_)) => {
            return Ok(failed(&request, error.client_message()));
        }
        Err(LificError::NotFound(_)) => return Ok(failed(&request, "not found")),
        Err(error) => return Ok(failed(&request, error.client_message())),
    };
    Ok(LabelReply {
        status: Ok("saved".into()),
        account_id: user.id,
        issue_id: saved.id,
        seq: saved.seq,
        labels: saved.labels.clone(),
        canonical: Some(ModuleAssignmentSnapshot::from_issue(&saved)),
        catalog_item: None,
    })
}

#[procedure("/__native_issue_edit/create_label")]
pub(crate) async fn create_label(cx: &Cx, request: LabelRequest) -> topcoat::Result<LabelReply> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = match crate::api::require_user(&caller.identity) {
        Ok(user) => user,
        Err(error) => return session::read(cx, Err(error)),
    };
    if user.id != request.account_id {
        return Ok(failed(&request, "Your account changed. Reload this page."));
    }
    if request.mode != "create" {
        return Ok(failed(&request, "invalid label operation"));
    }
    let db = context::db(cx);
    let issue = match services::issues::resolve_issue(db, &caller.identity, &request.identifier) {
        Ok(issue) if issue.id == request.issue_id => issue,
        Ok(_) | Err(LificError::NotFound(_)) => return Ok(failed(&request, "not found")),
        Err(error @ LificError::Forbidden(_)) => {
            return Ok(failed(&request, error.client_message()));
        }
        Err(error) => return Err(error.into()),
    };
    let label = match caller
        .scope(async {
            services::issues::commit_issue_label_create(
                db,
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                CreateLabel {
                    project_id: issue.project_id,
                    name: request.name.clone(),
                    color: request.color.clone(),
                },
            )
        })
        .await
    {
        Ok(label) => label,
        Err(error) => return Ok(failed(&request, error.client_message())),
    };
    let saved = caller
        .scope(async {
            services::issues::commit_issue_label_change(
                db,
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                issue.id,
                services::issues::IssueLabelChange::Attach(&label.name),
            )
        })
        .await;
    match saved {
        Ok(saved) => Ok(LabelReply {
            status: Ok("saved".into()),
            account_id: user.id,
            issue_id: saved.id,
            seq: saved.seq,
            labels: saved.labels.clone(),
            canonical: Some(ModuleAssignmentSnapshot::from_issue(&saved)),
            catalog_item: Some(LabelCatalogItem {
                name: label.name,
                color: label.color,
            }),
        }),
        Err(error) => Ok(LabelReply {
            status: Err(error.client_message().to_owned()),
            account_id: user.id,
            issue_id: issue.id,
            seq: issue.seq,
            labels: issue.labels.clone(),
            canonical: None,
            catalog_item: Some(LabelCatalogItem {
                name: label.name,
                color: label.color,
            }),
        }),
    }
}

fn failed(request: &LabelRequest, message: &str) -> LabelReply {
    LabelReply {
        status: Err(message.to_owned()),
        account_id: request.account_id,
        issue_id: request.issue_id,
        seq: 0,
        labels: Vec::new(),
        canonical: None,
        catalog_item: None,
    }
}
