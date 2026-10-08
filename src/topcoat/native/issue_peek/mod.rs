//! Shared issue hover and touch previews backed by authorized Rust services.
mod controls;
mod data;
mod entry;
mod gestures;
mod preview;
#[cfg(test)]
mod tests;

pub(crate) use entry::{button, request_handler, shared_owner};

use super::{context, icons, markdown, navigation, session};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, signal},
    view::{BoxView, Unescaped, View, ViewExt, component, view},
};

pub(crate) fn surface<'a>(
    cx: &'a Cx,
    account: i64,
    identifier: &str,
    touch: bool,
    close: Signal<String>,
) -> topcoat::Result<BoxView<'a>> {
    let caller = session::read(cx, data::checked_caller(cx, account))?;
    let data = data::load(cx, &caller, identifier);
    let identifier = identifier.to_owned();
    Ok(view! {
        cx =>
        owner(
            account: account,
            identifier: identifier,
            touch: touch,
            close: close,
            data: data
        )
    }
    .boxed())
}

#[component]
async fn owner(
    cx: &Cx,
    account: i64,
    identifier: String,
    touch: bool,
    close: Signal<String>,
    data: Result<data::Data, crate::error::LificError>,
) -> topcoat::Result<impl View> {
    if !touch {
        return Ok(preview::hover(cx, &identifier, data));
    }
    let comments = data
        .as_ref()
        .ok()
        .filter(|data| data.comments > 0)
        .map(|data| {
            let count = format!(
                "{}{}",
                data.comments,
                if data.comments_partial { "+" } else { "" }
            );
            let title = if data.comments_partial {
                format!("{} or more comments", data.comments)
            } else {
                format!(
                    "{} comment{}",
                    data.comments,
                    if data.comments == 1 { "" } else { "s" }
                )
            };
            view! {
                cx =>
                <span
                    class="inline-flex items-center gap-1 text-caption text-[var(--text-faint)]"
                    title=(title)
                    data-native-peek-comments=""
                >
                    (icons::ui_icon(cx, icons::UiIcon::Comment, 12))
                    (count)
                </span>
            }
            .boxed()
        });
    let content = match data {
        Ok(data) => sheet_content(cx, account, &data, close.clone()),
        Err(error) => view! {
            cx =>
            <div class="flex flex-col items-center gap-2 py-16 text-center">
                <p class="text-body-sm text-[var(--text-muted)]">
                    "Couldn't load this issue."
                </p>
                <p class="text-caption text-[var(--text-faint)]">(error.to_string())</p>
            </div>
        }
        .boxed(),
    };
    let scrim_close = close.clone();
    let button_close = close.clone();
    let dismiss = controls::dismiss(cx, close.clone());
    let drag = gestures::mount(cx, close);
    Ok(view! {
        cx =>
        <div
            class="fixed inset-0 z-[90] bg-black/30 backdrop-blur-[1px]"
            data-native-peek-scrim=""
            @click=$(|_event: Event| {
                scrim_close.set("".to_owned());
            })
        ></div>
        <section
            role="dialog"
            aria-modal="true"
            tabindex="-1"
            aria-label=(format!("{identifier} preview"))
            data-native-issue-peek=(identifier.clone())
            class="fixed z-[95] flex flex-col bg-[var(--surface)] shadow-2xl inset-x-0 bottom-0 h-[85dvh] rounded-t-xl border-t border-[var(--border)] pb-[env(safe-area-inset-bottom)] md:inset-y-0 md:right-0 md:left-auto md:bottom-auto md:h-full md:w-[480px] md:max-w-[92vw] md:rounded-none md:border-t-0 md:border-l"
            (dismiss)
        >
            <div class="shrink-0 touch-none" data-native-peek-grab="" (drag)>
                <div class="md:hidden flex justify-center pt-2 pb-1 shrink-0">
                    <div class="h-1 w-9 rounded-full bg-[var(--border)]"></div>
                </div>
                <div
                    class="flex items-center gap-2 px-4 pt-2 pb-2 md:pt-4 border-b border-[var(--border)]"
                >
                    <button
                        class="group inline-flex items-center gap-1 text-caption font-mono font-semibold px-1.5 py-0.5 rounded border border-[var(--border)] text-[var(--text-muted)] hover:border-[var(--accent)] hover:text-[var(--accent)] transition-colors"
                        aria-label=(format!("Copy {identifier}"))
                        (controls::clipboard(cx, &identifier))
                    >
                        (identifier.clone())
                        (icons::ui_icon(cx, icons::UiIcon::Copy, 11))
                    </button>
                    if let Some(comments) = comments {
                        (comments)
                    }
                    <div class="flex-1"></div>
                    <button
                        type="button"
                        class="size-7 flex items-center justify-center rounded-md text-[var(--text-faint)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)] transition-colors"
                        aria-label="Close preview"
                        @click=$(|_event: Event| {
                            button_close.set("".to_owned());
                        })
                    >
                        (icons::ui_icon(cx, icons::UiIcon::Close, 16))
                    </button>
                </div>
            </div>
            (content)
        </section>
    }.boxed())
}

