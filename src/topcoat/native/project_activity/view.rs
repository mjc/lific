//! Main's project history feed, expanded records and actor rail.
use super::super::dates::{absolute_time_view as absolute, relative_time_view as relative};
use super::super::icons::UiIcon;
use super::super::numbers::count as localized_count;
use super::super::{activity_text, avatar, icons, navigation};
use super::diff::{self, DiffKind, DiffRow};
use crate::db::models::{Activity, ActorStat, Priority, Status};
use chrono::{Datelike, NaiveDate, NaiveDateTime};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal},
    view::{BoxView, ViewExt, view},
};

pub(super) fn topbar<'a>(
    cx: &'a Cx,
    identifier: &str,
    items: &[Activity],
    actors: &[ActorStat],
    has_more: bool,
    filter: Signal<Option<Option<i64>>>,
) -> BoxView<'a> {
    let selected = filter.get_untracked();
    let count = items
        .iter()
        .filter(|item| matches_actor(item, selected))
        .count();
    let count = format!(
        "{count}{}",
        if has_more && selected.is_none() {
            "+"
        } else {
            ""
        }
    );
    let active = selected.map(|id| {
        actors
            .iter()
            .find(|actor| actor.actor_user_id == id)
            .map_or("?", actor_name)
            .to_owned()
    });
    let identifier = identifier.to_owned();
    view! {
        cx =>
        <div
            class="native-project-activity__topbar text-base font-normal leading-[1.6] flex items-center gap-3 px-6 py-2 w-full"
        >
            <div class="flex items-center gap-1.5 shrink-0">
                <a
                    class="text-body-sm font-mono font-medium text-[var(--text-muted)] hover:text-[var(--text)] transition-colors no-underline"
                    (navigation::attrs(cx, &format!("/{identifier}/overview")))
                >
                    (identifier)
                </a>
                (faint_icon(cx, UiIcon::BreadcrumbSeparator, 12))
                <span class="text-body-sm font-medium text-[var(--text)]">
                    "Activity"
                </span>
                <span
                    class="ml-1 text-micro text-[var(--text-faint)] font-medium tabular-nums"
                >
                    (count)
                </span>
            </div>
            if let Some(active) = active {
                <div class="flex items-center gap-1.5">
                    <span
                        class="flex items-center gap-1.5 text-caption font-medium text-[var(--accent)] bg-[var(--accent-subtle)] pl-2.5 pr-1 py-0.5 rounded-full"
                    >
                        (active)
                        " only"
                        <button
                            type="button"
                            class="size-4 flex items-center justify-center rounded-full hover:bg-[var(--accent)] hover:text-[var(--accent-text)] transition-colors bg-transparent border-0 p-0 text-inherit text-caption leading-[1.6]"
                            title="Clear actor filter"
                            @click=$(|_event: Event| filter.set(
                                    raw!("cx.none()", None::<Option<i64>>),
                                ))
                        >
                            "×"
                        </button>
                    </span>
                </div>
            }
        </div>
    }.boxed()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn content<'a>(
    cx: &'a Cx,
    identifier: &str,
    items: &[Activity],
    actors: &[ActorStat],
    has_more: bool,
    filter: Signal<Option<Option<i64>>>,
    expanded: Signal<Option<i64>>,
    now: Signal<f64>,
    more: Signal<usize>,
    loading_more: Signal<bool>,
    timezone: chrono_tz::Tz,
) -> BoxView<'a> {
    if items.is_empty() {
        return view! {
            cx =>
            <div
                class="native-project-activity__empty flex flex-col items-center py-20 gap-3 px-6 max-w-[480px] mx-auto text-center"
            >
                (faint_icon(cx, UiIcon::History, 32))
                <p class="text-body-lg text-[var(--text-muted)] m-0">
                    "No activity yet"
                </p>
                <p class="text-body-sm text-[var(--text-faint)] leading-relaxed m-0">
                    "Every change in this project lands here — who did it, what changed, and whether it came through the web UI, an agent over MCP, the API, or the CLI."
                </p>
            </div>
        }.boxed();
    }
    let selected = filter.get_untracked();
    let opened = expanded.get_untracked();
    let groups = day_groups(items, selected, now.get_untracked(), timezone).into_iter().map(|(label, rows)| {
        let count = rows.len();
        let rows = rows.into_iter().map(|item| row(cx, identifier, item, actors, opened == Some(item.id), expanded.clone(), now.clone())).collect::<Vec<_>>();
        view! {
            cx =>
            <div class="native-project-activity__day mb-6 last:mb-0">
                <div
                    class="sticky top-0 z-10 -mx-2 px-2 py-1.5 mb-1 bg-[var(--bg)] flex items-center gap-2"
                >
                    <span
                        class="text-micro font-semibold uppercase tracking-widest text-[var(--text-muted)]"
                    >
                        (label)
                    </span>
                    <span class="text-micro text-[var(--text-faint)] tabular-nums">
                        (count)
                    </span>
                    <div class="flex-1 h-px bg-[var(--border)]"></div>
                </div>
                for row in rows {
                    (row)
                }
            </div>
        }.boxed()
    }).collect::<Vec<_>>();
    let rail = actor_rail(cx, actors, selected, filter, expanded, now);
    view! {
        cx =>
        <div
            class="native-project-activity__content flex flex-col lg:flex-row gap-8 px-8 py-6 max-w-[1280px] mx-auto items-start"
        >
            <div class="native-project-activity__feed flex-1 min-w-0 w-full">
                for group in groups {
                    (group)
                }
                if has_more && selected.is_none() {
                    <button
                        type="button"
                        class="native-project-activity__more mt-2 text-caption leading-[1.6] text-[var(--text-muted)] hover:text-[var(--text)] inline-flex items-center gap-1 transition-colors px-2.5 py-1 rounded-md hover:bg-[var(--bg-subtle)] bg-transparent border-0"
                        :disabled=$(loading_more.get())
                        @click=$(|_event: Event| {
                            loading_more.set(true);
                            more.increment();
                        })
                    >
                        (icons::ui_icon(cx, UiIcon::Expand, 12))
                        $(if loading_more.get() { "Loading..." } else { "Load more" })
                    </button>
                }
            </div>
            (rail)
        </div>
    }.boxed()
}

