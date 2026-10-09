//! Read-only public page browsing, backed by a fresh published-project read on
//! every shard render.

use std::collections::HashSet;

use super::super::super::runtime::signal_vec::{SignalVecExt, VecPositionExt};
use super::super::{browser, public_route::Route};
use super::{data, data::Body};
use crate::db::models::{Folder, Page, Project};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard, signal},
    view::{BoxView, View, ViewExt, view},
};

const DEFAULT_STATUS: &str = "__active";
const SEARCH_RESULT_CAP: usize = 50;

#[derive(Clone)]
struct Filters {
    query: String,
    label: String,
    status: String,
    focus_folder: i64,
    expanded: Vec<i64>,
}

pub(super) fn content<'a>(cx: &'a Cx, project: &Project, folders: &[Folder]) -> BoxView<'a> {
    let project_identifier = project.identifier.clone();
    let title = project.name.clone();
    let owner = cx.keyed(("public-pages", project.id));
    let tab = signal(&owner, || "browse".to_owned());
    let query = signal(&owner, String::new);
    let label = signal(&owner, String::new);
    let status = signal(&owner, || DEFAULT_STATUS.to_owned());
    let focus_folder = signal(&owner, || 0_i64);
    let initially_expanded = folders.iter().map(|folder| folder.id).collect::<Vec<_>>();
    let expanded = signal(&owner, move || initially_expanded);
    let shard_project = project_identifier;
    let shard_tab = tab.clone();

    let tab_browse = tab.clone();
    let browse_click = expr!(|_event: Event| tab_browse.set("browse".to_owned()));
    let tab_recent = tab.clone();
    let recent_click = expr!(|_event: Event| tab_recent.set("recent".to_owned()));
    let tab_drafts = tab.clone();
    let drafts_click = expr!(|_event: Event| tab_drafts.set("drafts".to_owned()));
    let tab_archived = tab.clone();
    let archived_click = expr!(|_event: Event| tab_archived.set("archived".to_owned()));

    let body = view! {
        owner =>
        public_pages_rows(
            project: shard_project,
            tab: $(shard_tab.get()),
            query: query,
            label: label,
            status: status,
            focus_folder: focus_folder,
            expanded: expanded
        )
    }
    .boxed();

    view! {
        owner =>
        <main
            class="mx-auto flex min-h-0 w-full max-w-7xl flex-col gap-4 px-4 py-6 md:px-8"
            data-public-pages=""
        >
            <header class="flex flex-wrap items-center justify-between gap-3">
                <h1 class="m-0 text-heading font-semibold">
                    (title)
                    " pages"
                </h1>
                <nav
                    class="flex flex-wrap gap-1 rounded-lg border border-solid border-[var(--border)] p-1"
                    aria-label="Page views"
                >
                    <button
                        type="button"
                        :aria-pressed=$(tab.get() == "browse")
                        class="rounded-md px-3 py-2 text-body-sm hover:bg-[var(--bg-subtle)]"
                        data-public-page-tab="browse"
                        data-topcoat-on:click=(browse_click.into_evaluated_and_js().1)
                    >
                        "Browse"
                    </button>
                    <button
                        type="button"
                        :aria-pressed=$(tab.get() == "recent")
                        class="rounded-md px-3 py-2 text-body-sm hover:bg-[var(--bg-subtle)]"
                        data-public-page-tab="recent"
                        data-topcoat-on:click=(recent_click.into_evaluated_and_js().1)
                    >
                        "Recent"
                    </button>
                    <button
                        type="button"
                        :aria-pressed=$(tab.get() == "drafts")
                        class="rounded-md px-3 py-2 text-body-sm hover:bg-[var(--bg-subtle)]"
                        data-public-page-tab="drafts"
                        data-topcoat-on:click=(drafts_click.into_evaluated_and_js().1)
                    >
                        "Drafts"
                    </button>
                    <button
                        type="button"
                        :aria-pressed=$(tab.get() == "archived")
                        class="rounded-md px-3 py-2 text-body-sm hover:bg-[var(--bg-subtle)]"
                        data-public-page-tab="archived"
                        data-topcoat-on:click=(archived_click.into_evaluated_and_js().1)
                    >
                        "Archived"
                    </button>
                </nav>
            </header>
            (body)
        </main>
    }
    .boxed()
}