fn sheet_content<'a>(
    cx: &'a Cx,
    account: i64,
    data: &data::Data,
    close: Signal<String>,
) -> BoxView<'a> {
    let issue = &data.issue;
    let state_cx = cx.keyed((account, issue.id));
    let state = controls::State::new(&state_cx, issue);
    let editing = state.editing.clone();
    let start_edit = editing.clone();
    let title = state.title.clone();
    let input_title = state.draft.clone();
    let change_title = input_title.clone();
    let start_title = title.clone();
    let start_draft = input_title.clone();
    let busy = state.busy.clone();
    let title_busy = busy.clone();
    let error = state.error.clone();
    let undo_field = state.undo_field.clone();
    let updated = state.updated.clone();
    let title_blur = controls::edit(
        cx,
        &state,
        account,
        &issue.identifier,
        "title",
        "blur",
        false,
    );
    let title_key = controls::edit(
        cx,
        &state,
        account,
        &issue.identifier,
        "title",
        "keydown",
        false,
    );
    let undo = controls::edit(cx, &state, account, &issue.identifier, "", "click", true);
    let status_options = options(
        cx,
        &state,
        account,
        &issue.identifier,
        "status",
        &[
            ("backlog", "Backlog"),
            ("todo", "Todo"),
            ("active", "Active"),
            ("done", "Done"),
            ("cancelled", "Cancelled"),
        ],
    );
    let priority_options = options(
        cx,
        &state,
        account,
        &issue.identifier,
        "priority",
        &[
            ("urgent", "Urgent"),
            ("high", "High"),
            ("medium", "Medium"),
            ("low", "Low"),
            ("none", "No priority"),
        ],
    );
    let module_values: Vec<_> = std::iter::once((String::new(), "No module".to_owned()))
        .chain(
            data.modules
                .iter()
                .map(|module| (module.id.to_string(), module.name.clone())),
        )
        .collect();
    let module_borrows: Vec<_> = module_values
        .iter()
        .map(|(id, name)| (id.as_str(), name.as_str()))
        .collect();
    let module_options = options(
        cx,
        &state,
        account,
        &issue.identifier,
        "module",
        &module_borrows,
    );
    let module = data
        .modules
        .iter()
        .find(|module| Some(module.id) == issue.module_id);
    let module_name = module.map_or_else(
        || {
            issue
                .module_id
                .map_or_else(|| "No module".into(), |id| format!("Module #{id}"))
        },
        |module| module.name.clone(),
    );
    let module_icon = match module.and_then(|module| module.emoji.as_deref()) {
        Some(emoji) => icons::project_icon(cx, Some(emoji), 13),
        None => icons::ui_icon(cx, icons::UiIcon::Modules, 13),
    };
    let label_chips = issue
        .labels
        .iter()
        .map(|name| {
            let color = data
                .labels
                .iter()
                .find(|label| label.name == *name)
                .map(|label| label.color.as_str());
            super::label_chip::render(cx, name.clone(), color)
        })
        .collect::<Vec<_>>();
    let relations = relation_chips(cx, issue);
    let rendered_description =
        markdown::render(cx, &issue.description, markdown::Scope::Private, &[]);
    let description = if issue.description.is_empty() {
        None
    } else {
        Some(Unescaped::new_unchecked(rendered_description))
    };
    let href = navigation::attrs(
        cx,
        &format!(
            "/{}/issues/{}",
            issue
                .identifier
                .split_once('-')
                .map_or("", |(project, _)| project),
            issue.identifier
        ),
    );
    let open_close = close;
    let copied = controls::clipboard(cx, &issue.identifier);
    let editable = data.editable;
    let has_modules = !data.modules.is_empty();
    let readonly_title = issue.title.clone();
    let status_icon = icons::status_icon(cx, issue.status, 13);
    let status_caption = caption(issue.status.as_str()).to_owned();
    let priority_icon = icons::priority_icon(cx, issue.priority, 13);
    let priority_caption = caption(issue.priority.as_str()).to_owned();
    view! {
        cx =>
        <div class="flex-1 overflow-y-auto px-4 py-4">
            if editable {
                <button
                    type="button"
                    class="block w-full text-left text-title font-semibold py-1 mb-4"
                    :hidden=$(editing.get())
                    :disabled=$(title_busy.get())
                    @click=$(|_event: Event| {
                        start_draft.set(start_title.get());
                        start_edit.set(true);
                        raw!(
                            "requestAnimationFrame(()=>${_event}.inner.target.parentElement.querySelector('[data-native-peek-title]').focus());",
                            (),
                        );
                    })
                >
                    $(title.get())
                </button>
                <input
                    data-native-peek-title=""
                    aria-label="Issue title"
                    class="w-full text-title font-semibold py-1 mb-4 bg-transparent text-[var(--text)]"
                    :hidden=$(!editing.get())
                    :value=$(input_title.get())
                    @input=$(|_event: Event| {
                        change_title.set(
                            raw!(
                                "cx.hydrate(${_event}.inner.target.value)",
                                String::new(),
                            ),
                        );
                    })
                    (title_blur)
                    (title_key)
                >
            } else {
                <h2 class="text-title font-semibold py-1 mb-4">(readonly_title)</h2>
                <p class="text-caption text-[var(--text-faint)] mb-3">
                    "Read-only access"
                </p>
            }
            <div class="flex flex-wrap items-center gap-2 mb-4">
                if editable {
                    (status_options)
                    (priority_options)
                    if has_modules {
                        (module_options)
                    }
                } else {
                    <span
                        class="inline-flex items-center gap-1.5 rounded-md border border-[var(--border)] px-2 py-1 text-body-sm text-[var(--text)] cursor-default"
                    >
                        (status_icon)
                        (status_caption)
                    </span>
                    <span
                        class="inline-flex items-center gap-1.5 rounded-md border border-[var(--border)] px-2 py-1 text-body-sm text-[var(--text)] cursor-default"
                    >
                        (priority_icon)
                        (priority_caption)
                    </span>
                    <span
                        class="inline-flex items-center gap-1.5 rounded-md border border-[var(--border)] px-2 py-1 text-body-sm text-[var(--text)] cursor-default"
                    >
                        (module_icon)
                        (module_name)
                    </span>
                }
            </div>
            <p
                role="alert"
                class="text-caption text-[var(--error)]"
                :hidden=$(error.get().is_empty())
            >
                $(error.get())
            </p>
            <div
                role="status"
                class="text-caption"
                :hidden=$(undo_field.get().is_empty())
            >
                "Issue updated. "
                <button
                    type="button"
                    class="text-[var(--accent)] hover:underline"
                    :disabled=$(busy.get())
                    (undo)
                >
                    "Undo"
                </button>
            </div>
            if !label_chips.is_empty() {
                <div class="flex flex-wrap gap-1.5 mb-4">
                    for chip in label_chips {
                        (chip)
                    }
                </div>
            }
            (relations)
            <div class="border-t border-[var(--border)] -mx-4 mb-4"></div>
            if let Some(description) = description {
                <div class="tc-markdown text-[14px] leading-[1.7]">(description)</div>
            } else {
                <p class="text-body-sm text-[var(--text-faint)] italic">
                    "No description"
                </p>
            }
            <p class="text-caption text-[var(--text-faint)] mt-4">
                "Updated "
                (super::dates::absolute_signal(cx, updated))
            </p>
        </div>
        <div
            class="shrink-0 border-t border-[var(--border)] px-4 py-3 flex items-center justify-between gap-2"
        >
            <button
                type="button"
                class="inline-flex flex-row-reverse items-center gap-1 text-body-sm text-[var(--text-muted)] hover:text-[var(--text)] transition-colors"
                (copied)
            >
                "Copy identifier"
                (icons::ui_icon(cx, icons::UiIcon::Copy, 13))
            </button>
            <a
                class="inline-flex items-center gap-1.5 text-body-sm font-medium text-[var(--accent)] hover:underline transition-colors"
                (href)
                @click=$(|_event: Event| {
                    open_close.set("".to_owned());
                })
            >
                "Open full view"
                (icons::ui_icon(cx, icons::UiIcon::RecentActivity, 14))
            </a>
        </div>
    }.boxed()
}

