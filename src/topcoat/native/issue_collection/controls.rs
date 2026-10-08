//! Native filter, search, sort and display state shared by both issue layouts.
use super::super::{browser, icons, navigation};
use super::{data::Collection, model, persistence};
use topcoat::{
    context::Cx,
    runtime::{Event, Expr, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

const BUTTON: &str = "flex items-center gap-1.5 rounded-md px-2 py-1.5 text-caption font-medium text-[var(--text-muted)] hover:bg-[var(--bg-subtle)] hover:text-[var(--text)] aria-pressed:bg-[var(--accent-subtle)] aria-pressed:text-[var(--accent)]";

pub(super) struct State {
    pub wire: Signal<String>,
    pub tab: Signal<String>,
    pub lane: Signal<String>,
    pub groups: Signal<String>,
    pub hidden: Signal<String>,
    pub lanes: Signal<String>,
    pub columns: Signal<String>,
    pub storage_key: String,
    tab_key: String,
    project: String,
    hydrated: Signal<bool>,
    filter_open: Signal<bool>,
    sort_open: Signal<bool>,
    display_open: Signal<bool>,
    search_open: Signal<bool>,
}

impl State {
    pub(super) fn new(cx: &Cx, project: &str, project_id: i64) -> Self {
        let stored = |key, value: &'static str| signal(&cx.keyed(key), || value.to_owned());
        Self {
            wire: stored("state", persistence::DEFAULTS),
            tab: stored("tab", "all"),
            lane: stored("lane", "none"),
            groups: stored("groups", "[]"),
            hidden: stored("hidden", "[]"),
            lanes: stored("lanes", "[]"),
            columns: stored("columns", "[]"),
            storage_key: format!("lific:list:state:{project}"),
            tab_key: format!("lific:subtab:issues:{project_id}"),
            project: project.to_owned(),
            hydrated: signal(&cx.keyed("hydrated"), || false),
            filter_open: signal(&cx.keyed("filter-open"), || false),
            sort_open: signal(&cx.keyed("sort-open"), || false),
            display_open: signal(&cx.keyed("display-open"), || false),
            search_open: signal(&cx.keyed("search-open"), || false),
        }
    }
}

fn event(cx: &Cx, id: &str, name: &str, handler: topcoat::runtime::Js) -> Attributes {
    let mut attrs = Attributes::with_capacity(2);
    attrs.insert(cx, "data-native-issue-control", id.to_owned());
    attrs.insert(cx, format!("data-topcoat-on:{name}"), handler);
    attrs
}

fn filter_count(wire: Signal<String>) -> Expr<usize> {
    let browser = browser::bindings();
    expr!(
        (if browser
            .json_string(wire.get(), "filterStatus".to_owned(), "".to_owned())
            .is_empty()
        {
            0_usize
        } else {
            1_usize
        }) + (if browser
            .json_string(wire.get(), "filterPriority".to_owned(), "".to_owned())
            .is_empty()
        {
            0_usize
        } else {
            1_usize
        }) + (if browser
            .json_string(wire.get(), "filterLabel".to_owned(), "".to_owned())
            .is_empty()
        {
            0_usize
        } else {
            1_usize
        }) + (if browser
            .json_string(wire.get(), "filterModule".to_owned(), "".to_owned())
            .is_empty()
        {
            0_usize
        } else {
            1_usize
        })
    )
}

fn choose(cx: &Cx, state: &State, field: &str, value: &str, toggle: bool, id: &str) -> Attributes {
    let browser = browser::bindings();
    let wire = state.wire.clone();
    let key = state.storage_key.clone();
    let field = field.to_owned();
    let value = value.to_owned();
    let handler = expr!(|_event: Event| {
        if !browser.is_disposed() {
            let next = if toggle {
                if browser.json_string(wire.get(), field.clone(), "".to_owned()) == value {
                    "".to_owned()
                } else {
                    value.clone()
                }
            } else {
                value.clone()
            };
            let updated = browser.json_set_string(wire.get(), field.clone(), next);
            wire.set(updated.clone());
            browser.store(key.clone(), updated);
        }
    });
    event(cx, id, "click", handler.into_evaluated_and_js().1)
}

pub(super) fn clear_for(cx: &Cx, wire: Signal<String>, key: String) -> Attributes {
    let browser = browser::bindings();
    let handler = expr!(|_event: Event| {
        if !browser.is_disposed() {
            let next =
                browser.json_set_string(wire.get(), "filterStatus".to_owned(), "".to_owned());
            let next = browser.json_set_string(next, "filterPriority".to_owned(), "".to_owned());
            let next = browser.json_set_string(next, "filterLabel".to_owned(), "".to_owned());
            let next = browser.json_set_string(next, "filterModule".to_owned(), "".to_owned());
            let next = browser.json_set_string(next, "searchQuery".to_owned(), "".to_owned());
            wire.set(next.clone());
            browser.store(key.clone(), next);
        }
    });
    event(cx, "clear", "click", handler.into_evaluated_and_js().1)
}

fn mounted(cx: &Cx, state: &State, layout: &str) -> Attributes {
    let browser = browser::bindings();
    let wire = state.wire.clone();
    let hydrated = state.hydrated.clone();
    let tab = state.tab.clone();
    let lane = state.lane.clone();
    let groups = state.groups.clone();
    let hidden = state.hidden.clone();
    let lanes = state.lanes.clone();
    let columns = state.columns.clone();
    let search_open = state.search_open.clone();
    let filter_open = state.filter_open.clone();
    let sort_open = state.sort_open.clone();
    let display_open = state.display_open.clone();
    let key = state.storage_key.clone();
    let tab_key = state.tab_key.clone();
    let project = &state.project;
    let layout_key = format!("lific:list:layout:{project}");
    let group_key = format!("lific:list:collapsed:{project}");
    let hidden_key = format!("lific:board:hidden-statuses:{project}");
    let lane_key = format!("lific:board:lanes:{project}");
    let lanes_key = format!("lific:board:collapsed-lanes:{project}");
    let columns_key = format!("lific:board:collapsed-columns:{project}");
    let layout = layout.to_owned();
    let defaults = persistence::DEFAULTS.to_owned();
    let handler = expr!(|_mount: Event| {
        if !browser.is_disposed() {
            if !hydrated.get() {
                wire.set(browser.json_fields(browser.stored(key.clone()), defaults.clone()));
                search_open.set(
                    !browser
                        .json_string(wire.get(), "searchQuery".to_owned(), "".to_owned())
                        .is_empty(),
                );
                let saved_tab = browser.stored(tab_key.clone());
                if saved_tab == "recent" {
                    tab.set(saved_tab);
                } else if saved_tab == "open" {
                    tab.set(saved_tab);
                } else if saved_tab == "closed" {
                    tab.set(saved_tab);
                }
                let saved_lane = browser.stored(lane_key.clone());
                if saved_lane == "module" {
                    lane.set(saved_lane);
                } else if saved_lane == "priority" {
                    lane.set(saved_lane);
                }
                groups.set(browser.stored(group_key.clone()));
                hidden.set(browser.stored(hidden_key.clone()));
                lanes.set(browser.stored(lanes_key.clone()));
                columns.set(browser.stored(columns_key.clone()));
                hydrated.set(true);
            }
            browser.store(layout_key.clone(), layout.clone());
            let close = |event: Event| {
                if event.key.clone() == "Escape" {
                    if filter_open.get() {
                        event.prevent_default();
                        event.stop_propagation();
                        filter_open.set(false);
                        browser.focus_id("native-issue-filter-trigger".to_owned());
                    }
                    sort_open.set(false);
                    display_open.set(false);
                }
            };
            browser.window_listener("keydown".to_owned(), close);
        }
    });
    let mut attrs = event(cx, "owner", "mount", handler.into_evaluated_and_js().1);
    attrs.insert(cx, "data-native-issue-controls", state.project.clone());
    attrs
}

fn filter_option<'a>(
    cx: &'a Cx,
    state: &State,
    field: &str,
    value: &str,
    label: &str,
    description: &str,
) -> BoxView<'a> {
    let id = format!(
        "{}:{value}",
        match field {
            "filterStatus" => "status",
            "filterPriority" => "priority",
            "filterLabel" => "label",
            _ => "module",
        }
    );
    let attrs = choose(cx, state, field, value, !value.is_empty(), &id);
    let wire = state.wire.clone();
    let browser = browser::bindings();
    let field = field.to_owned();
    let value = value.to_owned();
    let label = label.to_owned();
    let description = description.to_owned();
    view! {
        cx =>
        <button
            type="button"
            class="flex w-full items-start gap-2.5 rounded-md px-2.5 py-2 text-left hover:bg-[var(--bg-subtle)] aria-pressed:bg-[var(--accent-subtle)]"
            :aria-pressed=$(browser.json_string(wire.get(), field.clone(), "".to_owned())
                == value)
            (attrs)
        >
            <span class="min-w-0 flex-1">
                <span
                    class="block text-body-sm font-medium capitalize text-[var(--text)]"
                >
                    (label)
                </span>
                <span class="block text-caption text-[var(--text-faint)]">
                    (description)
                </span>
            </span>
            <span
                :hidden=$(browser.json_string(wire.get(), field.clone(), "".to_owned())
                    != value)
            >
                (icons::ui_icon(cx, icons::UiIcon::Selected, 14))
            </span>
        </button>
    }.boxed()
}

