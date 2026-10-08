//! Shared Pages list folder picker and its owner-scoped handlers.
use super::super::super::runtime::signal_vec::{SignalVecExt, VecPositionExt};
use super::super::{browser, icons};
use super::actions::move_to_folder as commit_move_page;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr},
    view::{Attributes, BoxView, ViewExt, view},
};

pub(super) type Signals = (
    Signal<bool>,
    Signal<i64>,
    Signal<String>,
    Signal<String>,
    Signal<bool>,
    Signal<String>,
    Signal<String>,
    Signal<usize>,
    Signal<Vec<i64>>,
);

#[derive(Clone)]
pub(super) struct State {
    pub(super) open: Signal<bool>,
    pub(super) page_id: Signal<i64>,
    pub(super) page_title: Signal<String>,
    pub(super) folder: Signal<String>,
    pub(super) busy: Signal<bool>,
    pub(super) error: Signal<String>,
    pub(super) error_prefix: Signal<String>,
    pub(super) revision: Signal<usize>,
    pub(super) expanded: Signal<Vec<i64>>,
}

pub(super) fn state_signals(state: &State) -> Signals {
    (
        state.open.clone(),
        state.page_id.clone(),
        state.page_title.clone(),
        state.folder.clone(),
        state.busy.clone(),
        state.error.clone(),
        state.error_prefix.clone(),
        state.revision.clone(),
        state.expanded.clone(),
    )
}

pub(super) fn dialog<'a>(
    cx: &'a Cx,
    account: i64,
    state: State,
    folders: &[(i64, String)],
) -> BoxView<'a> {
    let State {
        open,
        page_title,
        folder,
        busy,
        error,
        error_prefix,
        expanded: _,
        ..
    } = state.clone();
    let browser = browser::bindings();
    let change = change(cx, account, state);
    let folder_options = folders
        .iter()
        .map(|(id, name)| {
            let value = id.to_string();
            let name = name.clone();
            let selected = folder.clone();
            view! {
                cx =>
                <option value=(value.clone()) :selected=$(selected.get() == value)>
                    (name)
                </option>
            }
            .boxed()
        })
        .collect::<Vec<_>>();
    view! {
        cx =>
        <div
            data-native-page-move-backdrop=""
            class="fixed inset-0 z-[100] flex items-start justify-center bg-black/25 px-2 pt-[14dvh]"
            :hidden=$(!open.get())
            @click=$(|_event: Event| {
                if !browser.is_disposed() {
                    if !busy.get() {
                        if raw!(
                            "cx.hydrate(${_event}.inner.target === ${_event}.inner.currentTarget)",
                            false,
                        ) {
                            open.set(false);
                        }
                    }
                }
            })
            @keydown=$(|event: Event| {
                if !browser.is_disposed() {
                    if event.key == "Escape" {
                        if !busy.get() {
                            open.set(false);
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
                        class="m-0 flex flex-1 items-center gap-2 text-body-lg font-semibold text-[var(--text)]"
                    >
                        (icons::project_icon(cx, Some("lucide:FolderOpen"), 16))
                        "Move to folder"
                    </h2>
                    <span class="truncate text-caption text-[var(--text-faint)]">
                        $(page_title.get())
                    </span>
                    <button
                        type="button"
                        aria-label="Close move folder picker"
                        data-native-page-move-cancel=""
                        class="size-7 rounded-md border-0 bg-transparent text-[var(--text-muted)] hover:bg-[var(--bg-subtle)] hover:text-[var(--text)] disabled:opacity-50"
                        :disabled=$(busy.get())
                        @click=$(|_event: Event| {
                            if !browser.is_disposed() {
                                if !busy.get() {
                                    open.set(false);
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
                        :value=$(folder.get())
                        :disabled=$(busy.get())
                        (change)
                    >
                        <option value="" :selected=$(folder.get().is_empty())>
                            "No folder / root"
                        </option>
                        for option in folder_options {
                            (option)
                        }
                    </select>
                    <p
                        data-native-page-move-error=""
                        role="alert"
                        class="mt-3 text-body-sm text-[var(--error)]"
                        :hidden=$(error.get().is_empty())
                    >
                        $(error_prefix.get())
                        $(error.get())
                    </p>
                </div>
            </div>
        </div>
    }
    .boxed()
}

fn change(cx: &Cx, account: i64, state: State) -> Attributes {
    let State {
        open,
        page_id,
        page_title: _,
        folder,
        busy,
        error,
        error_prefix: _,
        revision,
        expanded,
    } = state;
    let browser = browser::bindings();
    let id_zero = 0_i64;
    let failed_page_id = page_id.clone();
    let failed_folder = folder.clone();
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let handler = expr!(async |event: Event| {
        if !browser.is_disposed() {
            if !busy.get() {
                let next_folder = event.target.value.to_owned();
                let destination_id = browser.positive_i64(next_folder.clone(), id_zero);
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
                                        if destination_id > 0_i64 {
                                            if expanded.get().position(destination_id).is_none() {
                                                expanded.push(destination_id);
                                            }
                                        }
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

pub(super) fn row_action(
    cx: &Cx,
    page_id: i64,
    title: String,
    identifier: String,
    folder_id: Option<i64>,
    state: State,
    can_edit: bool,
) -> Attributes {
    if !can_edit {
        return Attributes::with_capacity(0);
    }
    let folder = folder_id.map_or_else(String::new, |id| id.to_string());
    let prefix = format!("Couldn't move {identifier}: ");
    let browser = browser::bindings();
    let State {
        open,
        page_id: selected_page,
        page_title: selected_title,
        folder: selected_folder,
        busy,
        error,
        error_prefix,
        revision: _,
        expanded: _,
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