fn options<'a>(
    cx: &'a Cx,
    state: &controls::State,
    account: i64,
    identifier: &str,
    field: &str,
    values: &[(&str, &str)],
) -> BoxView<'a> {
    let selected = match field {
        "status" => state.status.clone(),
        "priority" => state.priority.clone(),
        _ => state.module.clone(),
    };
    let attrs = controls::edit(cx, state, account, identifier, field, "change", false);
    let busy = state.busy.clone();
    let options = values
        .iter()
        .map(|(value, label)| {
            let value = value.to_string();
            let option_value = value.clone();
            let selected = selected.clone();
            let label = label.to_string();
            view! {
                cx =>
                <option value=(value) :selected=$(selected.get() == option_value)>
                    (label)
                </option>
            }
            .boxed()
        })
        .collect::<Vec<_>>();
    let label = field.to_owned();
    let marker = label.clone();
    view!{
        cx =>
        <select
            aria-label=(label)
            data-native-peek-save=(marker)
            class="rounded-md border border-[var(--border)] px-2 py-1 text-body-sm bg-[var(--surface)] text-[var(--text)]"
            :disabled=$(busy.get())
            (attrs)
        >
            for option in options {
                (option)
            }
        </select>
    }.boxed()
}
fn relation_chips<'a>(cx: &'a Cx, issue: &crate::db::models::Issue) -> BoxView<'a> {
    let mut chips = Vec::new();
    for (values, prefix, label, class) in [
        (
            &issue.blocked_by,
            "⛔ ",
            "Blocked by",
            "font-mono text-[var(--error)] bg-[var(--error-bg)] px-1.5 py-0.5 rounded",
        ),
        (
            &issue.blocks,
            "→ ",
            "Blocks",
            "font-mono text-[var(--accent)] bg-[var(--accent-subtle)] px-1.5 py-0.5 rounded",
        ),
        (
            &issue.relates_to,
            "",
            "Related to",
            "font-mono text-[var(--text-muted)] bg-[var(--bg-subtle)] px-1.5 py-0.5 rounded",
        ),
    ] {
        for value in values {
            let title = format!("{label} {value}");
            let text = format!("{prefix}{value}");
            chips.push(view! { cx => <span class=(class) title=(title)>(text)</span> }.boxed());
        }
    }
    view! {
        cx =>
        <div class="flex flex-wrap items-center gap-1.5 mb-4 text-caption">
            for chip in chips {
                (chip)
            }
        </div>
    }
    .boxed()
}

fn caption(value: &str) -> &str {
    match value {
        "backlog" => "Backlog",
        "todo" => "Todo",
        "active" => "Active",
        "done" => "Done",
        "cancelled" => "Cancelled",
        "urgent" => "Urgent",
        "high" => "High",
        "medium" => "Medium",
        "low" => "Low",
        "none" => "No priority",
        _ => value,
    }
}
