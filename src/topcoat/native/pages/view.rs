//! Native Pages list and editor, populated from the authorized shared service.
use super::super::super::runtime::whitespace::{StrEcmaTrimExt, trim_ecmascript};
use super::super::fuzzy::score as fuzzy_score;
use super::super::{browser, context, icons, mascot, navigation, session, transport};
use super::actions::{
    create as create_page, delete as delete_page, move_to_folder as commit_move_page,
    save as save_page,
};
use super::{pin, status};
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
    let folder_catalog = structure
        .folders
        .into_iter()
        .map(|folder| (folder.id, folder.name))
        .collect::<Vec<_>>();
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
    let move_open = signal(cx, || false);
    let move_page_id = signal(cx, || 0_i64);
    let move_page_title = signal(cx, String::new);
    let move_folder = signal(cx, String::new);
    let move_busy = signal(cx, || false);
    let move_error = signal(cx, String::new);
    let move_error_prefix = signal(cx, String::new);
    let move_browser = browser::bindings();
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
    let folder_options = folder_catalog
        .iter()
        .map(|(id, name)| {
            let value = id.to_string();
            let name = name.clone();
            view! { cx => <option value=(value)>(name)</option> }.boxed()
        })
        .collect::<Vec<_>>();
    let picker_folder_options = folder_catalog
        .iter()
        .map(|(id, name)| {
            let value = id.to_string();
            let name = name.clone();
            let selected = move_folder.clone();
            view! {
                cx =>
                <option value=(value.clone()) :selected=$(selected.get() == value)>
                    (name)
                </option>
            }
            .boxed()
        })
        .collect::<Vec<_>>();
    let picker_change = move_picker_change(
        cx,
        account,
        MovePickerState {
            open: move_open.clone(),
            page_id: move_page_id.clone(),
            page_title: move_page_title.clone(),
            folder: move_folder.clone(),
            busy: move_busy.clone(),
            error: move_error.clone(),
            error_prefix: move_error_prefix.clone(),
            revision: revision.clone(),
        },
    );
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
                    can_edit: can_edit,
                    query: $(query.get()),
                    status: $(status.get()),
                    label: $(label.get()),
                    tab: $(tab.get()),
                    folder: $(folder.get()),
                    revision: $(revision.get()),
                    move_state: (
                        move_open.clone(),
                        move_page_id.clone(),
                        move_page_title.clone(),
                        move_folder.clone(),
                        move_busy.clone(),
                        move_error.clone(),
                        move_error_prefix.clone(),
                    )
                )
                if can_edit {
                    <div
                        data-native-page-move-backdrop=""
                        class="fixed inset-0 z-[100] flex items-start justify-center bg-black/25 px-2 pt-[14dvh]"
                        :hidden=$(!move_open.get())
                        @click=$(|_event: Event| {
                            if !move_browser.is_disposed() {
                                if !move_busy.get() {
                                    if raw!(
                                        "cx.hydrate(${_event}.inner.target === ${_event}.inner.currentTarget)",
                                        false,
                                    ) {
                                        move_open.set(false);
                                    }
                                }
                            }
                        })
                        @keydown=$(|event: Event| {
                            if !move_browser.is_disposed() {
                                if event.key == "Escape" {
                                    if !move_busy.get() {
                                        move_open.set(false);
                                    }
                                }
                            }
                        })
                    >
                        <div
                            class="w-full max-w-[calc(100vw-1rem)] sm:max-w-[420px] rounded-xl border border-solid border-[var(--border)] bg-[var(--surface)] shadow-[0_16px_48px_rgba(0,0,0,0.28)]"
                            role="dialog"
                            aria-modal="true"
                            aria-label="Move page to folder"
                            tabindex="-1"
                        >
                            <div
                                class="flex items-center gap-3 px-4 py-3 border-b border-solid border-[var(--border)]"
                            >
                                <h2
                                    class="m-0 flex-1 text-body-lg font-semibold text-[var(--text)]"
                                >
                                    "Move to folder"
                                </h2>
                                <span
                                    class="truncate text-caption text-[var(--text-faint)]"
                                >
                                    $(move_page_title.get())
                                </span>
                                <button
                                    type="button"
                                    aria-label="Close move folder picker"
                                    data-native-page-move-cancel=""
                                    class="size-7 rounded-md border-0 bg-transparent text-[var(--text-muted)] hover:bg-[var(--bg-subtle)] hover:text-[var(--text)] disabled:opacity-50"
                                    :disabled=$(move_busy.get())
                                    @click=$(|_event: Event| {
                                        if !move_browser.is_disposed() {
                                            if !move_busy.get() {
                                                move_open.set(false);
                                            }
                                        }
                                    })
                                >
                                    (icons::ui_icon(cx, icons::UiIcon::Close, 15))
                                </button>
                            </div>
                            <div class="px-4 py-4">
                                <label
                                    for="native-page-move-folder"
                                    class="mb-2 block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                                >
                                    "Folder"
                                </label>
                                <select
                                    id="native-page-move-folder"
                                    aria-label="Folder"
                                    data-native-page-move-folder=""
                                    class="w-full rounded-md border border-solid border-[var(--border)] bg-[var(--bg)] px-2.5 py-2 text-body-sm text-[var(--text)]"
                                    :value=$(move_folder.get())
                                    :disabled=$(move_busy.get())
                                    (picker_change)
                                >
                                    <option value="" :selected=$(move_folder.get().is_empty())>
                                        "No folder / root"
                                    </option>
                                    for option in picker_folder_options {
                                        (option)
                                    }
                                </select>
                                <p
                                    data-native-page-move-error=""
                                    role="alert"
                                    class="mt-3 text-body-sm text-[var(--error)]"
                                    :hidden=$(move_error.get().is_empty())
                                >
                                    $(move_error_prefix.get())
                                    $(move_error.get())
                                </p>
                            </div>
                        </div>
                    </div>
                }
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

