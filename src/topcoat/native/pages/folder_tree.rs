//! Pages folder tree rows and list-owned expand/delete handlers.
use super::super::super::runtime::signal_vec::{SignalVecExt, VecPositionExt};
use super::super::{browser, icons};
use super::actions::delete_folder as commit_delete_folder;
use crate::db::models::Folder;
use std::collections::HashMap;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr},
    view::{Attributes, BoxView, ViewExt, view},
};

pub(super) type State = (
    Signal<Vec<i64>>,
    Signal<bool>,
    Signal<String>,
    Signal<usize>,
);
pub(super) struct Render<'data, 'view> {
    pub(super) folders: &'data [Folder],
    pub(super) page_rows: HashMap<Option<i64>, Vec<BoxView<'view>>>,
    pub(super) expanded: &'data [i64],
    pub(super) expanded_signal: Signal<Vec<i64>>,
    pub(super) can_edit: bool,
    pub(super) revision: usize,
    pub(super) parent: Option<i64>,
}

pub(super) fn state(cx: &Cx, revision: Signal<usize>, initially_expanded: Vec<i64>) -> State {
    (
        topcoat::runtime::signal(cx, move || initially_expanded),
        topcoat::runtime::signal(cx, || false),
        topcoat::runtime::signal(cx, String::new),
        revision,
    )
}

pub(super) fn handlers(cx: &Cx, account: i64, project_id: i64, state: &State) -> Attributes {
    let (expanded, busy, error, revision) = state.clone();
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let done_busy = busy.clone();
    let done_error = error.clone();
    let done_revision = revision.clone();
    let browser = browser::bindings();
    let revision_zero = 0_usize;
    let click = expr!(async |_event: Event| {
        if !browser.is_disposed() {
            let delete_id_value = raw!(
                "cx.hydrate(${_event}.inner.target?.closest('[data-native-page-folder-delete]')?.getAttribute('data-folder-id') || '')",
                "".to_owned()
            );
            let delete_id = browser.positive_i64(delete_id_value, 0_i64);
            if delete_id > 0_i64 {
                raw!("${_event}.inner.stopPropagation()", ());
                if !busy.get() {
                    let name = raw!(
                        "cx.hydrate(${_event}.inner.target.closest('[data-native-page-folder-delete]')?.getAttribute('data-folder-name') || '')",
                        "".to_owned()
                    );
                    let row_revision = raw!(
                        "cx.hydrate({...${revision_zero}.dehydrate(),v:String(Number(${_event}.inner.target.closest('[data-native-page-folder-delete]')?.getAttribute('data-folder-revision')))})",
                        0_usize
                    );
                    if revision.get() == row_revision {
                        let failed_name = name.clone();
                        let save_name = name.clone();
                        busy.set(true);
                        error.set("".to_owned());
                        let _failed = || {
                            if !browser.is_disposed() {
                                failed_busy.set(false);
                                failed_error.set("Couldn't delete ".to_owned());
                                failed_error.push_str(failed_name.clone());
                                failed_error.push_str(
                                    ": Couldn't reach the server. Check your connection and try again.".to_owned(),
                                );
                            }
                        };
                        let _save = async || {
                            if !browser.is_disposed() {
                                let outcome =
                                    commit_delete_folder(account, project_id, delete_id).await;
                                if !browser.is_disposed() {
                                    done_busy.set(false);
                                    if outcome.status.is_ok() {
                                        done_revision.increment();
                                    } else {
                                        let reason = outcome.status.unwrap_err();
                                        done_error.set("Couldn't delete ".to_owned());
                                        done_error.push_str(save_name.clone());
                                        done_error.push_str(": ".to_owned());
                                        done_error.push_str(reason);
                                    }
                                }
                            }
                        };
                        raw!(
                            "Promise.resolve().then(() => ${_save}()).catch(() => ${_failed}());",
                            ()
                        );
                    }
                }
            } else {
                let folder_id_value = raw!(
                    "cx.hydrate(${_event}.inner.target?.closest('[data-native-page-folder-toggle]')?.getAttribute('data-folder-id') || '')",
                    "".to_owned()
                );
                let id = browser.positive_i64(folder_id_value, 0_i64);
                if id > 0_i64 {
                    let row_revision = raw!(
                        "cx.hydrate({...${revision_zero}.dehydrate(),v:String(Number(${_event}.inner.target.closest('[data-native-page-folder-toggle]')?.getAttribute('data-folder-revision')))})",
                        0_usize
                    );
                    if revision.get() == row_revision {
                        let current = expanded.get();
                        let index = current.position(id);
                        if index.is_some() {
                            expanded.remove(index.unwrap());
                        } else {
                            expanded.push(id);
                        }
                    }
                }
            }
        }
    });
    let keydown = expr!(|event: Event| {
        if !browser.is_disposed() {
            let delete_control = raw!(
                "cx.hydrate(Boolean(${event}.inner.target?.closest('[data-native-page-folder-delete]')))",
                false
            );
            if !delete_control {
                if event.key == "Enter" {
                    let folder_id_value = raw!(
                        "cx.hydrate(${event}.inner.target?.closest('[data-native-page-folder-toggle]')?.getAttribute('data-folder-id') || '')",
                        "".to_owned()
                    );
                    let id = browser.positive_i64(folder_id_value, 0_i64);
                    if id > 0_i64 {
                        let row_revision = raw!(
                            "cx.hydrate({...${revision_zero}.dehydrate(),v:String(Number(${event}.inner.target.closest('[data-native-page-folder-toggle]')?.getAttribute('data-folder-revision')))})",
                            0_usize
                        );
                        if revision.get() == row_revision {
                            event.prevent_default();
                            let current = expanded.get();
                            let index = current.position(id);
                            if index.is_some() {
                                expanded.remove(index.unwrap());
                            } else {
                                expanded.push(id);
                            }
                        }
                    }
                }
            }
        }
    });
    let mut attrs = Attributes::with_capacity(2);
    attrs.insert(cx, "data-topcoat-on:click", click.into_evaluated_and_js().1);
    attrs.insert(
        cx,
        "data-topcoat-on:keydown",
        keydown.into_evaluated_and_js().1,
    );
    attrs
}