fn matches_actor(item: &Activity, selected: Option<Option<i64>>) -> bool {
    selected.is_none_or(|id| item.actor_user_id == id)
}
fn actor_name(actor: &ActorStat) -> &str {
    avatar::display_name(
        actor.display_name.as_deref(),
        actor.username.as_deref(),
        "system",
    )
}
fn activity_actor(item: &Activity) -> &str {
    avatar::display_name(
        item.actor_display_name.as_deref(),
        item.actor_username.as_deref(),
        "system",
    )
}

fn day_groups(
    items: &[Activity],
    selected: Option<Option<i64>>,
    epoch: f64,
    timezone: chrono_tz::Tz,
) -> Vec<(String, Vec<&Activity>)> {
    let today = chrono::DateTime::from_timestamp_millis(epoch as i64)
        .unwrap_or_default()
        .with_timezone(&timezone)
        .date_naive();
    let yesterday = chrono::DateTime::from_timestamp_millis((epoch - 86_400_000.0) as i64)
        .unwrap_or_default()
        .with_timezone(&timezone)
        .date_naive();
    let mut groups: Vec<(String, Vec<&Activity>)> = Vec::new();
    let mut previous = None;
    for item in items.iter().filter(|item| matches_actor(item, selected)) {
        let date = NaiveDateTime::parse_from_str(&item.ts, "%Y-%m-%d %H:%M:%S%.f")
            .map(|date| date.and_utc().with_timezone(&timezone).date_naive())
            .unwrap_or_default();
        if previous != Some(date) {
            groups.push((day_label(date, today, yesterday), Vec::new()));
            previous = Some(date);
        }
        if let Some((_, rows)) = groups.last_mut() {
            rows.push(item);
        }
    }
    groups
}
fn day_label(date: NaiveDate, today: NaiveDate, yesterday: NaiveDate) -> String {
    if date == today {
        "Today".into()
    } else if date == yesterday {
        "Yesterday".into()
    } else {
        date.format(if date.year() == today.year() {
            "%a, %b %-d"
        } else {
            "%a, %b %-d, %Y"
        })
        .to_string()
    }
}
fn verb(item: &Activity) -> String {
    match item.action.as_str() {
        "create" if item.entity_type == "comment" => "commented on".into(),
        "delete" if item.entity_type == "comment" => "deleted a comment on".into(),
        "update" if item.entity_type == "comment" => "edited a comment on".into(),
        "create" => format!("created {}", item.entity_type),
        "delete" => format!("deleted {}", item.entity_type),
        "update" => format!("changed {} on", item.field.as_deref().unwrap_or("null")),
        "attach" => "labeled".into(),
        "detach" => "unlabeled".into(),
        "link" | "unlink" => format!(
            "{} {}",
            if item.action == "link" {
                "linked"
            } else {
                "unlinked"
            },
            item.field
                .as_deref()
                .unwrap_or("relates_to")
                .replacen('_', " ", 1)
        ),
        _ => item.action.clone(),
    }
}
fn destination(identifier: &str, item: &Activity) -> Option<String> {
    match item.entity_type.as_str() {
        "issue" => item
            .entity_label
            .as_deref()
            .filter(|label| !label.is_empty())
            .map(|label| format!("/{identifier}/issues/{label}")),
        "page" => Some(format!("/{identifier}/pages/{}", item.entity_id)),
        "module" => Some(format!("/{identifier}/modules/{}", item.entity_id)),
        "comment"
            if item.issue_id.is_some()
                && item
                    .entity_label
                    .as_ref()
                    .is_some_and(|label| !label.is_empty()) =>
        {
            Some(format!(
                "/{identifier}/issues/{}",
                item.entity_label.as_deref().unwrap_or_default()
            ))
        }
        "comment" => item.page_id.map(|id| format!("/{identifier}/pages/{id}")),
        _ => None,
    }
}
fn faint_icon<'a>(cx: &'a Cx, name: UiIcon, size: u32) -> BoxView<'a> {
    let icon = icons::ui_icon(cx, name, size);
    view! {
        cx =>
        <span class="inline-flex shrink-0 text-[var(--text-faint)]">(icon)</span>
    }
    .boxed()
}

