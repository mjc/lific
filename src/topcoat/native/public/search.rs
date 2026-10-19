//! Public issue and page search controls shared by their native layouts.

use topcoat::{
    context::Cx,
    runtime::{Event, Js, Signal, expr},
    view::{BoxView, ViewExt, view},
};

use super::super::browser;
use super::super::icons::{self, UiIcon};

#[derive(Clone)]
pub(super) struct SearchPersistence {
    pub(super) wire: Signal<String>,
    pub(super) key: String,
    pub(super) field: String,
}

#[derive(Clone, Copy)]
pub(super) enum SearchKind {
    Issues,
    Pages,
}

impl SearchKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Issues => "issues",
            Self::Pages => "pages",
        }
    }

    pub(super) fn input_id(self) -> &'static str {
        match self {
            Self::Issues => "public-issue-search",
            Self::Pages => "public-page-search",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Issues => "Search issues",
            Self::Pages => "Search pages",
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            Self::Issues => "Search issues...",
            Self::Pages => "Search pages...",
        }
    }

    fn overlay_class(self) -> &'static str {
        match self {
            Self::Issues => {
                "max-md:absolute max-md:inset-0 max-md:z-20 max-md:bg-[var(--chrome)] max-md:flex max-md:items-center max-md:px-3 max-md:py-1.5"
            }
            Self::Pages => {
                "max-sm:absolute max-sm:inset-0 max-sm:z-20 max-sm:bg-[var(--chrome)] max-sm:flex max-sm:items-center max-sm:px-3 max-sm:py-1.5"
            }
        }
    }

    fn input_width_class(self) -> &'static str {
        match self {
            Self::Issues => "w-full md:w-[200px]",
            Self::Pages => "w-full sm:w-[200px]",
        }
    }
}

pub(super) struct SearchControl {
    pub(super) kind: SearchKind,
    pub(super) query: Signal<String>,
    pub(super) expanded: Signal<bool>,
    pub(super) persistence: Option<SearchPersistence>,
}

struct SearchEvents {
    query: Signal<String>,
    expanded: Signal<bool>,
    input_id: String,
    persistence: Option<SearchPersistence>,
}

#[derive(Clone, Copy)]
enum SearchAction {
    Input,
    Escape,
}

fn query_handler(events: SearchEvents, action: SearchAction) -> Js {
    let SearchEvents {
        query,
        expanded,
        input_id,
        persistence,
    } = events;
    let (wire, storage_key, field, persist) = match persistence {
        Some(persistence) => (persistence.wire, persistence.key, persistence.field, true),
        None => (query.clone(), String::new(), String::new(), false),
    };
    let is_input = match action {
        SearchAction::Input => true,
        SearchAction::Escape => false,
    };
    let browser = browser::bindings();
    expr!(|event: Event| {
        if !browser.is_disposed() {
            let _write_query = |value: String| {
                query.set(value.clone());
                if persist {
                    let next = browser.json_set_string(wire.get(), field.clone(), value);
                    wire.set(next.clone());
                    browser.store(storage_key.clone(), next);
                }
            };
            if is_input {
                browser.call1(_write_query, event.target.value.to_owned());
            } else if event.key == "Escape" {
                event.prevent_default();
                browser.call1(_write_query, "".to_owned());
                expanded.set(false);
                browser.blur_id(input_id.clone());
            }
        }
    })
    .into_evaluated_and_js()
    .1
}

pub(super) fn open_handler(
    expanded: Signal<bool>,
    input_id: String,
) -> topcoat::runtime::Expr<impl FnOnce(Event)> {
    let browser = browser::bindings();
    let frame_expanded = expanded.clone();
    expr!(|_event: Event| {
        if !browser.is_disposed() {
            expanded.set(true);
            let input_id = input_id.clone();
            browser.request_frame(|| {
                if !browser.is_disposed() {
                    if frame_expanded.get() {
                        browser.focus_id(input_id);
                    }
                }
            });
        }
    })
}

pub(super) fn control<'a>(cx: &'a Cx, control: SearchControl) -> BoxView<'a> {
    let browser = browser::bindings();
    let kind = control.kind;
    let input_id = kind.input_id().to_owned();
    let query = control.query;
    let expanded = control.expanded;
    let open = open_handler(expanded.clone(), input_id.clone());

    let input = query_handler(
        SearchEvents {
            query: query.clone(),
            expanded: expanded.clone(),
            input_id: input_id.clone(),
            persistence: control.persistence.clone(),
        },
        SearchAction::Input,
    );

    let blur_query = query.clone();
    let blur_expanded = expanded.clone();
    let blur_browser = browser;
    let blur = expr!(|_event: Event| {
        if !blur_browser.is_disposed() {
            if blur_query.get().is_empty() {
                blur_expanded.set(false);
            }
        }
    });

    let escape = query_handler(
        SearchEvents {
            query: query.clone(),
            expanded: expanded.clone(),
            input_id: input_id.clone(),
            persistence: control.persistence,
        },
        SearchAction::Escape,
    );

    let label = kind.label();
    let kind_name = kind.as_str();
    let overlay_class = format!("[&[hidden]]:hidden {}", kind.overlay_class());
    let input_width_class = kind.input_width_class();
    let placeholder = kind.placeholder();
    let input_class = format!(
        "{input_width_class} pl-7 pr-2 py-1 text-body-sm rounded-md border border-solid border-[var(--border)] bg-[var(--surface)] text-[var(--text)] placeholder:text-[var(--text-faint)] focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_var(--accent-subtle)] outline-none transition-colors"
    );
    let button_expanded = expanded.clone();
    let input_expanded = expanded;
    view! {
        cx =>
        <div class="flex items-center" data-native-search=(kind_name)>
            <button
                type="button"
                aria-label=(label)
                data-native-search-open=(kind_name)
                :aria-expanded=$(button_expanded.get())
                :hidden=$(button_expanded.get())
                class="[&[hidden]]:hidden touch-target size-7 flex items-center justify-center rounded-md text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)] transition-colors"
                data-topcoat-on:click=(open.into_evaluated_and_js().1)
            >
                (icons::ui_icon(cx, UiIcon::Search, 14))
            </button>
            <div class=(overlay_class) :hidden=$(input_expanded.get() == false)>
                <div class="relative w-full sm:w-auto">
                    <span
                        class="absolute left-2 top-1/2 -translate-y-1/2 pointer-events-none text-[var(--text-faint)]"
                    >
                        (icons::ui_icon(cx, UiIcon::Search, 12))
                    </span>
                    <input
                        id=(input_id)
                        type="text"
                        enterkeyhint="search"
                        autocomplete="off"
                        placeholder=(placeholder)
                        :value=$(query.get())
                        class=(input_class)
                        data-topcoat-on:input=(input)
                        data-topcoat-on:blur=(blur.into_evaluated_and_js().1)
                        data-topcoat-on:keydown=(escape)
                    />
                </div>
            </div>
        </div>
    }
    .boxed()
}