use rows_shard::public_pages_rows;

#[allow(
    clippy::too_many_arguments,
    reason = "Topcoat adds request context to separate reactive page filters"
)]
mod rows_shard {
    use super::*;

    #[shard("/public/__native/pages")]
    pub(super) async fn public_pages_rows(
        cx: &Cx,
        project: String,
        tab: String,
        query: Signal<String>,
        label: Signal<String>,
        status: Signal<String>,
        focus_folder: Signal<i64>,
        expanded: Signal<Vec<i64>>,
    ) -> topcoat::Result<impl View> {
        let route = Route::Pages { project };
        let snapshot = super::super::super::session::read(cx, data::load(cx, &route))?;
        let Body::Pages { pages, folders } = snapshot.body else {
            return Err(crate::error::LificError::NotFound("not found".into()).into());
        };
        let project = snapshot.project.identifier.as_str();
        let filters = Filters {
            query: query.get(),
            label: label.get(),
            status: status.get(),
            focus_folder: focus_folder.get(),
            expanded: expanded.get(),
        };
        Ok(render(
            cx,
            project,
            &pages,
            &folders,
            &tab,
            &filters,
            query,
            label,
            status,
            focus_folder,
            expanded,
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn render<'a>(
    cx: &'a Cx,
    project: &str,
    pages: &[Page],
    folders: &[Folder],
    tab: &str,
    filters: &Filters,
    query: Signal<String>,
    label: Signal<String>,
    status: Signal<String>,
    focus_folder: Signal<i64>,
    expanded: Signal<Vec<i64>>,
) -> BoxView<'a> {
    let query_input = query.clone();
    let query_change = expr!(|event: Event| query_input.set(event.target.value.to_owned()));
    let label_input = label.clone();
    let label_change = expr!(|event: Event| label_input.set(event.target.value.to_owned()));
    let status_input = status.clone();
    let status_change = expr!(|event: Event| status_input.set(event.target.value.to_owned()));
    let folder_input = focus_folder.clone();
    let browser = browser::bindings();
    let folder_change = expr!(|event: Event| {
        folder_input.set(browser.positive_i64(event.target.value, 0_i64));
    });
    let clear_query = query.clone();
    let clear_label = label.clone();
    let clear_status = status.clone();
    let clear_folder = focus_folder.clone();
    let clear = expr!(|_event: Event| {
        clear_query.set("".to_owned());
        clear_label.set("".to_owned());
        clear_status.set(DEFAULT_STATUS.to_owned());
        clear_folder.set(0_i64);
    });

    let mut label_names = pages
        .iter()
        .flat_map(|page| page.labels.iter().cloned())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    label_names.sort();
    let folder_options = folders
        .iter()
        .map(|folder| {
            let value = folder.id.to_string();
            let name = folder.name.clone();
            view! { cx => <option value=(value)>(name)</option> }.boxed()
        })
        .collect::<Vec<_>>();
    let tab = tab.to_owned();
    let rows = match tab.as_str() {
        "recent" => sorted_recent(pages)
            .into_iter()
            .take(20)
            .map(|page| page_row(cx, project, page, 0, None))
            .collect::<Vec<_>>(),
        "drafts" => sorted_updated(pages.iter().filter(|page| page.status == "draft"))
            .into_iter()
            .map(|page| page_row(cx, project, page, 0, None))
            .collect(),
        "archived" => sorted_updated(pages.iter().filter(|page| page.status == "archived"))
            .into_iter()
            .map(|page| page_row(cx, project, page, 0, None))
            .collect(),
        _ => browse(cx, project, pages, folders, filters, expanded),
    };
    let is_browse = tab == "browse";
    let tab_attr = tab.clone();
    let tab_label = match tab.as_str() {
        "recent" => "Recent",
        "drafts" => "Drafts",
        "archived" => "Archived",
        _ => "Browse",
    };
    view! {
        cx =>
        <section
            class="flex min-h-0 flex-col gap-4"
            data-public-page-results=(tab_attr)
        >
            if is_browse {
                <div
                    class="flex flex-wrap items-end gap-3 rounded-lg border border-solid border-[var(--border)] p-3"
                >
                    <label class="flex min-w-48 flex-1 flex-col gap-1 text-caption">
                        "Search pages"
                        <input
                            type="search"
                            autocomplete="off"
                            placeholder="Title, identifier, or content"
                            :value=$(query.get())
                            class="rounded-md border border-solid border-[var(--border)] bg-[var(--surface)] px-3 py-2"
                            data-topcoat-on:input=(query_change.into_evaluated_and_js().1)
                        />
                    </label>
                    <label class="flex min-w-36 flex-col gap-1 text-caption">
                        "Label"
                        <select
                            :value=$(label.get())
                            class="rounded-md border border-solid border-[var(--border)] bg-[var(--surface)] px-2 py-2"
                            data-topcoat-on:change=(label_change.into_evaluated_and_js().1)
                        >
                            <option value="">"All labels"</option>
                            for item in label_names {
                                <option value=(item.clone())>(item)</option>
                            }
                        </select>
                    </label>
                    <label class="flex min-w-36 flex-col gap-1 text-caption">
                        "Status"
                        <select
                            :value=$(status.get())
                            class="rounded-md border border-solid border-[var(--border)] bg-[var(--surface)] px-2 py-2"
                            data-topcoat-on:change=(status_change.into_evaluated_and_js().1)
                        >
                            <option value="__active">"Active (hide archived)"</option>
                            <option value="">"All statuses"</option>
                            <option value="draft">"Draft"</option>
                            <option value="active">"Active"</option>
                            <option value="complete">"Complete"</option>
                            <option value="archived">"Archived"</option>
                        </select>
                    </label>
                    <label class="flex min-w-40 flex-col gap-1 text-caption">
                        "Folder focus"
                        <select
                            :value=$(focus_folder.get())
                            class="rounded-md border border-solid border-[var(--border)] bg-[var(--surface)] px-2 py-2"
                            data-topcoat-on:change=(folder_change.into_evaluated_and_js().1)
                        >
                            <option value="0">"All folders"</option>
                            for option in folder_options {
                                (option)
                            }
                        </select>
                    </label>
                    <button
                        type="button"
                        class="rounded-md px-3 py-2 text-caption text-[var(--accent)] hover:bg-[var(--bg-subtle)]"
                        data-topcoat-on:click=(clear.into_evaluated_and_js().1)
                    >
                        "Clear filters"
                    </button>
                </div>
            }
            <h2 class="sr-only">(tab_label)</h2>
            if rows.is_empty() {
                <p
                    class="rounded-lg border border-dashed border-[var(--border)] px-4 py-8 text-center text-body-sm text-[var(--text-muted)]"
                >
                    "No pages match these filters."
                </p>
            } else {
                <ul
                    class="m-0 list-none divide-y divide-[var(--border)] rounded-lg border border-solid border-[var(--border)] p-0"
                >
                    for row in rows {
                        (row)
                    }
                </ul>
            }
        </section>
    }
    .boxed()
}

fn browse<'a>(
    cx: &'a Cx,
    project: &str,
    pages: &[Page],
    folders: &[Folder],
    filters: &Filters,
    expanded: Signal<Vec<i64>>,
) -> Vec<BoxView<'a>> {
    let query = filters.query.trim();
    if !query.is_empty() {
        let candidates = search_candidates(pages, filters);
        return search_rows(cx, project, &candidates, query);
    }
    let visible = pages
        .iter()
        .filter(|page| matches_filters(page, filters, folders))
        .collect::<Vec<_>>();