fn entity_icon<'a>(cx: &'a Cx, entity: &str, size: u32) -> BoxView<'a> {
    faint_icon(
        cx,
        match entity {
            "issue" => UiIcon::IssueLink,
            "page" => UiIcon::Page,
            "comment" => UiIcon::Comment,
            "module" => UiIcon::Modules,
            "label" => UiIcon::Labels,
            "folder" => UiIcon::Folder,
            _ => UiIcon::Entity,
        },
        size,
    )
}

fn row<'a>(
    cx: &'a Cx,
    identifier: &str,
    item: &Activity,
    actors: &[ActorStat],
    open: bool,
    expanded: Signal<Option<i64>>,
    now: Signal<f64>,
) -> BoxView<'a> {
    let id = item.id;
    let label = item
        .entity_label
        .clone()
        .unwrap_or_else(|| format!("#{}", item.entity_id));
    let dest = destination(identifier, item);
    let link_label = label.clone();
    let link = if let Some(path) = dest.clone() {
        view! {
            cx =>
            <a
                class="font-mono text-caption text-[var(--accent)] hover:underline no-underline"
                (navigation::attrs(cx, &path))
            >
                (link_label.clone())
            </a>
        }
        .boxed()
    } else {
        view! { cx => <span class="font-mono text-caption">(link_label.clone())</span> }.boxed()
    };
    let summary = summary(cx, item);
    let record = if open {
        Some(record(cx, item, actors, &label, dest, now.clone()))
    } else {
        None
    };
    let actor = activity_actor(item).to_owned();
    let description = verb(item);
    let bot = item.actor_is_bot;
    let icon = entity_icon(cx, &item.entity_type, 14);
    let time = relative(cx, &item.ts, now);
    let class = if open {
        "native-project-activity__row rounded-md transition-colors bg-[var(--surface)] border border-solid border-[var(--border)] shadow-[0_1px_3px_rgba(0,0,0,0.05)] my-1.5"
    } else {
        "native-project-activity__row rounded-md transition-colors hover:bg-[var(--bg-subtle)] border border-solid border-transparent"
    };
    view! {
        cx =>
        <div
            class=(class)
            data-activity-id=(id.to_string())
            data-activity-expanded=(open.to_string())
        >
            <div
                class="native-project-activity__row-toggle flex items-center gap-2.5 px-2.5 py-1.5 cursor-pointer"
                role="button"
                tabindex="0"
                @click=$(|_event: Event| {
                    let link = raw!(
                        "cx.hydrate(${_event}.target?.closest?.('a[href]') !== null)",
                        false,
                    );
                    if !link {
                        if open {
                            expanded.set(raw!("cx.none()", None::<i64>));
                        } else {
                            expanded.set(raw!("cx.some(${id})", Some(id)));
                        }
                    }
                })
            >
                (icon)
                <div
                    class="flex-1 min-w-0 text-body-sm leading-relaxed text-[var(--text-muted)] truncate"
                >
                    <span class="font-medium text-[var(--text)]">(actor)</span>
                    " "
                    if bot {
                        <span
                            class="inline-block align-middle text-micro font-semibold uppercase tracking-wider px-1 py-px rounded bg-[var(--accent-subtle)] text-[var(--accent)] mx-0.5"
                        >
                            "agent"
                        </span>
                        " "
                    }
                    (description)
                    " "
                    (link)
                    " "
                    (summary)
                </div>
                <span class="shrink-0 text-micro text-[var(--text-faint)] tabular-nums">
                    (time)
                </span>
                <span
                    class=(if open {
                        "inline-flex shrink-0 text-[var(--text-faint)] transition-transform rotate-180"
                    } else {
                        "inline-flex shrink-0 text-[var(--text-faint)] transition-transform"
                    })
                >
                    (icons::ui_icon(cx, UiIcon::Expand, 12))
                </span>
            </div>
            if let Some(record) = record {
                (record)
            }
        </div>
    }.boxed()
}