#[derive(Clone)]
struct MovePickerState {
    open: Signal<bool>,
    page_id: Signal<i64>,
    page_title: Signal<String>,
    folder: Signal<String>,
    busy: Signal<bool>,
    error: Signal<String>,
    error_prefix: Signal<String>,
    revision: Signal<usize>,
}

#[derive(Clone)]
struct MovePickerTriggerState {
    open: Signal<bool>,
    page_id: Signal<i64>,
    page_title: Signal<String>,
    folder: Signal<String>,
    busy: Signal<bool>,
    error: Signal<String>,
    error_prefix: Signal<String>,
}

fn move_picker_change(cx: &Cx, account: i64, state: MovePickerState) -> Attributes {
    let MovePickerState {
        open,
        page_id,
        page_title: _,
        folder,
        busy,
        error,
        error_prefix: _,
        revision,
    } = state;
    let browser = browser::bindings();
    let failed_page_id = page_id.clone();
    let failed_folder = folder.clone();
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let handler = expr!(async |event: Event| {
        if !browser.is_disposed() {
            if !busy.get() {
                let next_folder = event.target.value.to_owned();
                let previous_folder = folder.get();
                if next_folder == previous_folder {
                    open.set(false);
                } else {
                    let selected_page_id = page_id.get();
                    if selected_page_id != 0_i64 {
                        let request_folder = next_folder.clone();
                        let target_page = selected_page_id;
                        folder.set(next_folder.clone());
                        busy.set(true);
                        error.set("".to_owned());
                        let failed_target_page = selected_page_id;
                        let saved_target_page = selected_page_id;
                        let failed_previous_folder = previous_folder.clone();
                        let saved_previous_folder = previous_folder.clone();
                        let _failed = || {
                            if !browser.is_disposed() {
                                if failed_page_id.get() == failed_target_page {
                                    failed_busy.set(false);
                                    failed_folder.set(failed_previous_folder.clone());
                                    failed_error.set(
                                        "Couldn't reach the server. Check your connection and try again."
                                            .to_owned(),
                                    );
                                }
                            }
                        };
                        let _save = async || {
                            if browser.is_disposed() {
                                return;
                            }
                            let outcome =
                                commit_move_page(account, target_page, request_folder.clone())
                                    .await;
                            if !browser.is_disposed() {
                                if page_id.get() == saved_target_page {
                                    busy.set(false);
                                    if outcome.status.is_ok() {
                                        error.set("".to_owned());
                                        open.set(false);
                                        revision.increment();
                                    } else {
                                        folder.set(saved_previous_folder.clone());
                                        error.set(outcome.status.unwrap_err());
                                    }
                                }
                            }
                        };
                        browser.microtask(|| {
                        if !browser.is_disposed() {
                            raw!(
                                "Promise.resolve().then(()=>${_save}()).catch(()=>${_failed}());",
                                ()
                            );
                        }
                    });
                    }
                }
            }
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:change",
        handler.into_evaluated_and_js().1,
    );
    attrs
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

#[allow(clippy::too_many_arguments)]
fn move_picker_trigger(
    cx: &Cx,
    page_id: i64,
    title: String,
    identifier: String,
    folder_id: Option<i64>,
    state: MovePickerTriggerState,
    can_edit: bool,
) -> Attributes {
    if !can_edit {
        return Attributes::with_capacity(0);
    }
    let folder = folder_id.map_or_else(String::new, |id| id.to_string());
    let prefix = format!("Couldn't move {identifier}: ");
    let browser = browser::bindings();
    let MovePickerTriggerState {
        open,
        page_id: selected_page,
        page_title: selected_title,
        folder: selected_folder,
        busy,
        error,
        error_prefix,
    } = state;
    let click_open = open.clone();
    let click_page = selected_page.clone();
    let click_title = selected_title.clone();
    let click_folder = selected_folder.clone();
    let click_error = error.clone();
    let click_prefix = error_prefix.clone();
    let click_busy = busy.clone();
    let click_handler = expr!(|event: Event| {
        if !browser.is_disposed() {
            event.stop_propagation();
            if !click_busy.get() {
                click_page.set(page_id);
                click_title.set(title.clone());
                click_folder.set(folder.clone());
                click_error.set("".to_owned());
                click_prefix.set(prefix.clone());
                click_open.set(true);
            }
        }
    });
    let key_page = selected_page;
    let key_title = selected_title;
    let key_folder = selected_folder;
    let key_error = error;
    let key_prefix = error_prefix;
    let key_open = open;
    let key_busy = busy;
    let key_handler = expr!(|event: Event| {
        if !browser.is_disposed() {
            if event.key == "Enter" {
                event.prevent_default();
                event.stop_propagation();
                if !key_busy.get() {
                    key_page.set(page_id);
                    key_title.set(title.clone());
                    key_folder.set(folder.clone());
                    key_error.set("".to_owned());
                    key_prefix.set(prefix.clone());
                    key_open.set(true);
                }
            }
        }
    });
    let mut attrs = Attributes::with_capacity(2);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        click_handler.into_evaluated_and_js().1,
    );
    attrs.insert(
        cx,
        "data-topcoat-on:keydown",
        key_handler.into_evaluated_and_js().1,
    );
    attrs
}

type MovePickerSignals = (
    Signal<bool>,
    Signal<i64>,
    Signal<String>,
    Signal<String>,
    Signal<bool>,
    Signal<String>,
    Signal<String>,
);

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
        can_edit: bool,
        query: String,
        status: String,
        label: String,
        tab: String,
        folder: String,
        revision: usize,
        move_state: MovePickerSignals,
    ) -> topcoat::Result<impl View> {
        let _ = revision;
        let (
            move_open,
            move_page_id,
            move_page_title,
            move_folder,
            move_busy,
            move_error,
            move_error_prefix,
        ) = move_state;
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
        let show_move = can_edit && !searching && tab == "browse";
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
        let has_pages = !pages.is_empty();
        let page_rows = pages
            .into_iter()
            .map(|(page, href, preview)| {
                let action = move_picker_trigger(
                    cx,
                    page.id,
                    page.title.clone(),
                    page.identifier.clone(),
                    page.folder_id,
                    MovePickerTriggerState {
                        open: move_open.clone(),
                        page_id: move_page_id.clone(),
                        page_title: move_page_title.clone(),
                        folder: move_folder.clone(),
                        busy: move_busy.clone(),
                        error: move_error.clone(),
                        error_prefix: move_error_prefix.clone(),
                    },
                    can_edit,
                );
                view! {
                    cx =>
                    <li
                        data-native-page-row=(page.id.to_string())
                        class="border-b border-solid border-[var(--border)] last:border-b-0"
                    >
                        <div class="group flex items-center gap-2">
                            <a
                                class="native-pages__row flex min-w-0 flex-1 flex-col gap-1 rounded-md px-3 py-3 no-underline hover:bg-[var(--bg-subtle)]"
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
                                    <span class="mt-0.5 flex gap-1.5">
                                        for item in page.labels {
                                            <span
                                                class="rounded bg-[var(--bg-subtle)] px-1.5 py-0.5 text-micro text-[var(--text-muted)]"
                                            >
                                                (item)
                                            </span>
                                        }
                                    </span>
                                }
                            </a>
                            if show_move {
                                <span
                                    data-native-page-move=""
                                    role="button"
                                    tabindex="0"
                                    title="Move to folder…"
                                    aria-label="Move to folder…"
                                    class="shrink-0 cursor-pointer text-[var(--text-faint)] opacity-0 transition hover:text-[var(--accent)] group-hover:opacity-100 pointer-coarse:opacity-100"
                                    (action)
                                >
                                    (icons::project_icon(cx, Some("lucide:FolderOpen"), 13))
                                </span>
                            }
                        </div>
                    </li>
                }
                .boxed()
            })
            .collect::<Vec<_>>();
        Ok(view! {
            cx =>
            if !has_pages {
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
                    for row in page_rows {
                        (row)
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