    let mut rows = Vec::new();
    let mut pinned = visible
        .iter()
        .copied()
        .filter(|page| page.pinned)
        .collect::<Vec<_>>();
    pinned.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    if !pinned.is_empty() {
        rows.push(section_label(cx, "Pinned"));
        rows.extend(
            pinned
                .into_iter()
                .map(|page| page_row(cx, project, page, 0, None)),
        );
    }
    let focused = (filters.focus_folder > 0).then_some(filters.focus_folder);
    folder_tree(
        cx, project, pages, folders, focused, None, 0, filters, expanded, &mut rows,
    );
    if focused.is_none() {
        for page in sorted(
            visible
                .iter()
                .copied()
                .filter(|page| page.folder_id.is_none()),
        ) {
            rows.push(page_row(cx, project, page, 0, None));
        }
    }
    rows
}

fn search_candidates<'a>(pages: &'a [Page], filters: &Filters) -> Vec<&'a Page> {
    pages
        .iter()
        .filter(|page| {
            let matches_label =
                filters.label.is_empty() || page.labels.iter().any(|label| label == &filters.label);
            let matches_status = filters.status == DEFAULT_STATUS
                || filters.status.is_empty()
                || page.status == filters.status;
            matches_label && matches_status
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn folder_tree<'a>(
    cx: &'a Cx,
    project: &str,
    pages: &[Page],
    folders: &[Folder],
    focused: Option<i64>,
    parent: Option<i64>,
    depth: usize,
    filters: &Filters,
    expanded: Signal<Vec<i64>>,
    out: &mut Vec<BoxView<'a>>,
) {
    if depth > 24 {
        return;
    }
    let mut children = folders
        .iter()
        .filter(|folder| folder.parent_id == parent)
        .collect::<Vec<_>>();
    children.sort_by(|left, right| left.name.cmp(&right.name));
    for folder in children {
        if focused.is_some_and(|focus| {
            !folder_contains(folders, folder.id, focus)
                && !folder_contains(folders, focus, folder.id)
        }) {
            continue;
        }
        let folder_id = folder.id;
        let is_expanded = expanded.get().contains(&folder_id);
        let current = expanded.clone();
        let click = expr!(|_event: Event| {
            let index = current.get().position(folder_id);
            if index.is_some() {
                current.remove(index.unwrap());
            } else {
                current.push(folder_id);
            }
        });
        let folder_name = folder.name.clone();
        let indent = format!("padding-left: {}px", depth * 18);
        let child_count = pages
            .iter()
            .filter(|page| {
                page.folder_id == Some(folder_id) && matches_filters(page, filters, folders)
            })
            .count();
        out.push(
            view! {
                cx =>
                <li class="list-none">
                    <button
                        type="button"
                        style=(indent)
                        :aria-expanded=$(if is_expanded { "true" } else { "false" })
                        :aria-label=$(if is_expanded {
                            "Collapse folder"
                        } else {
                            "Expand folder"
                        })
                        class="flex w-full items-center gap-2 rounded-md px-2 py-2 text-left hover:bg-[var(--bg-subtle)]"
                        data-public-folder=(folder_id.to_string())
                        data-topcoat-on:click=(click.into_evaluated_and_js().1)
                    >
                        <span aria-hidden="true" class="text-[var(--text-faint)]">
                            $(if is_expanded { "▾" } else { "▸" })
                        </span>
                        <span class="min-w-0 flex-1 truncate font-medium">
                            (folder_name)
                        </span>
                        <span class="text-caption text-[var(--text-faint)]">
                            (child_count)
                        </span>
                    </button>
                </li>
            }
            .boxed(),
        );
        if !is_expanded {
            continue;
        }
        for page in sorted(pages.iter().filter(|page| {
            page.folder_id == Some(folder_id) && matches_filters(page, filters, folders)
        })) {
            out.push(page_row(cx, project, page, (depth + 1) * 18, None));
        }
        folder_tree(
            cx,
            project,
            pages,
            folders,
            focused,
            Some(folder_id),
            depth + 1,
            filters,
            expanded.clone(),
            out,
        );
    }
}

fn folder_contains(folders: &[Folder], folder_id: i64, ancestor_id: i64) -> bool {
    let mut current = Some(folder_id);
    let mut depth = 0;
    while let Some(id) = current {
        if id == ancestor_id {
            return true;
        }
        current = folders
            .iter()
            .find(|folder| folder.id == id)
            .and_then(|folder| folder.parent_id);
        depth += 1;
        if depth > 24 {
            return false;
        }
    }
    false
}

fn matches_filters(page: &Page, filters: &Filters, folders: &[Folder]) -> bool {
    let matches_label =
        filters.label.is_empty() || page.labels.iter().any(|label| label == &filters.label);
    let matches_status = if filters.status == DEFAULT_STATUS {
        page.status != "archived"
    } else {
        filters.status.is_empty() || page.status == filters.status
    };
    let matches_focus = filters.focus_folder <= 0
        || page
            .folder_id
            .is_some_and(|folder_id| folder_contains(folders, folder_id, filters.focus_folder));
    matches_label && matches_status && matches_focus
}

fn search_rows<'a>(cx: &'a Cx, project: &str, pages: &[&Page], query: &str) -> Vec<BoxView<'a>> {
    let mut hits = pages
        .iter()
        .filter_map(|page| {
            let (score, snippet) = score_page(page, query);
            (score >= 0.25).then_some((score, *page, snippet))
        })
        .collect::<Vec<_>>();
    hits.sort_by(|left, right| right.0.total_cmp(&left.0));
    hits.truncate(SEARCH_RESULT_CAP);
    hits.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.identifier.cmp(&right.1.identifier))
    });
    hits.into_iter()
        .map(|(_, page, snippet)| page_row(cx, project, page, 0, snippet.as_deref()))
        .collect()
}