fn status<'a>(cx: &'a Cx, value: Option<&str>) -> BoxView<'a> {
    value
        .and_then(|value| value.parse::<Status>().ok())
        .map_or_else(
            || {
                view! {
                    cx =>
                    <span
                        style="color:var(--tc-faint);display:inline-flex;flex-shrink:0"
                    >
                        (icons::ui_icon(cx, UiIcon::Issue, 12))
                    </span>
                }
                .boxed()
            },
            |value| icons::status_icon(cx, value, 12),
        )
}
fn summary<'a>(cx: &'a Cx, item: &Activity) -> BoxView<'a> {
    if item.action == "update" && item.field.as_deref() == Some("status") {
        let old = status(cx, item.old_value.as_deref());
        let new = status(cx, item.new_value.as_deref());
        let value = item.new_value.clone().unwrap_or_default();
        return view! {
            cx =>
            <span class="inline-flex items-center gap-1 align-middle mx-0.5">
                (old)
            </span>
            <span class="text-[var(--text-faint)]">"→"</span>
            <span class="inline-flex items-center gap-1 align-middle mx-0.5">
                (new)
                <span class="capitalize text-[var(--text)]">(value)</span>
            </span>
        }
        .boxed();
    }
    if item.action == "update" && item.field.as_deref() == Some("priority") {
        let icon = icons::priority_icon(
            cx,
            item.new_value
                .as_deref()
                .and_then(|value| value.parse::<Priority>().ok())
                .unwrap_or(Priority::None),
            12,
        );
        let value = item.new_value.clone().unwrap_or_default();
        return view! {
            cx =>
            <span class="text-[var(--text-faint)]">"→"</span>
            <span class="inline-flex items-center gap-1 align-middle mx-0.5">
                (icon)
                <span class="capitalize text-[var(--text)]">(value)</span>
            </span>
        }
        .boxed();
    }
    let value = if item.action == "attach" || item.action == "link" {
        item.new_value.clone()
    } else {
        item.old_value.clone()
    }
    .unwrap_or_default();
    if matches!(item.action.as_str(), "attach" | "detach") {
        return view! {
            cx =>
            <span
                class="text-micro font-medium px-1.5 py-0.5 rounded-full border border-solid border-[var(--border)] align-middle"
            >
                (value)
            </span>
        }.boxed();
    }
    if matches!(item.action.as_str(), "link" | "unlink") {
        return view! {
            cx =>
            <span class="font-mono text-caption text-[var(--accent)]">(value)</span>
        }
        .boxed();
    }
    if item.action == "create" && item.entity_type == "comment" {
        let text = activity_text::short_value(item.new_value.as_deref(), 48);
        return view! {
            cx =>
            <span class="text-[var(--text-faint)] italic">
                "“"
                (text)
                "”"
            </span>
        }
        .boxed();
    }
    if item.action == "update" && !matches!(item.field.as_deref(), Some("description" | "content"))
    {
        let old = activity_text::short_value(item.old_value.as_deref(), 24);
        let new = activity_text::short_value(item.new_value.as_deref(), 24);
        return view! {
            cx =>
            <span class="text-[var(--text-faint)]">(old)</span>
            " "
            <span class="text-[var(--text-faint)]">"→"</span>
            " "
            <span class="text-[var(--text)]">(new)</span>
        }
        .boxed();
    }
    view! { cx => <span></span> }.boxed()
}

