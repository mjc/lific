//! Shared issue rows and board cards, selected by the Rust model.
use super::super::{icons, issue_peek, navigation};
use super::{data::Collection, model::Selection};
use crate::db::models::{Issue, Status};
use topcoat::{
    context::Cx,
    view::{Attributes, BoxView, ViewExt, view},
};

#[derive(Clone, Copy)]
pub(crate) enum Audience<'a> {
    Private,
    Published(&'a str),
}

#[derive(Clone, Copy)]
pub(crate) enum Disclosure<'a> {
    Group { kind: &'a str, key: &'a str },
    Lane { key: &'a str },
    Column(Status),
}

type DisclosureAttributes<'a> = &'a dyn Fn(Disclosure<'_>) -> Attributes;

pub(crate) fn region<'a>(
    cx: &'a Cx,
    collection: &Collection,
    selection: &Selection,
    clear_filters: Attributes,
    audience: Audience<'_>,
) -> BoxView<'a> {
    region_with_disclosures(cx, collection, selection, clear_filters, audience, None)
}

pub(crate) fn region_with_disclosures<'a>(
    cx: &'a Cx,
    collection: &Collection,
    selection: &Selection,
    clear_filters: Attributes,
    audience: Audience<'_>,
    disclosures: Option<DisclosureAttributes<'_>>,
) -> BoxView<'a> {
    if selection.layout == "board" {
        return board(cx, collection, selection, audience, disclosures);
    }
    let project = collection.project.identifier.clone();
    let body = if selection.issues.is_empty() {
        empty(cx, selection.empty_filtered, clear_filters, audience)
    } else if let Some(groups) = &selection.groups {
        let groups = groups.iter().map(|group| {
            let key = group.key.clone(); let label = group.label.clone(); let collapsed = group.collapsed;
            let label_class = if group.kind == "module" { "font-semibold text-caption" } else { "font-semibold text-caption capitalize" };
            let count = group.issues.len().to_string();
            let rows = list_rows(cx, &project, &group.issues, selection, audience);
            let attributes = disclosures.map(|disclose| disclose(Disclosure::Group { kind: &group.kind, key: &key }));
            let heading = disclosure_heading(cx, label, label_class, count, collapsed, attributes,
                "sticky top-0 z-10 flex items-center gap-2 px-6 py-2 bg-[var(--surface)] border-b border-solid border-[var(--border)]");
            view! {
                cx =>
                <section
                    data-native-issue-group=(key)
                    data-native-group-collapsed=(collapsed.to_string())
                >
                    (heading)
                    if !collapsed {
                        (rows)
                    }
                </section>
            }.boxed()
        }).collect::<Vec<_>>();
        view! {
            cx =>
            for group in groups {
                (group)
            }
        }
        .boxed()
    } else {
        list_rows(cx, &project, &selection.issues, selection, audience)
    };
    let capped = selection.show_search_cap;
    let count = selection.count_label.clone();
    view! {
        cx =>
        <div
            data-native-issue-list=(project)
            data-native-issue-count=(count.clone())
            class="native-issue-list min-w-0 min-h-0 overflow-auto"
        >
            <div
                class="px-6 py-2 text-caption text-[var(--text-faint)]"
                aria-label="Issue count"
            >
                (count)
            </div>
            if capped {
                <div
                    class="text-micro text-[var(--text-faint)] uppercase tracking-widest font-semibold px-6 py-2 border-b border-solid border-[var(--border)] bg-[var(--surface)]"
                >
                    "Top 50 matches — narrow the query for fewer results"
                </div>
            }
            (body)
        </div>
    }.boxed()
}

fn disclosure_heading<'a>(
    cx: &'a Cx,
    label: String,
    label_class: &'static str,
    count: String,
    collapsed: bool,
    attributes: Option<Attributes>,
    class: &'static str,
) -> BoxView<'a> {
    let caption = view! {
        cx =>
        <span class=(label_class)>(label)</span>
        <span class="text-micro text-[var(--text-faint)]">(count)</span>
    }
    .boxed();
    if let Some(attributes) = attributes {
        view! {
            cx =>
            <button
                type="button"
                class=(format!("{class} w-full text-left hover:bg-[var(--bg-subtle)]"))
                aria-expanded=((!collapsed).to_string())
                (attributes)
            >
                (icons::ui_icon(
                    cx,
                    if collapsed { icons::UiIcon::Next } else { icons::UiIcon::Expand },
                    13,
                ))
                (caption)
            </button>
        }
        .boxed()
    } else {
        view! { cx => <div class=(class)>(caption)</div> }.boxed()
    }
}