fn score_page(page: &Page, query: &str) -> (f64, Option<String>) {
    let title = super::super::fuzzy::find(query, &page.title);
    let identifier = super::super::fuzzy::find(query, &page.identifier);
    let content = crate::db::queries::changes::preview_of(&page.content);
    let body = super::super::fuzzy::find(query, &content);
    let labels = page.labels.join(" ");
    let label = super::super::fuzzy::find(query, &labels).map_or(0.0, |hit| hit.score * 0.55);
    let score = title
        .map_or(0.0, |hit| hit.score)
        .max(identifier.map_or(0.0, |hit| hit.score * 0.9))
        .max(label)
        .max(body.map_or(0.0, |hit| hit.score * 0.6));
    let snippet = body
        .filter(|hit| hit.score * 0.6 == score)
        .map(|hit| super::super::fuzzy::snippet(&content, hit));
    (score, snippet)
}

fn sorted<'a>(pages: impl Iterator<Item = &'a Page>) -> Vec<&'a Page> {
    let mut pages = pages.collect::<Vec<_>>();
    pages.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    pages
}

fn sorted_updated<'a>(pages: impl Iterator<Item = &'a Page>) -> Vec<&'a Page> {
    let mut pages = pages.collect::<Vec<_>>();
    pages.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    pages
}