fn ordinal(rank: usize) -> String {
    let suffix = match (rank % 100, rank % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{rank}{suffix}")
}

fn record<'a>(
    cx: &'a Cx,
    item: &Activity,
    actors: &[ActorStat],
    label: &str,
    dest: Option<String>,
    now: Signal<f64>,
) -> BoxView<'a> {
    let when = absolute(cx, &item.ts);
    let utc = format!("{} UTC", item.ts);
    let actor = activity_actor(item).to_owned();
    let transport = item.transport.clone();
    let bot = item.actor_is_bot;
    let username = if item
        .actor_username
        .as_ref()
        .is_some_and(|name| !name.is_empty())
        && item
            .actor_display_name
            .as_ref()
            .is_some_and(|name| !name.is_empty())
        && item.actor_username != item.actor_display_name
    {
        item.actor_username.clone()
    } else {
        None
    };
    let standing = actors
        .iter()
        .enumerate()
        .find(|(_, actor)| actor.actor_user_id == item.actor_user_id)
        .map(|(index, actor)| {
            let actions = localized_count(cx, actor.actions);
            let plural = if actor.actions == 1 {
                " action"
            } else {
                " actions"
            };
            let rank = ordinal(index + 1);
            let seen = relative(cx, &actor.last_ts, now);
            view! {
                cx =>
                <p class="text-micro text-[var(--text-muted)] m-0 mt-0.5">
                    (actions)
                    (plural)
                    " in this project · "
                    (rank)
                    " most active · last seen "
                    (seen)
                </p>
            }
            .boxed()
        });
    let entity = item.entity_type.clone();
    let icon = entity_icon(cx, &entity, 13);
    let label = label.to_owned();
    let action = format!(
        "— {}{}",
        item.action,
        item.field
            .as_deref()
            .filter(|field| !field.is_empty())
            .map(|field| format!(" · {field}"))
            .unwrap_or_default()
    );
    let values = values(cx, item);
    view! {
        cx =>
        <div
            class="native-project-activity__record px-4 pb-3.5 pt-1 border-0 border-t border-solid border-[var(--border)] mx-2.5 mb-1"
        >
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-x-8 gap-y-3 pt-2.5">
                <div>
                    <p
                        class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mt-0 mb-1"
                    >
                        "When"
                    </p>
                    <p class="text-body-sm text-[var(--text)] m-0">(when)</p>
                    <p class="text-micro font-mono text-[var(--text-faint)] m-0 mt-0.5">
                        (utc)
                    </p>
                </div>
                <div>
                    <p
                        class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mt-0 mb-1"
                    >
                        "Who"
                    </p>
                    <p class="text-body-sm text-[var(--text)] m-0">
                        (actor)
                        if let Some(username) = username {
                            " "
                            <span class="text-[var(--text-faint)]">
                                "("
                                (username)
                                ")"
                            </span>
                        }
                        if bot {
                            <span
                                class="inline-block align-middle text-micro font-semibold uppercase tracking-wider px-1 py-px rounded bg-[var(--accent-subtle)] text-[var(--accent)] ml-1"
                            >
                                "agent"
                            </span>
                        }
                        " "
                        <span class="text-[var(--text-muted)]">
                            "via "
                            (transport)
                        </span>
                    </p>
                    if let Some(standing) = standing {
                        (standing)
                    }
                </div>
                <div class="sm:col-span-2">
                    <p
                        class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mt-0 mb-1"
                    >
                        "What"
                    </p>
                    <p
                        class="text-body-sm text-[var(--text)] m-0 flex items-center gap-1.5 flex-wrap"
                    >
                        (icon)
                        <span class="capitalize">(entity)</span>
                        <span class="font-mono text-caption text-[var(--text-muted)]">
                            (label)
                        </span>
                        <span class="text-[var(--text-muted)]">(action)</span>
                        if let Some(dest) = dest {
                            <a
                                class="inline-flex items-center gap-0.5 text-caption text-[var(--accent)] hover:underline no-underline"
                                (navigation::attrs(cx, &dest))
                            >
                                "Open"
                                (icons::ui_icon(cx, UiIcon::RecentActivity, 11))
                            </a>
                        }
                    </p>
                </div>
                (values)
            </div>
        </div>
    }.boxed()
}

