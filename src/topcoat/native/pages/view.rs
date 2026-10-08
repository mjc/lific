//! Native Pages list and editor, populated from the authorized shared service.
use super::super::super::runtime::whitespace::{
    StrEcmaTrimExt, is_ecmascript_whitespace, trim_ecmascript,
};
use super::super::{context, mascot, navigation, session, transport};
use super::actions::{create as create_page, delete as delete_page, save as save_page};
use super::status;
use crate::{db::models::Page as PageModel, error::LificError};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard, signal},
    view::{Attributes, BoxView, Unescaped, View, ViewExt, component, view},
};

pub(super) fn list<'a>(
    cx: &'a Cx,
    project: &str,
    project_id: i64,
    identity: &Option<crate::resolve_caller::ResolvedIdentity>,
    account: i64,
) -> topcoat::Result<BoxView<'a>> {
    let can_edit = match crate::authz::require_role(
        context::db(cx),
        identity,
        project_id,
        crate::db::models::Role::Maintainer,
    ) {
        Ok(()) => true,
        Err(LificError::Forbidden(_)) => false,
        Err(error) => return session::read(cx, Err(error)),
    };
    let project = project.to_owned();
    let list_cx = cx.keyed((account, project_id));
    Ok(view! {
        list_cx =>
        pages_list(
            account: account,
            project: project,
            project_id: project_id,
            can_edit: can_edit
        )
    }
    .boxed())
}