fn sorted_recent(pages: &[Page]) -> Vec<&Page> {
    let mut pages = pages
        .iter()
        .filter(|page| page.status != "archived")
        .collect::<Vec<_>>();
    pages.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    pages
}

fn section_label<'a>(cx: &'a Cx, label: &str) -> BoxView<'a> {
    let label = label.to_owned();
    view! {
        cx =>
        <li
            class="border-b border-solid border-[var(--border)] bg-[var(--bg-subtle)] px-3 py-2 text-caption font-semibold"
        >
            (label)
        </li>
    }.boxed()
}

fn page_row<'a>(
    cx: &'a Cx,
    project: &str,
    page: &Page,
    indent: usize,
    snippet: Option<&str>,
) -> BoxView<'a> {
    let title = page.title.clone();
    let identifier = page.identifier.clone();
    let status = page.status.clone();
    let href = super::super::navigation::attrs(cx, &format!("/public/{project}/pages/{}", page.id));
    let pinned = page.pinned;
    let label_names = page.labels.clone();
    let created = page.created_at.clone();
    let page_id = page.id.to_string();
    let snippet = match snippet.map(str::to_owned) {
        Some(text) => view! {
            cx =>
            <p class="basis-full text-body-sm text-[var(--text-muted)]">(text)</p>
        }
        .boxed(),
        None => view! { cx => }.boxed(),
    };
    let indent = format!("padding-left: {}px", indent + 12);
    view! {
        cx =>
        <li
            class="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-3"
            style=(indent)
            data-public-page-id=(page_id)
        >
            <a class="min-w-0 flex-1 font-medium hover:underline" (href)>(title)</a>
            <span class="text-caption text-[var(--text-faint)]">(identifier)</span>
            if pinned {
                <span class="text-caption text-[var(--accent)]" aria-label="Pinned">
                    "Pinned"
                </span>
            }
            <span
                class="rounded bg-[var(--bg-subtle)] px-2 py-1 text-caption text-[var(--text-muted)]"
            >
                (status)
            </span>
            <time class="text-caption text-[var(--text-faint)]">(created)</time>
            for label in label_names {
                <span class="text-caption text-[var(--text-muted)]">(label)</span>
            }
            (snippet)
        </li>
    }
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(id: i64, status: &str, folder_id: Option<i64>) -> Page {
        Page {
            id,
            project_id: Some(1),
            sequence: Some(id),
            identifier: format!("PUB-DOC-{id}"),
            folder_id,
            title: format!("Page {id}"),
            content: String::new(),
            sort_order: 0.0,
            status: status.into(),
            pinned: false,
            created_at: format!("2026-01-{id:02} 00:00:00"),
            updated_at: format!("2026-02-{id:02} 00:00:00"),
            seq: id,
            labels: Vec::new(),
        }
    }

    fn folder(id: i64) -> Folder {
        Folder {
            id,
            project_id: 1,
            parent_id: None,
            name: format!("Folder {id}"),
            sort_order: 0.0,
        }
    }

    #[test]
    fn search_with_default_status_includes_archived_and_ignores_folder_focus() {
        let mut active_elsewhere = page(1, "active", Some(20));
        active_elsewhere.title = "needle active".into();
        let mut archived = page(2, "archived", Some(30));
        archived.title = "needle archived".into();
        let filters = Filters {
            query: "needle".into(),
            label: String::new(),
            status: DEFAULT_STATUS.into(),
            focus_folder: 10,
            expanded: Vec::new(),
        };

        let pages = [active_elsewhere, archived];
        let found = search_candidates(&pages, &filters);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn search_keeps_concrete_status_and_label_filters() {
        let mut matching = page(1, "draft", Some(20));
        matching.labels.push("guide".into());
        let mut other_status = page(2, "active", Some(20));
        other_status.labels.push("guide".into());
        let mut other_label = page(3, "draft", Some(20));
        other_label.labels.push("notes".into());
        let filters = Filters {
            query: "page".into(),
            label: "guide".into(),
            status: "draft".into(),
            focus_folder: 0,
            expanded: Vec::new(),
        };

        let pages = [matching, other_status, other_label];
        let found = search_candidates(&pages, &filters);
        assert_eq!(found.iter().map(|page| page.id).collect::<Vec<_>>(), [1]);
    }

    #[test]
    fn search_scores_labels_and_only_searches_the_page_preview() {
        let mut labeled = page(1, "active", None);
        labeled.labels.push("needle".into());
        assert_eq!(score_page(&labeled, "needle").0, 0.5225);

        let mut long_body = page(2, "active", None);
        long_body.content = format!("{}needle", "x".repeat(220));
        assert_eq!(score_page(&long_body, "needle").0, 0.0);
    }

    #[test]
    fn label_search_scores_the_joined_label_text() {
        let mut labeled = page(1, "active", None);
        labeled.labels = vec!["unrelated".into(), "needle".into()];
        let expected = super::super::super::fuzzy::find("needle", "unrelated needle")
            .unwrap()
            .score
            * 0.55;
        assert_eq!(score_page(&labeled, "needle").0, expected);
    }

    #[test]
    fn drafts_and_archived_are_sorted_by_updated_time() {
        let mut older_update = page(1, "draft", None);
        older_update.created_at = "2026-03-01".into();
        older_update.updated_at = "2026-03-01".into();
        let mut newer_update = page(2, "draft", None);
        newer_update.created_at = "2026-02-01".into();
        newer_update.updated_at = "2026-04-01".into();

        let ordered = sorted_updated([&older_update, &newer_update].into_iter());
        assert_eq!(ordered[0].id, newer_update.id);
    }

    #[test]
    fn browse_pages_are_sorted_by_created_time() {
        let mut older_created = page(1, "active", None);
        older_created.created_at = "2026-03-01".into();
        older_created.updated_at = "2026-04-01".into();
        let mut newer_created = page(2, "active", None);
        newer_created.created_at = "2026-04-01".into();
        newer_created.updated_at = "2026-03-01".into();

        let ordered = sorted([&older_created, &newer_created].into_iter());
        assert_eq!(ordered[0].id, newer_created.id);
    }
}