fn values<'a>(cx: &'a Cx, item: &Activity) -> BoxView<'a> {
    if item.old_value.is_none() && item.new_value.is_none() {
        return view! { cx => <span></span> }.boxed();
    }
    let old = item.old_value.as_deref().unwrap_or_default();
    let new = item.new_value.as_deref().unwrap_or_default();
    if (old.contains('\n') || new.contains('\n'))
        && let Some(lines) = diff::diff_lines(old, new)
    {
        let unchanged = diff::is_unchanged(&lines);
        let rows=diff::fold_context(&lines).into_iter().map(|row|match row {DiffRow::Fold{count}=>{let text=format!("{count} unchanged line{}",if count==1{""}else{"s"});view! {
            cx =>
            <div
                class="flex items-center gap-2 px-3 py-1 bg-[var(--bg-subtle)] select-none"
            >
                <span class="h-px flex-1 bg-[var(--border)]"></span>
                <span class="text-micro text-[var(--text-faint)] tabular-nums">
                    (text)
                </span>
                <span class="h-px flex-1 bg-[var(--border)]"></span>
            </div>
        }.boxed()},DiffRow::Line(line)=>{let (class,marker)=match line.kind {DiffKind::Removed=>("flex gap-2 px-3 min-h-[1.5em] bg-[var(--tc-error-bg)] text-[var(--text-muted)]","-"),DiffKind::Added=>("flex gap-2 px-3 min-h-[1.5em] bg-[var(--tc-success-bg)] text-[var(--text)]","+"),DiffKind::Context=>("flex gap-2 px-3 min-h-[1.5em] text-[var(--text-muted)]","")};view! {
            cx =>
            <div class=(class)>
                <span
                    class="shrink-0 w-3 font-mono select-none text-[var(--text-faint)]"
                    aria-hidden="true"
                >
                    (marker)
                </span>
                <span class="flex-1 min-w-0 whitespace-pre-wrap break-words">
                    (line.text)
                </span>
            </div>
        }.boxed()}}).collect::<Vec<_>>();
        return view! {
            cx =>
            <div class="sm:col-span-2 flex flex-col gap-1.5">
                <div
                    class="native-project-activity__diff text-caption leading-relaxed rounded-md border border-solid border-[var(--border)] overflow-hidden max-h-[320px] overflow-y-auto"
                >
                    if unchanged {
                        <p class="px-3 py-2 m-0 text-[var(--text-faint)] italic">
                            "No line-level changes"
                        </p>
                    }
                    for row in rows {
                        (row)
                    }
                </div>
            </div>
        }.boxed();
    }
    let old = item.old_value.clone();
    let new = item.new_value.clone();
    view! {
        cx =>
        <div class="sm:col-span-2 flex flex-col gap-1.5">
            if let Some(old) = old {
                <div
                    class="text-caption leading-relaxed px-3 py-2 rounded-md border border-solid border-[var(--border)] bg-[var(--tc-error-bg)] text-[var(--text-muted)] whitespace-pre-wrap break-words max-h-[240px] overflow-y-auto"
                >
                    (old)
                </div>
            }
            if let Some(new) = new {
                <div
                    class="text-caption leading-relaxed px-3 py-2 rounded-md border border-solid border-[var(--border)] bg-[var(--tc-success-bg)] text-[var(--text)] whitespace-pre-wrap break-words max-h-[240px] overflow-y-auto"
                >
                    (new)
                </div>
            }
        </div>
    }.boxed()
}