pub(super) fn row<'a>(
    cx: &'a Cx,
    folder: &Folder,
    expanded: Signal<Vec<i64>>,
    can_edit: bool,
    revision: usize,
) -> BoxView<'a> {
    let id = folder.id.to_string();
    let folder_id = folder.id;
    let revision = revision.to_string();
    let name = folder.name.clone();
    view! {
        cx =>
        <div
            data-native-page-folder-row=(id.clone())
            data-native-page-folder-toggle=(id.clone())
            data-folder-id=(id.clone())
            data-folder-revision=(revision.clone())
            data-folder-name=(name.clone())
            role="button"
            tabindex="0"
            aria-label=(name.clone())
            :aria-expanded=$(if expanded.get().position(folder_id).is_some() {
                "true"
            } else {
                "false"
            })
            class="group flex items-center gap-1 rounded-md px-2 py-1.5 hover:bg-[var(--bg-subtle)]"
        >
            <span
                class="flex min-w-0 flex-1 items-center gap-2 text-body-sm text-[var(--text)]"
            >
                if expanded.get().contains(&folder_id) {
                    (icons::project_icon(cx, Some("lucide:FolderOpen"), 18))
                } else {
                    (icons::project_icon(cx, Some("lucide:FolderClosed"), 18))
                }
                <span class="truncate">(name.clone())</span>
            </span>
            if can_edit {
                <button
                    type="button"
                    data-native-page-folder-delete=(id.clone())
                    data-folder-id=(id)
                    data-folder-revision=(revision)
                    data-folder-name=(name.clone())
                    title="Delete folder"
                    aria-label="Delete folder"
                    class="flex size-6 shrink-0 items-center justify-center rounded border-0 bg-transparent p-0 text-[var(--text-faint)] opacity-0 transition-opacity hover:bg-[var(--error-bg)] hover:text-[var(--error)] group-hover:opacity-100 pointer-coarse:opacity-100"
                >
                    (icons::ui_icon(cx, icons::UiIcon::Delete, 13))
                </button>
            }
        </div>
    }
    .boxed()
}

pub(super) fn render<'view, 'data>(
    cx: &'view Cx,
    mut state: Render<'data, 'view>,
) -> BoxView<'view> {
    let parent = state.parent;
    let mut roots = level(cx, parent, &mut state);
    roots.extend(state.page_rows.remove(&parent).unwrap_or_default());
    view! {
        cx =>
        <ul
            class="native-pages__folder-tree list-none p-0 m-0 space-y-1"
            aria-label="Pages and folders"
        >
            for node in roots {
                (node)
            }
        </ul>
    }
    .boxed()
}

fn level<'view, 'data>(
    cx: &'view Cx,
    parent: Option<i64>,
    state: &mut Render<'data, 'view>,
) -> Vec<BoxView<'view>> {
    let folders = state.folders;
    folders
        .iter()
        .filter(|folder| folder.parent_id == parent)
        .map(|folder| {
            let mut children = level(cx, Some(folder.id), state);
            if state.expanded.contains(&folder.id) {
                children.extend(state.page_rows.remove(&Some(folder.id)).unwrap_or_default());
            }
            let folder_row = row(
                cx,
                &folder,
                state.expanded_signal.clone(),
                state.can_edit,
                state.revision,
            );
            let id = folder.id.to_string();
            let is_expanded = state.expanded.contains(&folder.id);
            view! {
                cx =>
                <li class="list-none">
                    (folder_row)
                    if is_expanded {
                        <ul
                            data-native-page-folder-children=(id)
                            class="ml-5 list-none border-l border-solid border-[var(--border)] pl-2"
                        >
                            for child in children {
                                (child)
                            }
                        </ul>
                    }
                </li>
            }
            .boxed()
        })
        .collect()
}

pub(super) fn empty_error<'a>(cx: &'a Cx, state: &State) -> BoxView<'a> {
    let error = state.2.clone();
    let revision = state.3.clone();
    view! {
        cx =>
        <div
            role="alert"
            data-native-page-folder-error=""
            :data-revision=$(revision.get())
            class="text-body-sm text-[var(--error)]"
            :hidden=$(error.get().is_empty())
        >
            $(error.get())
        </div>
    }
    .boxed()
}
