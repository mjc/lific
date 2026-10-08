//! New-folder menu, inline composer, and owner-scoped handlers for Pages.
use super::super::super::runtime::whitespace::StrEcmaTrimExt;
use super::super::browser;
use super::actions::create_folder;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

#[derive(Clone)]
pub(super) struct State {
    menu_open: Signal<bool>,
    name: Signal<String>,
    composer_open: Signal<bool>,
    parent_folder: Signal<String>,
    busy: Signal<bool>,
    error: Signal<String>,
}

impl State {
    pub(super) fn new(cx: &Cx) -> Self {
        Self {
            menu_open: signal(cx, || false),
            name: signal(cx, String::new),
            composer_open: signal(cx, || false),
            parent_folder: signal(cx, String::new),
            busy: signal(cx, || false),
            error: signal(cx, String::new),
        }
    }
}

pub(super) struct Views<'a> {
    pub(super) menu: BoxView<'a>,
    pub(super) composer: BoxView<'a>,
    pub(super) error: BoxView<'a>,
}

pub(super) fn views<'a>(
    cx: &'a Cx,
    account: i64,
    project_id: i64,
    folder: Signal<String>,
    revision: Signal<usize>,
    state: State,
) -> Views<'a> {
    let create = create_attributes(cx, account, project_id, state.clone(), revision);
    let menu_toggle = menu_toggle_attributes(cx, state.menu_open.clone());
    let menu_item = menu_item_attributes(
        cx,
        state.menu_open.clone(),
        state.name.clone(),
        folder,
        state.parent_folder.clone(),
        state.composer_open.clone(),
        state.error.clone(),
    );
    let keydown = keydown_attributes(
        cx,
        state.name.clone(),
        state.composer_open.clone(),
        state.error.clone(),
        state.busy.clone(),
    );
    let blur = blur_attributes(
        cx,
        state.name.clone(),
        state.composer_open.clone(),
        state.busy.clone(),
    );
    let State {
        menu_open,
        name,
        composer_open,
        busy,
        error,
        ..
    } = state;
    let input_error = error.clone();

    Views {
        menu: view! {
            cx =>
            <div class="relative">
                <button
                    type="button"
                    aria-haspopup="menu"
                    :aria-expanded=$(if menu_open.get() { "true" } else { "false" })
                    class="text-body-sm px-3 py-1.5 rounded-md border border-solid border-[var(--border)] bg-[var(--bg)] text-[var(--text)]"
                    (menu_toggle)
                >
                    "New"
                </button>
                <div
                    role="menu"
                    aria-label="New page item"
                    class="absolute right-0 top-full z-10 mt-1 min-w-40 rounded-md border border-solid border-[var(--border)] bg-[var(--bg)] p-1 shadow-lg"
                    :hidden=$(!menu_open.get())
                >
                    <button
                        type="button"
                        role="menuitem"
                        class="w-full rounded px-2 py-1.5 text-left text-body-sm hover:bg-[var(--bg-subtle)]"
                        (menu_item)
                    >
                        "New folder"
                    </button>
                </div>
            </div>
        }
        .boxed(),
        composer: view! {
            cx =>
            <div
                data-native-folder-create=""
                class="flex items-center gap-2 mb-4"
                :hidden=$(!composer_open.get())
            >
                <input
                    aria-label="Folder name"
                    placeholder="Folder name"
                    autofocus=""
                    class="text-body-sm px-2.5 py-1.5 rounded-md border border-solid border-[var(--border)] bg-[var(--bg)] text-[var(--text)]"
                    :value=$(name.get())
                    @input=$(|event: Event| {
                        name.set(event.target.value.to_owned());
                        input_error.set("".to_owned());
                    })
                    (keydown)
                    (blur)
                />
                <button
                    id="native-pages-create-folder-button"
                    type="button"
                    class="text-body-sm px-3 py-1.5 rounded-md border-0 bg-[var(--accent)] text-[var(--accent-text)] disabled:opacity-50"
                    :disabled=$(if busy.get() {
                        true
                    } else {
                        name.get().trim().is_empty()
                    })
                    (create)
                >
                    $(if busy.get() { "Creating…" } else { "Create folder" })
                </button>
            </div>
        }
        .boxed(),
        error: view! {
            cx =>
            <div
                data-native-folder-create-error=""
                role="alert"
                class="text-body-sm text-[var(--error)] mb-3"
                :hidden=$(error.get().is_empty())
            >
                $(error.get())
            </div>
        }
        .boxed(),
    }
}