fn actor_rail<'a>(
    cx: &'a Cx,
    actors: &[ActorStat],
    selected: Option<Option<i64>>,
    filter: Signal<Option<Option<i64>>>,
    expanded: Signal<Option<i64>>,
    now: Signal<f64>,
) -> BoxView<'a> {
    let max = actors
        .iter()
        .map(|actor| actor.actions)
        .max()
        .unwrap_or(1)
        .max(1);
    let total = actors.len();
    let rows=actors.iter().map(|actor|{let key=actor.actor_user_id;let active=selected==Some(key);let name=actor_name(actor).to_owned();let initials=avatar::initials(&name);let bot=actor.is_bot;let title=format!("{} · most via {}",if active {"Clear filter".into()}else{format!("Show only {name}")},actor.top_transport);let transport=actor.top_transport.clone();let seen=relative(cx,&actor.last_ts,now.clone());let actions=localized_count(cx,actor.actions);let width=format!("width:calc({}% - 1.25rem)",(actor.actions as f64/max as f64*100.0).max(4.0));let filter=filter.clone();let expanded=expanded.clone();let class=if active {"native-project-activity__actor relative text-left text-base font-normal text-[var(--text)] leading-[1.6] px-2.5 py-2 rounded-md transition-colors overflow-hidden bg-[var(--accent-subtle)] border-0"}else{"native-project-activity__actor relative text-left text-base font-normal text-[var(--text)] leading-[1.6] px-2.5 py-2 rounded-md transition-colors overflow-hidden bg-transparent hover:bg-[var(--bg-subtle)] border-0"};let avatar_class=if bot {"size-6 rounded-full flex items-center justify-center text-micro font-bold shrink-0 select-none bg-[var(--accent-subtle)] text-[var(--accent)] border border-solid border-[var(--accent)]"}else{"size-6 rounded-full flex items-center justify-center text-micro font-bold shrink-0 select-none bg-[var(--accent)] text-[var(--accent-text)]"};view! {
        cx =>
        <button
            type="button"
            class=(class)
            title=(title)
            data-activity-actor=(key.map_or_else(
                || "system".to_owned(),
                |id| id.to_string(),
            ))
            @click=$(|_event: Event| {
                if active {
                    filter.set(raw!("cx.none()", None::<Option<i64>>));
                } else {
                    filter.set(raw!("cx.some(${key})", Some(key)));
                }
                expanded.set(raw!("cx.none()", None::<i64>));
            })
        >
            <div class="flex items-center gap-2.5">
                <span class=(avatar_class)>(initials)</span>
                <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-1.5">
                        <span
                            class="text-body-sm text-[var(--text)] truncate font-medium"
                        >
                            (name)
                        </span>
                        if bot {
                            <span
                                class="text-micro font-semibold uppercase tracking-wider px-1 py-px rounded bg-[var(--accent-subtle)] text-[var(--accent)] shrink-0"
                            >
                                "agent"
                            </span>
                        }
                    </div>
                    <div class="text-micro text-[var(--text-faint)]">
                        "via "
                        (transport)
                        " · "
                        (seen)
                    </div>
                </div>
                <span
                    class="text-caption text-[var(--text-muted)] tabular-nums shrink-0"
                >
                    (actions)
                </span>
            </div>
            <span
                class="absolute bottom-0 left-2.5 h-[2px] rounded-full bg-[var(--accent)] opacity-30"
                style=(width)
                aria-hidden="true"
            ></span>
        </button>
    }.boxed()}).collect::<Vec<_>>();
    view! {
        cx =>
        <aside
            class="native-project-activity__actors w-full lg:w-[260px] shrink-0 lg:sticky lg:top-6"
        >
            <div class="flex items-center gap-2 mb-3">
                <span
                    class="text-micro font-semibold uppercase tracking-widest text-[var(--text-muted)]"
                >
                    "Actors"
                </span>
                <span class="text-micro text-[var(--text-faint)] tabular-nums">
                    (total)
                </span>
            </div>
            <div class="flex flex-col gap-0.5">
                for row in rows {
                    (row)
                }
            </div>
        </aside>
    }.boxed()
}

#[cfg(test)]
mod tests {
    use topcoat::{context::CxTestBuilder, view::ViewExt};

    use super::*;