fn empty<'a>(
    cx: &'a Cx,
    filtered: bool,
    clear_filters: Attributes,
    audience: Audience<'_>,
) -> BoxView<'a> {
    let show_clear = matches!(audience, Audience::Private);
    view! {
        cx =>
        <div class="flex flex-col items-center justify-center py-20 gap-3 text-center">
            if filtered {
                <p class="text-[var(--text-muted)] text-body-lg">
                    "No issues match your filters"
                </p>
                if show_clear {
                    <button
                        type="button"
                        class="text-body-sm text-[var(--accent)] hover:underline border-0 bg-transparent"
                        (clear_filters)
                    >
                        "Clear filters"
                    </button>
                }
            } else {
                <p class="text-[var(--text)] text-heading font-medium m-0">
                    "All quiet here"
                </p>
                <p class="text-[var(--text-muted)] text-body m-0">
                    "No work on the board. Time for a nap… or a fresh idea."
                </p>
            }
        </div>
    }.boxed()
}

fn list_rows<'a>(
    cx: &'a Cx,
    project: &str,
    issues: &[Issue],
    selection: &Selection,
    audience: Audience<'_>,
) -> BoxView<'a> {
    let rows = issues.iter().map(|issue| {
        let id = issue.id.to_string(); let status = issue.status; let priority = issue.priority;
        let identifier = issue.identifier.clone(); let title = issue.title.clone();
        let href = navigation::attrs(cx, &issue_href(project, &identifier, audience));
        let peek = match audience { Audience::Private => Some(issue_peek::button(cx, &identifier)), Audience::Published(_) => None };
        let preview = if let Some(snippet) = selection.snippets.get(&issue.id) { snippet.clone() }
            else if selection.density == "comfortable" { description_preview(&issue.description) }
            else { String::new() };
        let labels = issue.labels.clone();
        view! {
            cx =>
            <li data-native-issue-row=(id) class="group flex items-center">
                <a class="native-issue-list__row flex-1 min-w-0" (href)>
                    (icons::status_icon(cx, status, 16))
                    <span class="native-issue-list__identifier">(identifier)</span>
                    <span class="flex-1 min-w-0">
                        <span class="native-issue-list__title block">(title)</span>
                        if !preview.is_empty() {
                            <span
                                class="block truncate text-caption text-[var(--text-faint)]"
                            >
                                (preview)
                            </span>
                        }
                    </span>
                    for label in labels {
                        <span
                            class="hidden md:inline text-micro px-1.5 py-0.5 rounded bg-[var(--bg-subtle)] text-[var(--text-muted)]"
                        >
                            (label)
                        </span>
                    }
                    (icons::priority_icon(cx, priority, 21))
                </a>
                if let Some(peek) = peek {
                    <span class="mr-3">(peek)</span>
                }
            </li>
        }.boxed()
    }).collect::<Vec<_>>();
    view! {
        cx =>
        <ul class="native-issue-list__rows" aria-label="Issues">
            for row in rows {
                (row)
            }
        </ul>
    }
    .boxed()
}

fn description_preview(description: &str) -> String {
    let line = description
        .lines()
        .find(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .unwrap_or("");
    let stripped = line
        .chars()
        .filter(|character| !"*_`>[]".contains(*character))
        .collect::<String>();
    String::from_utf16_lossy(&stripped.trim().encode_utf16().take(160).collect::<Vec<_>>())
}

fn board<'a>(
    cx: &'a Cx,
    collection: &Collection,
    selection: &Selection,
    audience: Audience<'_>,
    disclosures: Option<DisclosureAttributes<'_>>,
) -> BoxView<'a> {
    let project = collection.project.identifier.clone();
    let lanes = if let Some(lanes) = &selection.lanes {
        lanes.iter().map(|lane| {
            let key = lane.key.clone(); let label = lane.label.clone(); let collapsed = lane.collapsed;
            let label_class = if lane.kind == "priority" { "text-caption font-semibold capitalize" } else { "text-caption font-semibold" };
            let count = lane.issues.len().to_string(); let columns = columns(cx, &project, &lane.issues, selection, audience, disclosures);
            let heading = if let Some(disclose) = disclosures {
                let attributes = disclose(Disclosure::Lane { key: &key });
                disclosure_heading(cx, label, label_class, count, collapsed, Some(attributes),
                    "flex items-center gap-2 px-4 py-2 border-b border-solid border-[var(--border)] bg-[var(--bg-subtle)]")
            } else {
                view! {
                    cx =>
                    <header
                        class="flex items-center gap-2 px-4 py-2 border-b border-solid border-[var(--border)] bg-[var(--bg-subtle)]"
                    >
                        <span class=(label_class)>(label)</span>
                        <span class="text-micro text-[var(--text-faint)]">(count)</span>
                    </header>
                }.boxed()
            };
            view! {
                cx =>
                <section
                    data-native-board-lane=(key)
                    data-native-lane-collapsed=(collapsed.to_string())
                    class="flex flex-col min-h-0"
                >
                    (heading)
                    if !collapsed {
                        (columns)
                    }
                </section>
            }.boxed()
        }).collect::<Vec<_>>()
    } else {
        vec![columns(
            cx,
            &project,
            &selection.issues,
            selection,
            audience,
            disclosures,
        )]
    };
    let count = selection.count_label.clone();
    view! {
        cx =>
        <section
            class="native-board flex-col overflow-auto"
            data-native-board=(project)
            data-native-issue-count=(count.clone())
            aria-label="Issue board"
        >
            <div
                class="px-4 py-2 text-caption text-[var(--text-faint)]"
                aria-label="Issue count"
            >
                (count)
            </div>
            for lane in lanes {
                (lane)
            }
        </section>
    }
    .boxed()
}