fn menu_toggle_attributes(cx: &Cx, open: Signal<bool>) -> Attributes {
    let handler = expr!(|_event: Event| open.set(!open.get()));
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

fn menu_item_attributes(
    cx: &Cx,
    menu_open: Signal<bool>,
    name: Signal<String>,
    current_folder: Signal<String>,
    parent_folder: Signal<String>,
    composer_open: Signal<bool>,
    error: Signal<String>,
) -> Attributes {
    let browser = browser::bindings();
    let focus_open = composer_open.clone();
    let handler = expr!(|_event: Event| {
        if !browser.is_disposed() {
            menu_open.set(false);
            name.set("".to_owned());
            error.set("".to_owned());
            parent_folder.set(current_folder.get());
            composer_open.set(true);
            let _focus = || {
                if !browser.is_disposed() {
                    if focus_open.get() {
                        raw!(
                            "document.querySelector(\"input[placeholder='Folder name']\")?.focus()",
                            ()
                        );
                    }
                }
            };
            raw!("requestAnimationFrame(()=>${_focus}());", ());
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

fn create_attributes(
    cx: &Cx,
    account: i64,
    project_id: i64,
    state: State,
    revision: Signal<usize>,
) -> Attributes {
    let State {
        name,
        composer_open,
        busy,
        error,
        parent_folder,
        ..
    } = state;
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let browser = browser::bindings();
    let handler = expr!(async |_event: Event| {
        if !browser.is_disposed() {
            if !busy.get() {
                let trimmed = name.get().trim_ecmascript().to_owned();
                if !trimmed.is_empty() {
                    busy.set(true);
                    composer_open.set(false);
                    error.set("".to_owned());
                    let _failed = || {
                        if !browser.is_disposed() {
                            failed_busy.set(false);
                            failed_error.set("Couldn't create the folder. Try again.".to_owned());
                        }
                    };
                    let _save = async || {
                        if !browser.is_disposed() {
                            let outcome =
                                create_folder(account, project_id, trimmed, parent_folder.get())
                                    .await;
                            if !browser.is_disposed() {
                                busy.set(false);
                                name.set("".to_owned());
                                if outcome.status.is_ok() {
                                    revision.increment();
                                } else {
                                    error.set(outcome.status.unwrap_err());
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

fn keydown_attributes(
    cx: &Cx,
    name: Signal<String>,
    composer_open: Signal<bool>,
    error: Signal<String>,
    busy: Signal<bool>,
) -> Attributes {
    let browser = browser::bindings();
    let cancel_name = name;
    let handler = expr!(|event: Event| {
        if !browser.is_disposed() {
            if event.key == "Escape" {
                event.prevent_default();
                if !busy.get() {
                    composer_open.set(false);
                    cancel_name.set("".to_owned());
                    error.set("".to_owned());
                }
            } else if event.key == "Enter" {
                event.prevent_default();
                raw!(
                    "document.querySelector('#native-pages-create-folder-button')?.click()",
                    ()
                );
            }
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:keydown",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

fn blur_attributes(
    cx: &Cx,
    name: Signal<String>,
    composer_open: Signal<bool>,
    busy: Signal<bool>,
) -> Attributes {
    let browser = browser::bindings();
    let handler = expr!(|_event: Event| {
        if !browser.is_disposed() {
            if !busy.get() {
                if name.get().trim_ecmascript().is_empty() {
                    composer_open.set(false);
                    name.set("".to_owned());
                }
            }
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:blur",
        handler.into_evaluated_and_js().1,
    );
    attrs
}