    #[tokio::test]
    async fn missing_and_unknown_statuses_render_main_faint_circle() {
        let cx = CxTestBuilder::new().build();
        for value in [None, Some(""), Some("unknown")] {
            let html = status(&cx, value).single().await.unwrap().render(&cx);
            assert!(html.contains("data-icon=\"Circle\""), "{value:?}: {html}");
            assert!(html.contains("width=\"12\""), "{value:?}: {html}");
            assert!(html.contains("var(--tc-faint)"), "{value:?}: {html}");
        }
    }
    fn event(id: i64, ts: &str, actor: Option<i64>) -> Activity {
        Activity {
            id,
            ts: ts.into(),
            actor_user_id: actor,
            actor_username: None,
            actor_display_name: None,
            actor_is_bot: false,
            transport: "web".into(),
            entity_type: "issue".into(),
            entity_id: 7,
            entity_label: Some("ACC-7".into()),
            project_id: Some(1),
            issue_id: Some(7),
            page_id: None,
            action: "create".into(),
            field: None,
            old_value: None,
            new_value: None,
        }
    }
    #[test]
    fn filters_distinguish_everyone_system_and_user_without_reordering_rows() {
        let items = [
            event(1, "2026-10-06 00:00:00", None),
            event(2, "2026-10-06 00:00:00", Some(4)),
        ];
        assert_eq!(
            items
                .iter()
                .filter(|item| matches_actor(item, None))
                .count(),
            2
        );
        assert_eq!(
            items
                .iter()
                .filter(|item| matches_actor(item, Some(None)))
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(
            items
                .iter()
                .filter(|item| matches_actor(item, Some(Some(4))))
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            vec![2]
        );
    }
    #[test]
    fn day_groups_use_local_dates_and_only_group_consecutive_rows() {
        let items = [
            event(1, "2026-10-06 01:00:00", None),
            event(2, "2026-10-05 20:00:00", None),
            event(3, "2026-10-05 01:00:00", None),
        ];
        let epoch = NaiveDateTime::parse_from_str("2026-10-06 18:00:00", "%Y-%m-%d %H:%M:%S")
            .unwrap()
            .and_utc()
            .timestamp_millis() as f64;
        let groups = day_groups(&items, None, epoch, chrono_tz::America::Denver);
        assert_eq!(
            groups
                .iter()
                .map(|(label, rows)| (label.as_str(), rows.len()))
                .collect::<Vec<_>>(),
            vec![("Yesterday", 2), ("Sun, Oct 4", 1)]
        );
        assert_eq!(
            day_label(
                NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
                NaiveDate::from_ymd_opt(2026, 10, 6).unwrap(),
                NaiveDate::from_ymd_opt(2026, 10, 5).unwrap()
            ),
            "Wed, Dec 31, 2025"
        );
    }
    #[test]
    fn day_groups_apply_historical_denver_offsets_to_each_entry() {
        let items = [
            event(1, "2026-07-01 06:30:00", None),
            event(2, "2026-01-01 06:30:00", None),
        ];
        let epoch = NaiveDateTime::parse_from_str("2026-07-02 18:00:00", "%Y-%m-%d %H:%M:%S")
            .unwrap()
            .and_utc()
            .timestamp_millis() as f64;
        let groups = day_groups(&items, None, epoch, chrono_tz::America::Denver);
        assert_eq!(
            groups
                .iter()
                .map(|(label, _)| label.as_str())
                .collect::<Vec<_>>(),
            vec!["Yesterday", "Wed, Dec 31, 2025"]
        );
    }
    #[test]
    fn yesterday_uses_elapsed_twenty_four_hours_across_fall_transition() {
        let items = [
            event(1, "2026-11-02 06:00:00", None),
            event(2, "2026-11-01 05:30:00", None),
        ];
        let epoch = NaiveDateTime::parse_from_str("2026-11-02 06:30:00", "%Y-%m-%d %H:%M:%S")
            .unwrap()
            .and_utc()
            .timestamp_millis() as f64;
        let groups = day_groups(&items, None, epoch, chrono_tz::America::Denver);
        assert_eq!(
            groups
                .iter()
                .map(|(label, _)| label.as_str())
                .collect::<Vec<_>>(),
            vec!["Today", "Sat, Oct 31"]
        );
    }
    #[test]
    fn verbs_destinations_and_rank_suffixes_preserve_project_specific_contract() {
        let mut item = event(1, "2026-10-06 01:00:00", None);
        item.entity_type = "plan_step".into();
        item.action = "create".into();
        assert_eq!(verb(&item), "created plan_step");
        assert_eq!(destination("ACC", &item), None);
        item.entity_type = "comment".into();
        assert_eq!(verb(&item), "commented on");
        assert_eq!(destination("ACC", &item), Some("/ACC/issues/ACC-7".into()));
        item.entity_label = Some(String::new());
        item.page_id = Some(9);
        assert_eq!(destination("ACC", &item), Some("/ACC/pages/9".into()));
        item.action = "link".into();
        item.field = Some("blocks_then_links".into());
        assert_eq!(verb(&item), "linked blocks then_links");
        assert_eq!(
            [1, 2, 3, 11, 12, 13, 21, 111].map(ordinal),
            ["1st", "2nd", "3rd", "11th", "12th", "13th", "21st", "111th"]
        );
    }
}