#[component]
async fn pages_list(
    cx: &Cx,
    account: i64,
    project: String,
    project_id: i64,
    can_edit: bool,
) -> topcoat::Result<impl View> {
    let caller = session::read(cx, context::caller(cx))?;
    let structure = session::read(
        cx,
        crate::services::pages::project_structure(context::db(cx), &caller.identity, project_id),
    )?;
    let folder_names = structure
        .folders
        .into_iter()
        .map(|folder| (folder.id, folder.name));
    let label_names = structure.labels.into_iter().map(|label| label.name);
    let query = signal(cx, || "".to_owned());
    let status = signal(cx, || "__active".to_owned());
    let label = signal(cx, || "".to_owned());
    let tab = signal(cx, || "browse".to_owned());
    let folder = signal(cx, || "0".to_owned());
    let title = signal(cx, || "".to_owned());
    let busy = signal(cx, || false);
    let error = signal(cx, || "".to_owned());
    let revision = signal(cx, || 0_usize);
    let create = create_attributes(
        cx,
        account,
        project_id,
        project.clone(),
        PageCreateState {
            title: title.clone(),
            busy: busy.clone(),
            error: error.clone(),
            revision: revision.clone(),
        },
    );
    let focus_create = focus_create_attributes(cx);
    let label_options = label_names
        .map(|name| view! { cx => <option value=(name.clone())>(name)</option> }.boxed())
        .collect::<Vec<_>>();
    let folder_options = folder_names
        .map(|(id, name)| view! { cx => <option value=(id.to_string())>(name)</option> }.boxed())
        .collect::<Vec<_>>();
    let status_tabs = [
        ("browse", "Browse"), ("recent", "Recent"),
        ("drafts", "Drafts"), ("archived", "Archived"),
    ].into_iter().map(|(id, name)| {
        let selected = tab.clone();
        view! {
            cx =>
            <button
                type="button"
                class="px-2.5 py-1.5 text-body-sm rounded-md border-0 bg-transparent text-[var(--text-muted)] hover:text-[var(--text)] aria-[current=page]:text-[var(--text)]"
                :aria-current=$(if selected.get() == id { "page" } else { "false" })
                @click=$(|_event: Event| selected.set(id.to_owned()))
            >
                (name)
            </button>
        }.boxed()
    }).collect::<Vec<_>>();
    Ok(view! {
        cx =>
        <div
            class="native-pages h-full min-h-0 overflow-y-auto leading-[1.6] text-[var(--text)]"
        >
            <main class="native-pages__content max-w-[1100px] mx-auto px-6 py-6">
                <header class="flex items-center gap-4 mb-4">
                    <h1 class="text-heading font-semibold m-0">"Pages"</h1>
                    <span class="ml-auto"></span>
                    if can_edit {
                        <input
                            id="native-pages-create-title"
                            aria-label="New page title"
                            maxlength="200"
                            placeholder="New page title…"
                            class="text-body-sm px-2.5 py-1.5 rounded-md border border-solid border-[var(--border)] bg-[var(--bg)] text-[var(--text)]"
                            :value=$(title.get())
                            @input=$(|event: Event| {
                                title.set(event.target.value.to_owned());
                                error.set("".to_owned());
                            })
                        />
                        <button
                            type="button"
                            class="text-body-sm text-[var(--accent)] border-0 bg-transparent"
                            (focus_create)
                        >
                            "Create a page"
                        </button>
                        <button
                            type="button"
                            class="text-body-sm font-medium px-3 py-1.5 rounded-md border-0 bg-[var(--accent)] text-[var(--accent-text)] disabled:opacity-50"
                            :disabled=$(if busy.get() {
                                true
                            } else {
                                title.get().trim().is_empty()
                            })
                            (create)
                        >
                            $(if busy.get() { "Creating…" } else { "New page" })
                        </button>
                    }
                </header>
                <div class="flex flex-wrap items-center gap-2 mb-4">
                    <div
                        class="inline-flex gap-1 p-0.5 rounded-lg bg-[var(--bg-subtle)]"
                        role="tablist"
                        aria-label="Page views"
                    >
                        for button in status_tabs {
                            (button)
                        }
                    </div>
                    <input
                        aria-label="Search pages"
                        placeholder="Search pages…"
                        class="ml-auto w-56 text-body-sm px-2.5 py-1.5 rounded-md border border-solid border-[var(--border)] bg-[var(--bg)] text-[var(--text)]"
                        :value=$(query.get())
                        @input=$(|event: Event| query.set(event.target.value.to_owned()))
                    />
                    <select
                        aria-label="Filter by status"
                        class="text-body-sm px-2 py-1.5 rounded-md border border-solid border-[var(--border)] bg-[var(--bg)] text-[var(--text)]"
                        :value=$(status.get())
                        @change=$(|event: Event| status.set(
                                event.target.value.to_owned(),
                            ))
                    >
                        <option value="__active">"Active"</option>
                        <option value="">"All"</option>
                        <option value="draft">"Draft"</option>
                        <option value="active">"Active"</option>
                        <option value="complete">"Complete"</option>
                        <option value="archived">"Archived"</option>
                    </select>
                    <select
                        aria-label="Filter by label"
                        class="text-body-sm px-2 py-1.5 rounded-md border border-solid border-[var(--border)] bg-[var(--bg)] text-[var(--text)]"
                        :value=$(label.get())
                        @change=$(|event: Event| label.set(event.target.value.to_owned()))
                    >
                        <option value="">"Label"</option>
                        for option in label_options {
                            (option)
                        }
                    </select>
                    <select
                        aria-label="Filter by folder"
                        class="text-body-sm px-2 py-1.5 rounded-md border border-solid border-[var(--border)] bg-[var(--bg)] text-[var(--text)]"
                        :value=$(folder.get())
                        @change=$(|event: Event| folder.set(
                                event.target.value.to_owned(),
                            ))
                    >
                        <option value="0">"All folders"</option>
                        for option in folder_options {
                            (option)
                        }
                    </select>
                </div>
                <div
                    role="alert"
                    class="text-body-sm text-[var(--error)] mb-3"
                    :hidden=$(error.get().is_empty())
                >
                    $(error.get())
                </div>
                native_pages_rows(
                    account: account,
                    project_id: project_id,
                    project: project.clone(),
                    query: $(query.get()),
                    status: $(status.get()),
                    label: $(label.get()),
                    tab: $(tab.get()),
                    folder: $(folder.get()),
                    revision: $(revision.get())
                )
            </main>
        </div>
    })
}

struct PageCreateState {
    title: Signal<String>,
    busy: Signal<bool>,
    error: Signal<String>,
    revision: Signal<usize>,
}