pub(super) fn view<'a>(
    cx: &'a Cx,
    collection: &Collection,
    state: &State,
    layout: &str,
) -> BoxView<'a> {
    let mount = mounted(cx, state, layout);
    let browser = browser::bindings();
    let wire = state.wire.clone();
    let filter_open = state.filter_open.clone();
    let sort_open = state.sort_open.clone();
    let display_open = state.display_open.clone();
    let search_open = state.search_open.clone();
    let clear = clear_for(cx, state.wire.clone(), state.storage_key.clone());
    let count = filter_count(state.wire.clone());
    let stats = model::statistics(&collection.issues);
    let tallies = model::STATUSES
        .into_iter()
        .zip(stats.statuses)
        .filter(|(_, count)| *count > 0)
        .map(|(status, count)| {
            let name = status.as_str();
            let attrs = choose(
                cx,
                state,
                "filterStatus",
                name,
                true,
                &format!("tally:{name}"),
            );
            view! {
                cx =>
                <button
                    type="button"
                    class=(BUTTON)
                    aria-label=(format!("Filter {name} issues"))
                    (attrs)
                >
                    (icons::status_icon(cx, status, 14))
                    (count.to_string())
                </button>
            }
            .boxed()
        })
        .collect::<Vec<_>>();
    let status = [
        ("", "Any", "No status filter."),
        (
            "@unresolved",
            "Unresolved",
            "All open work — backlog, todo, and active.",
        ),
        ("backlog", "Backlog", "Captured, not yet planned."),
        ("todo", "Todo", "Planned and ready to start."),
        ("active", "Active", "In progress right now."),
        ("done", "Done", "Completed and shipped."),
        ("cancelled", "Cancelled", "Abandoned — won't be done."),
    ]
    .into_iter()
    .map(|(value, label, help)| filter_option(cx, state, "filterStatus", value, label, help))
    .collect::<Vec<_>>();
    let priority = [
        ("", "Any", "No priority filter."),
        ("urgent", "Urgent", "Drop everything."),
        ("high", "High", "Important — do soon."),
        ("medium", "Medium", "Normal priority."),
        ("low", "Low", "Nice to have, no rush."),
        ("none", "None", "No priority set."),
    ]
    .into_iter()
    .map(|(value, label, help)| filter_option(cx, state, "filterPriority", value, label, help))
    .collect::<Vec<_>>();
    let labels = std::iter::once(filter_option(cx, state, "filterLabel", "", "Any", ""))
        .chain(
            collection
                .labels
                .iter()
                .map(|label| filter_option(cx, state, "filterLabel", &label.name, &label.name, "")),
        )
        .collect::<Vec<_>>();
    let modules = std::iter::once(filter_option(cx, state, "filterModule", "", "Any", ""))
        .chain(collection.modules.iter().map(|module| {
            filter_option(
                cx,
                state,
                "filterModule",
                &module.name,
                &module.name,
                &module.description,
            )
        }))
        .collect::<Vec<_>>();
    let sorts = [
        ("priority", "Priority"),
        ("age", "Age"),
        ("updated", "Updated"),
        ("number", "Issue number"),
    ]
    .into_iter()
    .map(|(value, label)| {
        let wire = state.wire.clone();
        let key = state.storage_key.clone();
        let handler = expr!(|_event: Event| {
            if !browser.is_disposed() {
                let direction = if browser.json_string(
                    wire.get(),
                    "sortField".to_owned(),
                    "priority".to_owned(),
                ) == value
                {
                    if browser.json_string(wire.get(), "sortDir".to_owned(), "asc".to_owned())
                        == "asc"
                    {
                        "desc"
                    } else {
                        "asc"
                    }
                } else if value == "updated" {
                    "desc"
                } else {
                    "asc"
                };
                let next =
                    browser.json_set_string(wire.get(), "sortField".to_owned(), value.to_owned());
                let next =
                    browser.json_set_string(next, "sortDir".to_owned(), direction.to_owned());
                wire.set(next.clone());
                browser.store(key.clone(), next);
            }
        });
        let attrs = event(
            cx,
            &format!("sort:{value}"),
            "click",
            handler.into_evaluated_and_js().1,
        );
        view! { cx => <button type="button" class=(BUTTON) (attrs)>(label)</button> }.boxed()
    })
    .collect::<Vec<_>>();
    let groups = [
        ("status", "Status"),
        ("priority", "Priority"),
        ("module", "Module"),
        ("none", "None"),
    ]
    .into_iter()
    .map(|(value, label)| {
        let attrs = choose(
            cx,
            state,
            "groupBy",
            value,
            false,
            &format!("group:{value}"),
        );
        view! { cx => <button type="button" class=(BUTTON) (attrs)>(label)</button> }.boxed()
    })
    .collect::<Vec<_>>();
    let densities = [("compact", "Compact"), ("comfortable", "Comfortable")]
        .into_iter()
        .map(|(value, label)| {
            let attrs = choose(
                cx,
                state,
                "density",
                value,
                false,
                &format!("density:{value}"),
            );
            view! { cx => <button type="button" class=(BUTTON) (attrs)>(label)</button> }.boxed()
        })
        .collect::<Vec<_>>();
    let tab_buttons = [
        ("all", "All"),
        ("recent", "Recent"),
        ("open", "Open"),
        ("closed", "Closed"),
    ]
    .into_iter()
    .map(|(value, label)| {
        let tab = state.tab.clone();
        let key = state.tab_key.clone();
        let handler = expr!(|_event: Event| {
            if !browser.is_disposed() {
                tab.set(value.to_owned());
                browser.store(key.clone(), value.to_owned());
            }
        });
        let attrs = event(
            cx,
            &format!("tab:{value}"),
            "click",
            handler.into_evaluated_and_js().1,
        );
        let tab_count = match value {
            "all" => Some(stats.total),
            "open" => Some(stats.statuses[..3].iter().sum()),
            "closed" => Some(stats.statuses[3..].iter().sum()),
            _ => None,
        };
        view! {
            cx =>
            <button
                type="button"
                class=(BUTTON)
                :aria-pressed=$(tab.get() == value)
                (attrs)
            >
                (label)
                if let Some(count) = tab_count {
                    <span
                        data-native-issue-tab-count=(value)
                        class="text-micro text-[var(--text-faint)]"
                    >
                        (count.to_string())
                    </span>
                }
            </button>
        }
        .boxed()
    })
    .collect::<Vec<_>>();
    let input_wire = state.wire.clone();
    let input_key = state.storage_key.clone();
    let input = expr!(|event: Event| {
        if !browser.is_disposed() {
            let next = browser.json_set_string(
                input_wire.get(),
                "searchQuery".to_owned(),
                event.target.value.to_owned(),
            );
            input_wire.set(next.clone());
            browser.store(input_key.clone(), next);
        }
    });
    let escape_wire = state.wire.clone();
    let escape_key = state.storage_key.clone();
    let escape = expr!(|event: Event| {
        if !browser.is_disposed() {
            if event.key.clone() == "Escape" {
                event.prevent_default();
                let next = browser.json_set_string(
                    escape_wire.get(),
                    "searchQuery".to_owned(),
                    "".to_owned(),
                );
                escape_wire.set(next.clone());
                browser.store(escape_key.clone(), next);
                search_open.set(false);
            }
        }
    });
    let mut search = event(cx, "search", "input", input.into_evaluated_and_js().1);
    search.insert(
        cx,
        "data-topcoat-on:keydown",
        escape.into_evaluated_and_js().1,
    );
    let blur_wire = state.wire.clone();
    let blur_open = state.search_open.clone();
    let blur = expr!(|_event: Event| {
        if !browser.is_disposed() {
            if browser
                .json_string(blur_wire.get(), "searchQuery".to_owned(), "".to_owned())
                .is_empty()
            {
                blur_open.set(false);
            }
        }
    });
    search.insert(cx, "data-topcoat-on:blur", blur.into_evaluated_and_js().1);
    let open_state = state.search_open.clone();
    let open = expr!(|event: Event| {
        if !browser.is_disposed() {
            event.stop_propagation();
            open_state.set(true);
            browser.microtask(|| {
                if !browser.is_disposed() {
                    browser.focus_id("native-issue-search".to_owned());
                }
            });
        }
    });
    let search_open_attrs = event(cx, "search-open", "click", open.into_evaluated_and_js().1);
    let lane = state.lane.clone();
    let lane_key = format!("lific:board:lanes:{}", state.project);
    let has_labels = !collection.labels.is_empty();
    let has_modules = !collection.modules.is_empty();
    let identifier = &collection.project.identifier;
    let column_buttons = model::STATUSES
        .into_iter()
        .map(|status| {
            let browser = browser.clone();
            let name = status.as_str();
            let hidden = state.hidden.clone();
            let key = format!("lific:board:hidden-statuses:{}", state.project);
            let handler = expr!(|_event: Event| {
                if !browser.is_disposed() {
                    let included = !browser.json_array_contains(hidden.get(), name.to_owned());
                    let next =
                        browser.json_array_set_string(hidden.get(), name.to_owned(), included);
                    hidden.set(next.clone());
                    browser.store(key.clone(), next);
                }
            });
            let attrs = event(
                cx,
                &format!("column:{name}"),
                "click",
                handler.into_evaluated_and_js().1,
            );
            view! {
                cx =>
                <button
                    type="button"
                    class=(BUTTON)
                    :aria-pressed=$(!browser.json_array_contains(
                        hidden.get(),
                        name.to_owned(),
                    ))
                    (attrs)
                >
                    (icons::status_icon(cx, status, 12))
                    <span class="capitalize">(name)</span>
                </button>
            }
            .boxed()
        })
        .collect::<Vec<_>>();
    let project_name = collection.project.name.clone();
    let list_link = navigation::attrs(cx, &format!("/{identifier}/issues"));
    let board_link = navigation::attrs(cx, &format!("/{identifier}/board"));
    let layout = layout.to_owned();
    view! {
        cx =>
        <div
            class="flex w-full flex-wrap items-center gap-2 text-[var(--text)]"
            (mount)
        >
            <span class="truncate text-body-sm font-medium">(project_name)</span>
            <div class="flex rounded-md bg-[var(--bg)] p-0.5">
                <a
                    class=(BUTTON)
                    aria-current=(if layout == "list" { "page" } else { "false" })
                    (list_link)
                >
                    "List"
                </a>
                <a
                    class=(BUTTON)
                    aria-current=(if layout == "board" { "page" } else { "false" })
                    (board_link)
                >
                    "Board"
                </a>
            </div>
            <div
                class="hidden items-center gap-1 sm:flex"
                aria-label="Issue status totals"
            >
                for tally in tallies {
                    (tally)
                }
            </div>
            <button
                id="native-issue-filter-trigger"
                type="button"
                class=(BUTTON)
                aria-controls="native-issue-filter-dialog"
                :aria-expanded=$(filter_open.get())
                @click=$(|_event: Event| {
                    if !browser.is_disposed() {
                        filter_open.set(!filter_open.get());
                        sort_open.set(false);
                        display_open.set(false);
                    }
                })
            >
                "Filter"
                <span
                    :hidden=$(count.clone() == 0_usize)
                    class="rounded bg-[var(--accent-subtle)] px-1 text-micro text-[var(--accent)]"
                >
                    (count.clone())
                </span>
            </button>
            <div class="ml-auto flex flex-wrap items-center gap-1">
                if layout == "board" {
                    <div
                        class="flex flex-wrap items-center gap-0.5 rounded-md bg-[var(--bg-subtle)] p-0.5"
                        aria-label="Columns"
                    >
                        <span
                            class="px-2 text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                        >
                            "Columns"
                        </span>
                        for button in column_buttons {
                            (button)
                        }
                    </div>
                }
                if layout == "list" {
                    for button in tab_buttons {
                        (button)
                    }
                }
                <input
                    id="native-issue-search"
                    :hidden=$(!search_open.get())
                    aria-label="Search issues"
                    placeholder="Search issues..."
                    type="search"
                    class="w-40 rounded-md border border-[var(--border)] bg-[var(--bg)] px-2 py-1 text-body-sm text-[var(--text)] sm:w-52"
                    :value=$(browser.json_string(
                        wire.get(),
                        "searchQuery".to_owned(),
                        "".to_owned(),
                    ))
                    (search)
                />
                <button
                    id="native-issue-search-trigger"
                    type="button"
                    class=(BUTTON)
                    aria-label="Search issues"
                    :hidden=$(search_open.get())
                    (search_open_attrs)
                >
                    (icons::ui_icon(cx, icons::UiIcon::Search, 14))
                </button>
                <div class="relative">
                    <button
                        type="button"
                        class=(BUTTON)
                        aria-label="Sort issues"
                        :aria-expanded=$(sort_open.get())
                        @click=$(|_event: Event| {
                            if !browser.is_disposed() {
                                sort_open.set(!sort_open.get());
                                display_open.set(false);
                            }
                        })
                    >
                        "Sort"
                    </button>
                    <div
                        class="absolute right-0 top-full z-30 mt-1.5 w-52 rounded-lg border border-[var(--border)] bg-[var(--surface)] p-2 shadow-lg"
                        :hidden=$(!sort_open.get())
                    >
                        <p
                            class="px-2 text-micro uppercase tracking-widest text-[var(--text-faint)]"
                        >
                            "Sort by"
                        </p>
                        for button in sorts {
                            (button)
                        }
                        <p
                            class="border-t border-[var(--border)] px-2 pt-2 text-micro text-[var(--text-faint)]"
                        >
                            "Click the active row to flip direction."
                        </p>
                    </div>
                </div>
                <div class="relative">
                    <button
                        type="button"
                        class=(BUTTON)
                        :aria-expanded=$(display_open.get())
                        @click=$(|_event: Event| {
                            if !browser.is_disposed() {
                                display_open.set(!display_open.get());
                                sort_open.set(false);
                            }
                        })
                    >
                        "Display"
                    </button>
                    <div
                        class="absolute right-0 top-full z-30 mt-1.5 w-52 rounded-lg border border-[var(--border)] bg-[var(--surface)] p-2 shadow-lg"
                        :hidden=$(!display_open.get())
                    >
                        <p
                            class="px-2 text-micro uppercase tracking-widest text-[var(--text-faint)]"
                        >
                            "Group by"
                        </p>
                        for button in groups {
                            (button)
                        }
                        <p
                            class="px-2 text-micro uppercase tracking-widest text-[var(--text-faint)]"
                        >
                            "Density"
                        </p>
                        for button in densities {
                            (button)
                        }
                    </div>
                </div>
                if layout == "board" {
                    <select
                        aria-label="Swimlanes"
                        class="rounded-md bg-[var(--surface)] px-2 py-1 text-caption text-[var(--text)]"
                        :value=$(lane.get())
                        @change=$(|event: Event| {
                            if !browser.is_disposed() {
                                lane.set(event.target.value.to_owned());
                                browser.store(lane_key.clone(), lane.get());
                            }
                        })
                    >
                        <option value="none">"No swimlanes"</option>
                        <option value="module">"Module"</option>
                        <option value="priority">"Priority"</option>
                    </select>
                }
            </div>
            <div
                class="fixed inset-0 z-[100] flex items-end justify-center bg-black/25 sm:items-start sm:px-4 sm:pt-[9dvh]"
                :hidden=$(!filter_open.get())
                @click=$(|_event: Event| {
                    if !browser.is_disposed() {
                        filter_open.set(false);
                    }
                })
            >
                <section
                    id="native-issue-filter-dialog"
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="native-issue-filter-heading"
                    class="flex max-h-[85dvh] w-full flex-col overflow-hidden rounded-t-2xl border border-[var(--border)] bg-[var(--surface)] shadow-xl sm:max-h-[82dvh] sm:max-w-[640px] sm:rounded-xl"
                    @click=$(|event: Event| event.stop_propagation())
                >
                    <header
                        class="flex shrink-0 items-center gap-3 border-b border-[var(--border)] px-5 py-3.5"
                    >
                        <h2
                            id="native-issue-filter-heading"
                            class="text-body-lg font-semibold"
                        >
                            "Filters"
                        </h2>
                        <button type="button" class=(BUTTON) (clear)>
                            "Clear all"
                        </button>
                        <button
                            type="button"
                            aria-label="Close filters"
                            class="ml-auto grid size-9 place-items-center rounded-md hover:bg-[var(--bg-subtle)]"
                            @click=$(|_event: Event| {
                                if !browser.is_disposed() {
                                    filter_open.set(false);
                                }
                            })
                        >
                            (icons::ui_icon(cx, icons::UiIcon::Close, 15))
                        </button>
                    </header>
                    <div
                        class="grid min-h-0 flex-1 grid-cols-1 gap-x-6 gap-y-5 overflow-y-auto overscroll-contain px-5 py-4 sm:grid-cols-2"
                    >
                        <section>
                            <h3
                                class="px-1 pb-1.5 text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                            >
                                "Status"
                            </h3>
                            for button in status {
                                (button)
                            }
                        </section>
                        <section>
                            <h3
                                class="px-1 pb-1.5 text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                            >
                                "Priority"
                            </h3>
                            for button in priority {
                                (button)
                            }
                        </section>
                        if has_labels {
                            <section>
                                <h3
                                    class="px-1 pb-1.5 text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                                >
                                    "Label"
                                </h3>
                                for button in labels {
                                    (button)
                                }
                            </section>
                        }
                        if has_modules {
                            <section>
                                <h3
                                    class="px-1 pb-1.5 text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                                >
                                    "Module"
                                </h3>
                                for button in modules {
                                    (button)
                                }
                            </section>
                        }
                    </div>
                    <footer
                        class="flex items-center justify-between border-t border-[var(--border)] px-5 py-2.5 text-micro text-[var(--text-faint)]"
                    >
                        <span :hidden=$(count.clone() > 0_usize)>
                            "No filters applied"
                        </span>
                        <span :hidden=$(count.clone() == 0_usize)>
                            (count.clone())
                            " active"
                        </span>
                        "esc close"
                    </footer>
                </section>
            </div>
        </div>
    }.boxed()
}