fn columns<'a>(
    cx: &'a Cx,
    project: &str,
    issues: &[Issue],
    selection: &Selection,
    audience: Audience<'_>,
    disclosures: Option<DisclosureAttributes<'_>>,
) -> BoxView<'a> {
    let columns = selection
        .visible_statuses
        .iter()
        .map(|status| {
            let status = *status;
            let cards = issues
                .iter()
                .filter(|issue| issue.status == status)
                .map(|issue| {
                    let id = issue.id.to_string();
                    let identifier = issue.identifier.clone();
                    let title = issue.title.clone();
                    let priority = issue.priority;
                    let href = navigation::attrs(cx, &issue_href(project, &identifier, audience));
                    let peek = match audience {
                        Audience::Private => Some(issue_peek::button(cx, &identifier)),
                        Audience::Published(_) => None,
                    };
                    let title_class = if matches!(status, Status::Done | Status::Cancelled) {
                        "native-board__title native-board__title--closed"
                    } else {
                        "native-board__title"
                    };
                    view! {
                        cx =>
                        <div class="relative group">
                            <a
                                class="native-board__card"
                                data-native-board-card=(id)
                                (href)
                            >
                                <div class="native-board__card-top">
                                    <span class="native-board__identifier">(identifier)</span>
                                    (icons::priority_icon(cx, priority, 14))
                                </div>
                                <h3 class=(title_class)>(title)</h3>
                            </a>
                            if let Some(peek) = peek {
                                <span class="absolute right-8 top-2">(peek)</span>
                            }
                        </div>
                    }
                    .boxed()
                })
                .collect::<Vec<_>>();
            let count = cards.len().to_string();
            let collapsed = selection
                .collapsed_columns
                .iter()
                .any(|column| column == status.as_str());
            let collapse_control = disclosures.map(|disclose| {
                let attributes = disclose(Disclosure::Column(status));
                let title = if collapsed { "Expand column" } else { "Collapse column" };
                view! {
                    cx =>
                    <button
                        type="button"
                        class="ml-auto flex size-5 shrink-0 items-center justify-center rounded text-[var(--text-faint)] hover:bg-[var(--bg-subtle)] hover:text-[var(--text)]"
                        aria-label=(format!("{title}: {}", status.as_str()))
                        title=(title)
                        aria-expanded=((!collapsed).to_string())
                        (attributes)
                    >
                        (icons::ui_icon(
                            cx,
                            if collapsed {
                                icons::UiIcon::ExpandColumn
                            } else {
                                icons::UiIcon::CollapseColumn
                            },
                            12,
                        ))
                    </button>
                }.boxed()
            });
            view! {
                cx =>
                <section
                    class=(if collapsed {
                        "native-board__column !basis-12"
                    } else {
                        "native-board__column"
                    })
                    data-native-board-status=(status.as_str())
                    data-native-column-collapsed=(collapsed.to_string())
                    aria-label=(status.as_str())
                >
                    <header class="native-board__header">
                        (icons::status_icon(cx, status, 14))
                        if !collapsed {
                            <h2>(status.as_str())</h2>
                        }
                        <span
                            class="native-board__count"
                            data-native-board-count=(count.clone())
                        >
                            (count)
                        </span>
                        if let Some(collapse_control) = collapse_control {
                            (collapse_control)
                        }
                    </header>
                    if !collapsed {
                        <div class="native-board__cards">
                            if cards.is_empty() {
                                <p class="native-board__empty">"All quiet"</p>
                            }
                            for card in cards {
                                (card)
                            }
                        </div>
                    }
                </section>
            }
            .boxed()
        })
        .collect::<Vec<_>>();
    view! {
        cx =>
        <div class="native-board__columns">
            for column in columns {
                (column)
            }
        </div>
    }
    .boxed()
}

fn issue_href(project: &str, identifier: &str, audience: Audience<'_>) -> String {
    match audience {
        Audience::Private => format!("/{project}/issues/{identifier}"),
        Audience::Published(project) => format!("/public/{project}/issues/{identifier}"),
    }
}