fn create_attributes(
    cx: &Cx,
    account: i64,
    project_id: i64,
    project: String,
    state: PageCreateState,
) -> Attributes {
    let PageCreateState {
        title,
        busy,
        error,
        revision,
    } = state;
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let destination = transport::mounted_url(cx, &format!("/{project}/pages/"));
    let handler = expr!(async |_event: Event| {
        if !busy.get() {
            let value = title.get().trim_ecmascript().to_owned();
            if !value.is_empty() {
                busy.set(true);
                error.set("".to_owned());
                let _failed = || {
                    failed_busy.set(false);
                    failed_error
                        .set("Couldn't create the page. Your title is still here.".to_owned());
                };
                let _create = async || {
                    let outcome = create_page(account, project_id, value).await;
                    busy.set(false);
                    if outcome.status.is_ok() {
                        let _id = outcome.page_id.unwrap();
                        title.set("".to_owned());
                        revision.increment();
                        raw!(
                            "void cx.navigate(${destination}.toString()+${_id}.toString())",
                            ()
                        );
                    } else {
                        let message = outcome.status.unwrap_err();
                        error.set(if message == "reauth" {
                            "Please sign in again.".to_owned()
                        } else if message == "forbidden" {
                            "You can no longer edit this project.".to_owned()
                        } else {
                            message
                        });
                    }
                };
                raw!(
                    "Promise.resolve().then(() => ${_create}()).catch(() => ${_failed}());",
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
    attrs
}

fn focus_create_attributes(cx: &Cx) -> Attributes {
    let handler = expr!(|_event: Event| {
        raw!(
            "document.querySelector('#native-pages-create-title')?.focus()",
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

use row_shards::native_pages_rows;

#[allow(
    clippy::too_many_arguments,
    reason = "Topcoat generates flat shard handlers and drops function lint attributes"
)]
mod row_shards {
    use super::*;

    #[shard("/__native_pages/rows")]
    pub(super) async fn native_pages_rows(
        cx: &Cx,
        account: i64,
        project_id: i64,
        project: String,
        query: String,
        status: String,
        label: String,
        tab: String,
        folder: String,
        revision: usize,
    ) -> topcoat::Result<impl View> {
        let _ = revision;
        let caller = session::read(cx, context::caller(cx))?;
        let user = session::read(cx, crate::api::require_user(&caller.identity))?;
        if user.id != account {
            return session::read(
                cx,
                Err(LificError::Forbidden(
                    "Your account changed. Reload this page.".into(),
                )),
            );
        }
        let folder = folder.parse::<i64>().unwrap_or(0);
        let query_empty = trim_ecmascript(&query).is_empty();
        let searching = !query_empty;
        let rows = session::read(
            cx,
            crate::services::pages::list_project_pages(
                context::db(cx),
                &caller.identity,
                project_id,
            ),
        )?;
        let is_true_empty = rows.is_empty();
        let mut hits = rows
            .into_iter()
            .filter(|page| page_matches_filters(page, &tab, &status, &label, folder, searching))
            .filter_map(|page| {
                let (score, _) = search_hit(&query, &page);
                score.map(|score| (score, page))
            })
            .collect::<Vec<_>>();
        if query_empty {
            if tab == "recent" || tab == "drafts" || tab == "archived" {
                hits.sort_by(|left, right| right.1.updated_at.cmp(&left.1.updated_at));
            } else {
                hits.sort_by(|left, right| right.1.created_at.cmp(&left.1.created_at));
            }
        } else {
            hits.sort_by(|left, right| right.0.total_cmp(&left.0));
            hits.truncate(50);
        }
        let pages = hits
            .into_iter()
            .map(|(_, page)| {
                let href = navigation::attrs(cx, &format!("/{project}/pages/{}", page.id));
                let preview = content_preview(&page.preview);
                (page, href, preview)
            })
            .collect::<Vec<_>>();
        Ok(view! {
            cx =>
            if pages.is_empty() {
                if is_true_empty {
                    if query_empty {
                        if tab == "browse" {
                            <div
                                class="native-pages__empty rounded-lg border border-solid border-[var(--border)] px-5 py-8 text-center"
                            >
                                (mascot::render(cx, mascot::Mascot::Reading, 0.25))
                                <h2
                                    class="text-heading font-semibold text-[var(--text)] mt-4 mb-2"
                                >
                                    "A blank page"
                                </h2>
                                <p
                                    class="text-body-sm text-[var(--text-muted)] max-w-[460px] mx-auto"
                                >
                                    "Pages are your project's docs: specs, notes, decisions. Start the first one and give the ideas a home."
                                </p>
                            </div>
                        } else {
                            <div
                                class="native-pages__empty rounded-lg border border-solid border-[var(--border)] px-5 py-8 text-center"
                            >
                                <p class="text-body font-medium text-[var(--text)] m-0">
                                    if query_empty {
                                        if tab == "drafts" {
                                            "No drafts"
                                        } else if tab == "archived" {
                                            "No archived pages"
                                        } else {
                                            "No pages yet"
                                        }
                                    } else {
                                        "No matching pages"
                                    }
                                </p>
                            </div>
                        }
                    } else {
                        <div
                            class="native-pages__empty rounded-lg border border-solid border-[var(--border)] px-5 py-8 text-center"
                        >
                            <p class="text-body font-medium text-[var(--text)] m-0">
                                if query_empty {
                                    "No pages yet"
                                } else {
                                    "No matching pages"
                                }
                            </p>
                        </div>
                    }
                } else {
                    <div
                        class="native-pages__empty rounded-lg border border-solid border-[var(--border)] px-5 py-8 text-center"
                    >
                        <p class="text-body font-medium text-[var(--text)] m-0">
                            "No matching pages"
                        </p>
                    </div>
                }
            } else {
                <ul class="native-pages__rows list-none p-0 m-0" aria-label="Pages">
                    for (page, href, preview) in pages {
                        <li
                            data-native-page-row=(page.id.to_string())
                            class="border-b border-solid border-[var(--border)] last:border-b-0"
                        >
                            <a
                                class="native-pages__row flex flex-col gap-1 px-3 py-3 rounded-md no-underline hover:bg-[var(--bg-subtle)]"
                                (href)
                            >
                                <span
                                    class="flex flex-wrap items-baseline gap-x-2 gap-y-0.5"
                                >
                                    <span
                                        class="font-mono text-caption text-[var(--text-muted)]"
                                    >
                                        (page.identifier)
                                    </span>
                                    <span class="text-body font-medium text-[var(--text)]">
                                        (page.title)
                                    </span>
                                    <span class="text-micro text-[var(--text-faint)]">
                                        (status_label(&page.status))
                                    </span>
                                    if page.pinned {
                                        <span class="text-micro text-[var(--accent)]">
                                            "Pinned"
                                        </span>
                                    }
                                </span>
                                if !preview.is_empty() {
                                    <span
                                        class="text-body-sm text-[var(--text-muted)] line-clamp-2"
                                    >
                                        (preview)
                                    </span>
                                }
                                if !page.labels.is_empty() {
                                    <span class="flex gap-1.5 mt-0.5">
                                        for item in page.labels {
                                            <span
                                                class="text-micro px-1.5 py-0.5 rounded bg-[var(--bg-subtle)] text-[var(--text-muted)]"
                                            >
                                                (item)
                                            </span>
                                        }
                                    </span>
                                }
                            </a>
                        </li>
                    }
                </ul>
            }
        })
    }
}

fn page_matches_filters(
    page: &crate::services::pages::PageRow,
    tab: &str,
    status: &str,
    label: &str,
    folder: i64,
    searching: bool,
) -> bool {
    if !label.is_empty() && !page.labels.iter().any(|item| item == label) {
        return false;
    }
    if !searching && folder != 0 && page.folder_id != Some(folder) {
        return false;
    }
    if !searching {
        let tab_match = match tab {
            "recent" => page.status != "archived",
            "drafts" => page.status == "draft",
            "archived" => page.status == "archived",
            _ => true,
        };
        if !tab_match {
            return false;
        }
    }
    if searching && status == "__active" {
        return true;
    }
    if !searching && tab != "browse" {
        return true;
    }
    status.is_empty() || status == "__active" && page.status != "archived" || page.status == status
}

pub(super) fn status_label(status: &str) -> &'static str {
    match status {
        "draft" => "Draft",
        "active" => "Active",
        "complete" => "Complete",
        "archived" => "Archived",
        _ => "Draft",
    }
}

fn content_preview(preview: &str) -> String {
    preview
        .trim_start_matches('#')
        .trim()
        .chars()
        .filter(|character| !matches!(character, '*' | '_' | '`' | '[' | ']'))
        .take(140)
        .collect()
}

fn search_hit(
    query: &str,
    page: &crate::services::pages::PageRow,
) -> (Option<f64>, Option<String>) {
    let query = trim_ecmascript(query);
    if query.is_empty() {
        return (Some(0.0), None);
    }
    const THRESHOLD: f64 = 0.25;
    let title = fuzzy_score(query, &page.title).unwrap_or(0.0);
    let identifier = fuzzy_score(query, &page.identifier).unwrap_or(0.0) * 0.9;
    let content = fuzzy_score(query, &page.preview).unwrap_or(0.0) * 0.6;
    let labels = fuzzy_score(query, &page.labels.join(" ")).unwrap_or(0.0) * 0.55;
    let best = title.max(identifier).max(content).max(labels);
    ((best >= THRESHOLD).then_some(best), None)
}

/// Port of the original two-tier case-insensitive PageList matcher.
fn fuzzy_score(query: &str, text: &str) -> Option<f64> {
    if query.is_empty() || text.is_empty() {
        return None;
    }
    let q = query.to_lowercase();
    let t = text.to_lowercase();
    if let Some(start) = t.find(&q) {
        let boundary = start == 0 || t[..start].chars().next_back().is_some_and(is_word_boundary);
        return Some(if start == 0 {
            0.95
        } else if boundary {
            0.9
        } else {
            0.8
        });
    }
    let query_units = q.encode_utf16().collect::<Vec<_>>();
    let text_units = t.encode_utf16().collect::<Vec<_>>();
    let (mut query_index, mut first, mut last) = (0, None, None);
    let (mut current, mut longest, mut boundaries) = (0_usize, 0_usize, 0_usize);
    for (index, unit) in text_units.iter().copied().enumerate() {
        if query_units.get(query_index) != Some(&unit) {
            continue;
        }
        first.get_or_insert(index);
        let consecutive = match last {
            Some(previous) => index == previous + 1,
            None => index == 0,
        };
        if consecutive {
            current += 1;
        } else {
            current = 1;
            if index == 0 || is_word_boundary_unit(text_units[index - 1]) {
                boundaries += 1;
            }
        }
        longest = longest.max(current);
        last = Some(index);
        query_index += 1;
        if query_index == query_units.len() {
            break;
        }
    }
    if query_index != query_units.len() {
        return None;
    }
    let span = last.unwrap().saturating_sub(first.unwrap_or(0)) + 1;
    let size = query_units.len() as f64;
    Some(
        (0.4 * size / span as f64 + 0.4 * longest as f64 / size + 0.2 * boundaries as f64 / size)
            .min(0.7),
    )
}

fn is_word_boundary(character: char) -> bool {
    is_ecmascript_whitespace(character) || "-_/.,()[]{}<>:;!?\"'`".contains(character)
}

fn is_word_boundary_unit(unit: u16) -> bool {
    char::from_u32(u32::from(unit)).is_some_and(is_word_boundary)
}

pub(super) fn detail<'a>(
    cx: &'a Cx,
    project: &str,
    page_id: &str,
    identity: &Option<crate::resolve_caller::ResolvedIdentity>,
    account: i64,
) -> topcoat::Result<BoxView<'a>> {
    let _id = page_id
        .parse::<i64>()
        .map_err(|_| topcoat::router::error::not_found())?;
    let page = session::read(
        cx,
        crate::services::pages::get(context::db(cx), identity, _id),
    )?;
    if let Some(page_project_id) = page.project_id {
        let conn = context::db(cx).read()?;
        let route_id = crate::db::queries::resolve_project_identifier(&conn, project)?;
        if page_project_id != route_id {
            return Err(topcoat::router::error::not_found().into());
        }
    }
    let can_edit = match crate::services::pages::require_page_role(
        context::db(cx),
        identity,
        page.project_id,
        crate::db::models::Role::Maintainer,
    ) {
        Ok(()) => true,
        Err(LificError::Forbidden(_)) => false,
        Err(error) => return session::read(cx, Err(error)),
    };
    let list_path = format!("/{project}/pages");
    let project = project.to_owned();
    let page_cx = cx.keyed((account, page.id));
    Ok(view! {
        page_cx =>
        page_detail(
            account: account,
            project: project,
            page: page,
            can_edit: can_edit,
            list_path: list_path
        )
    }
    .boxed())
}

#[shard("/__native_pages/markdown")]
async fn native_page_markdown(
    cx: &Cx,
    account: i64,
    page_id: i64,
    source: String,
) -> topcoat::Result<impl View> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return session::read(
            cx,
            Err(LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    session::read(
        cx,
        crate::services::pages::get(context::db(cx), &caller.identity, page_id),
    )?;
    let rendered =
        super::super::markdown::render(cx, &source, super::super::markdown::Scope::Private, &[]);
    Ok(view! {
        cx =>
        if source.trim().is_empty() {
            <p class="text-body-sm italic text-[var(--text-muted)]">"Empty page"</p>
        } else {
            <article class="markdown-body prose max-w-none">
                (Unescaped::new_unchecked(rendered))
            </article>
        }
    })
}

#[component]
async fn page_detail(
    cx: &Cx,
    account: i64,
    project: String,
    page: PageModel,
    can_edit: bool,
    list_path: String,
) -> topcoat::Result<impl View> {
    let list_attrs = navigation::attrs(cx, &list_path);
    let title = signal(cx, || page.title.clone());
    let body = signal(cx, || page.content.clone());
    let title_draft = signal(cx, || page.title.clone());
    let body_draft = signal(cx, || page.content.clone());
    let seq = signal(cx, || page.seq);
    let title_editing = signal(cx, || false);
    let body_editing = signal(cx, || false);
    let busy = signal(cx, || false);
    let confirming_delete = signal(cx, || false);
    let message = signal(cx, || "".to_owned());
    let save = save_attributes(
        cx,
        account,
        page.id,
        title.clone(),
        body.clone(),
        title_draft.clone(),
        body_draft.clone(),
        seq.clone(),
        title_editing.clone(),
        body_editing.clone(),
        busy.clone(),
        message.clone(),
    );
    let delete = delete_attributes(
        cx,
        account,
        page.id,
        project,
        busy.clone(),
        confirming_delete.clone(),
        message.clone(),
    );
    let created_at = super::super::dates::absolute_time_view(cx, &page.created_at);
    let updated_at = super::super::dates::absolute_time_view(cx, &page.updated_at);
    Ok(view! {
        cx =>
        <div
            class="native-pages h-full min-h-0 overflow-y-auto leading-[1.6] text-[var(--text)]"
        >
            <main class="native-pages__detail max-w-[860px] mx-auto px-6 py-6">
                <a
                    class="text-body-sm text-[var(--text-muted)] no-underline hover:text-[var(--text)]"
                    (list_attrs)
                >
                    "‹ Pages"
                </a>
                <div class="mt-5 mb-6">
                    <div class="font-mono text-caption text-[var(--text-muted)]">
                        (page.identifier)
                    </div>
                    if can_edit {
                        <h1
                            class="text-title font-semibold mt-1 mb-0"
                            :hidden=$(title_editing.get())
                        >
                            <button
                                type="button"
                                class="p-0 border-0 bg-transparent text-left text-[var(--text)] font-semibold"
                                @click=$(|_event: Event| {
                                    title_draft.set(title.get());
                                    title_editing.set(true);
                                })
                            >
                                $(title.get())
                            </button>
                        </h1>
                        <input
                            aria-label="Page title"
                            class="text-title font-semibold w-full mt-1 px-0 py-1 border-0 border-b border-solid border-[var(--border)] bg-transparent text-[var(--text)]"
                            :hidden=$(if title_editing.get() { false } else { true })
                            :value=$(title_draft.get())
                            :disabled=$(busy.get())
                            @input=$(|event: Event| {
                                title_draft.set(event.target.value.to_owned());
                                title_editing.set(true);
                            })
                        />
                    } else {
                        <h1 class="text-title font-semibold mt-1 mb-0">
                            $(title.get())
                        </h1>
                    }
                </div>
                <div class="mb-6 flex flex-wrap items-center gap-4">
                    (pin::detail(
                        cx,
                        page.pinned,
                        account,
                        page.id,
                        seq.clone(),
                        busy.clone(),
                        can_edit,
                    ))
                    (status::detail(
                        cx,
                        page.status.clone(),
                        status::State::new(
                            account,
                            page.id,
                            seq.clone(),
                            busy.clone(),
                            can_edit,
                        ),
                    ))
                </div>
                if can_edit {
                    <button
                        type="button"
                        class="text-body-sm text-[var(--accent)] border-0 bg-transparent px-0 py-1"
                        :hidden=$(body_editing.get())
                        @click=$(|_event: Event| {
                            if !title_editing.get() {
                                title_draft.set(title.get());
                                title_editing.set(true);
                            }
                            body_draft.set(body.get());
                            body_editing.set(true);
                            message.set("".to_owned());
                        })
                    >
                        "Edit page"
                    </button>
                    native_page_markdown(
                        account: account,
                        page_id: page.id,
                        source: $(body.get())
                    )
                    <textarea
                        aria-label="Page content in Markdown"
                        class="native-pages__editor w-full min-h-[240px] resize-y p-3 rounded-md border border-solid border-[var(--border)] bg-[var(--bg)] text-[var(--text)] font-mono text-body-sm"
                        placeholder="Start writing... (markdown supported)"
                        :hidden=$(if body_editing.get() { false } else { true })
                        :value=$(body_draft.get())
                        :disabled=$(busy.get())
                        @input=$(|event: Event| {
                            body_draft.set(event.target.value.to_owned());
                            body_editing.set(true);
                        })
                    ></textarea>
                    <div
                        class="flex items-center gap-3 mt-2"
                        :hidden=$(if body_editing.get() {
                            false
                        } else {
                            if title_editing.get() { false } else { true }
                        })
                    >
                        <button
                            type="button"
                            class="px-3 py-1.5 rounded-md bg-[var(--accent)] text-[var(--accent-text)] border-0 text-body-sm"
                            :disabled=$(if busy.get() {
                                true
                            } else {
                                if title_editing.get() {
                                    false
                                } else {
                                    if body_editing.get() { false } else { true }
                                }
                            })
                            (save)
                        >
                            $(if busy.get() { "Saving…" } else { "Save changes" })
                        </button>
                        <button
                            type="button"
                            class="px-2.5 py-1.5 rounded-md border border-solid border-[var(--border)] bg-transparent text-body-sm text-[var(--text)]"
                            :disabled=$(busy.get())
                            @click=$(|_event: Event| {
                                title_draft.set(title.get());
                                body_draft.set(body.get());
                                title_editing.set(false);
                                body_editing.set(false);
                                message.set("".to_owned());
                            })
                        >
                            "Cancel"
                        </button>
                        <span
                            class="text-body-sm text-[var(--text-muted)]"
                            role="status"
                        >
                            $(message.get())
                        </span>
                        <button
                            type="button"
                            class="ml-auto px-3 py-1.5 rounded-md border border-solid border-[var(--error)] text-[var(--error)] bg-transparent text-body-sm"
                            :hidden=$(confirming_delete.get())
                            :disabled=$(busy.get())
                            @click=$(|_event: Event| confirming_delete.set(true))
                        >
                            "Delete page"
                        </button>
                        <span
                            class="ml-auto flex items-center gap-2 text-body-sm text-[var(--error)]"
                            :hidden=$(if confirming_delete.get() { false } else { true })
                        >
                            "Delete this page?"
                            <button
                                type="button"
                                class="px-2.5 py-1 rounded-md border-0 bg-[var(--error)] text-white"
                                :disabled=$(busy.get())
                                (delete)
                            >
                                "Confirm delete"
                            </button>
                            <button
                                type="button"
                                class="px-2.5 py-1 rounded-md border border-solid border-[var(--border)] bg-transparent text-[var(--text)]"
                                :disabled=$(busy.get())
                                @click=$(|_event: Event| confirming_delete.set(false))
                            >
                                "Cancel"
                            </button>
                        </span>
                    </div>
                } else {
                    native_page_markdown(
                        account: account,
                        page_id: page.id,
                        source: $(body.get())
                    )
                }
                <div
                    class="mt-10 pt-6 border-t border-solid border-[var(--border)] flex gap-8"
                >
                    <div>
                        <span
                            class="block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-0.5"
                        >
                            "Created"
                        </span>
                        <span class="text-body-sm text-[var(--text-muted)]">
                            (created_at)
                        </span>
                    </div>
                    <div>
                        <span
                            class="block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-0.5"
                        >
                            "Updated"
                        </span>
                        <span class="text-body-sm text-[var(--text-muted)]">
                            (updated_at)
                        </span>
                    </div>
                </div>
            </main>
        </div>
    })
}

#[allow(clippy::too_many_arguments)]
fn save_attributes(
    cx: &Cx,
    account: i64,
    page_id: i64,
    title: Signal<String>,
    body: Signal<String>,
    title_draft: Signal<String>,
    body_draft: Signal<String>,
    seq: Signal<i64>,
    title_editing: Signal<bool>,
    body_editing: Signal<bool>,
    busy: Signal<bool>,
    message: Signal<String>,
) -> Attributes {
    let failed_busy = busy.clone();
    let failed_message = message.clone();
    let handler = expr!(async |_event: Event| {
        if !busy.get() {
            let sent_title = title_draft.get();
            let next_title = sent_title.trim_ecmascript().to_owned();
            let next_body = body_draft.get();
            if !next_title.is_empty() {
                busy.set(true);
                message.set("".to_owned());
                let sent_seq = seq.get();
                let sent_body = next_body.clone();
                let _failed = || {
                    failed_busy.set(false);
                    failed_message
                        .set("Couldn't save the page. Your draft is still here.".to_owned());
                };
                let _save = async || {
                    let outcome =
                        save_page(account, page_id, next_title, next_body, sent_seq).await;
                    busy.set(false);
                    if outcome.status.is_ok() {
                        let saved_title = outcome.title.clone().unwrap();
                        let saved_body = outcome.content.clone().unwrap();
                        let title_unchanged = title_draft.get() == sent_title;
                        let body_unchanged = body_draft.get() == sent_body;
                        title.set(saved_title.clone());
                        body.set(saved_body.clone());
                        seq.set(outcome.seq.unwrap());
                        if title_unchanged {
                            title_draft.set(saved_title);
                            title_editing.set(false);
                        }
                        if body_unchanged {
                            body_draft.set(saved_body);
                            body_editing.set(false);
                        }
                        message.set("Saved".to_owned());
                    } else {
                        let reason = outcome.status.unwrap_err();
                        if reason == "conflict" {
                            message.set("This page changed elsewhere. Reload before saving again; your draft is still here.".to_owned());
                        } else if reason == "reauth" {
                            message.set("Please sign in again.".to_owned());
                        } else if reason == "forbidden" {
                            message.set("You can no longer edit this page.".to_owned());
                        } else {
                            message.set(reason);
                        }
                    }
                };
                raw!(
                    "Promise.resolve().then(()=>${_save}()).catch(()=>${_failed}());",
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
    attrs
}

fn delete_attributes(
    cx: &Cx,
    account: i64,
    page_id: i64,
    project: String,
    busy: Signal<bool>,
    confirming: Signal<bool>,
    message: Signal<String>,
) -> Attributes {
    let failed_busy = busy.clone();
    let failed_message = message.clone();
    let destination = transport::mounted_url(cx, &format!("/{project}/pages"));
    let handler = expr!(async |_event: Event| {
        if if confirming.get() { !busy.get() } else { false } {
            busy.set(true);
            message.set("".to_owned());
            let _failed = || {
                failed_busy.set(false);
                failed_message.set("Couldn't delete this page.".to_owned());
            };
            let _delete = async || {
                let outcome = delete_page(account, page_id).await;
                busy.set(false);
                if outcome.status.is_ok() {
                    raw!("void cx.navigate(${destination}.toString())", ());
                } else {
                    let reason = outcome.status.unwrap_err();
                    message.set(if reason == "reauth" {
                        "Please sign in again.".to_owned()
                    } else if reason == "forbidden" {
                        "You can no longer edit this page.".to_owned()
                    } else {
                        reason
                    });
                }
            };
            raw!(
                "Promise.resolve().then(()=>${_delete}()).catch(()=>${_failed}());",
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

#[cfg(test)]
mod tests {
    use super::{fuzzy_score, page_matches_filters, search_hit};
    use crate::services::pages::PageRow;
    use topcoat::{
        context::Cx,
        runtime::{Surrogated, signal},
        view::{View, ViewExt, component, view},
    };

    #[component]
    async fn page_owner_probe(cx: &Cx, account: i64, page_id: i64) -> topcoat::Result<impl View> {
        let owner = cx.keyed((account, page_id));
        let draft = signal(&owner, || "".to_owned());
        let signal_id = serde_json::to_value(draft.into_surrogate()).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        Ok(view! { cx => <span data-page-draft-signal=(signal_id)></span> })
    }

    async fn page_owner_markup(cx: &Cx, account: i64, page_id: i64) -> String {
        view! { cx => page_owner_probe(account: account, page_id: page_id) }
            .single()
            .await
            .unwrap()
            .render(cx)
    }

    fn row(status: &str, folder_id: Option<i64>, labels: &[&str]) -> PageRow {
        PageRow {
            id: 1,
            identifier: "P-1".into(),
            title: "Page".into(),
            preview: "preview".into(),
            status: status.into(),
            folder_id,
            pinned: false,
            labels: labels.iter().map(|label| (*label).into()).collect(),
            created_at: "".into(),
            updated_at: "".into(),
        }
    }
    #[test]
    fn fuzzy_search_matches_master_substring_and_subsequence_scores() {
        assert_eq!(fuzzy_score("design", "Design notes"), Some(0.95));
        assert_eq!(fuzzy_score("notes", "Design notes"), Some(0.9));
        assert!((fuzzy_score("dgn", "Design notes").unwrap() - 0.466_666_666_7).abs() < 0.000_001);
        assert_eq!(fuzzy_score("xyz", "Design notes"), None);
    }

    #[test]
    fn fuzzy_search_matches_master_first_match_utf16_and_ecmascript_whitespace() {
        assert!((fuzzy_score("ab", "axb").unwrap() - 0.466_666_666_7).abs() < 0.000_001);
        assert!((fuzzy_score("ab", " axb").unwrap() - 0.566_666_666_7).abs() < 0.000_001);
        assert!((fuzzy_score("ab", "a😀b").unwrap() - 0.4).abs() < 0.000_001);
        assert!((fuzzy_score("ab", "\u{feff}axb").unwrap() - 0.566_666_666_7).abs() < 0.000_001);
        assert!((fuzzy_score("ab", "\u{0085}axb").unwrap() - 0.466_666_666_7).abs() < 0.000_001);

        let mut page = row("active", None, &[]);
        page.title = "axb".into();
        assert!(
            (search_hit("\u{feff}ab\u{feff}", &page).0.unwrap() - 0.466_666_666_7).abs()
                < 0.000_001
        );
    }

    #[test]
    fn search_ignores_tabs_folders_and_default_active_filter_but_keeps_explicit_filters() {
        let archived = row("archived", Some(7), &["spec"]);
        assert!(page_matches_filters(
            &archived, "drafts", "__active", "", 99, true
        ));
        assert!(!page_matches_filters(
            &archived, "drafts", "archived", "other", 99, true
        ));
        assert!(!page_matches_filters(
            &archived, "browse", "active", "spec", 7, true
        ));
        assert!(!page_matches_filters(
            &archived, "browse", "__active", "", 99, false
        ));
    }

    #[tokio::test]
    async fn keyed_page_owners_separate_signal_identity_between_records() {
        let cx = Cx::default();
        let first = page_owner_markup(&cx, 7, 11).await;
        let second = page_owner_markup(&cx, 7, 12).await;
        let first_again = page_owner_markup(&cx, 7, 11).await;
        assert_ne!(first, second);
        assert_eq!(first, first_again);
    }
}
