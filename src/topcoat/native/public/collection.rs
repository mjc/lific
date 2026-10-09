//! Read-only issue and board views for published projects.
use super::super::issue_collection::{model, view};
use super::data::{self, Body};
use super::preferences;
use crate::services::issues::IssueCollection;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};

pub(super) fn content<'a>(cx: &'a Cx, collection: &IssueCollection, board: bool) -> BoxView<'a> {
    let layout = if board { "board" } else { "list" }.to_owned();
    let project = collection.project.identifier.clone();
    let owner = cx.keyed(("public-issue-list", project.clone(), layout.clone()));
    let preference_owner = cx.keyed(("public-issue-preferences", project.clone()));
    let query = signal(&preference_owner, String::new);
    let list_href = super::super::navigation::attrs(cx, &format!("/public/{project}/issues"));
    let board_href = super::super::navigation::attrs(cx, &format!("/public/{project}/board"));
    let title = collection.project.name.clone();
    let project_id = collection.project.id;
    let status = signal(&preference_owner, String::new);
    let priority = signal(&preference_owner, String::new);
    let label = signal(&preference_owner, String::new);
    let module = signal(&preference_owner, String::new);
    let sort_field = signal(&preference_owner, || "priority".to_owned());
    let sort_dir = signal(&preference_owner, || "asc".to_owned());
    let group_by = signal(&preference_owner, || "status".to_owned());
    let issue_sub_tab = signal(&preference_owner, || "all".to_owned());
    let state_wire = signal(&preference_owner, || preferences::ISSUE_DEFAULTS.to_owned());
    let display = signal(&preference_owner, || {
        preferences::DISPLAY_DEFAULTS.to_owned()
    });
    let hydrated = signal(&preference_owner, || false);
    let state_key = preferences::issue_storage_key(&project);
    let tab_key = preferences::issue_tab_storage_key(project_id);
    let query_handler = preferences::persist_input(
        query.clone(),
        state_wire.clone(),
        state_key.clone(),
        "searchQuery",
    );
    let status_handler = preferences::persist_input(
        status.clone(),
        state_wire.clone(),
        state_key.clone(),
        "filterStatus",
    );
    let priority_handler = preferences::persist_input(
        priority.clone(),
        state_wire.clone(),
        state_key.clone(),
        "filterPriority",
    );
    let label_handler = preferences::persist_input(
        label.clone(),
        state_wire.clone(),
        state_key.clone(),
        "filterLabel",
    );
    let module_handler = preferences::persist_input(
        module.clone(),
        state_wire.clone(),
        state_key.clone(),
        "filterModule",
    );
    let sort_field_handler = preferences::persist_input(
        sort_field.clone(),
        state_wire.clone(),
        state_key.clone(),
        "sortField",
    );
    let sort_dir_handler = preferences::persist_input(
        sort_dir.clone(),
        state_wire.clone(),
        state_key.clone(),
        "sortDir",
    );
    let group_handler = preferences::persist_input(
        group_by.clone(),
        state_wire.clone(),
        state_key.clone(),
        "groupBy",
    );
    let density_buttons = [("compact", "Compact"), ("comfortable", "Comfortable")]
        .into_iter()
        .map(|(value, label)| display_choice(cx, &display, &state_wire, &state_key, value, label))
        .collect::<Vec<_>>();
    let preferences_attrs = preferences::issue_mount(
        cx,
        &project,
        project_id,
        &layout,
        preferences::IssueSignals {
            wire: state_wire.clone(),
            display: display.clone(),
            hydrated,
            query: query.clone(),
            status: status.clone(),
            priority: priority.clone(),
            label: label.clone(),
            module: module.clone(),
            sort_field: sort_field.clone(),
            sort_dir: sort_dir.clone(),
            group_by: group_by.clone(),
            issue_sub_tab: issue_sub_tab.clone(),
        },
    );
    let clear_query = query.clone();
    let clear_status = status.clone();
    let clear_priority = priority.clone();
    let clear_label = label.clone();
    let clear_module = module.clone();
    let clear_sort_field = sort_field.clone();
    let clear_sort_dir = sort_dir.clone();
    let clear_group = group_by.clone();
    let clear_sub_tab = issue_sub_tab.clone();
    let clear_wire = state_wire.clone();
    let clear_key = state_key.clone();
    let clear_tab_key = tab_key.clone();
    let clear_defaults = preferences::ISSUE_DEFAULTS.to_owned();
    let browser = super::super::browser::bindings();
    let clear_handler = expr!(|_event: Event| {
        if !browser.is_disposed() {
            clear_query.set("".to_owned());
            clear_status.set("".to_owned());
            clear_priority.set("".to_owned());
            clear_label.set("".to_owned());
            clear_module.set("".to_owned());
            clear_sort_field.set("priority".to_owned());
            clear_sort_dir.set("asc".to_owned());
            clear_group.set("status".to_owned());
            clear_sub_tab.set("all".to_owned());
            browser.store(clear_tab_key.clone(), "all".to_owned());
            let density =
                browser.json_string(clear_wire.get(), "density".to_owned(), "compact".to_owned());
            let next =
                browser.json_set_string(clear_defaults.clone(), "density".to_owned(), density);
            clear_wire.set(next.clone());
            browser.store(clear_key.clone(), next);
        }
    });
    let labels = collection
        .labels
        .iter()
        .map(|item| item.name.clone())
        .collect::<Vec<_>>();
    let modules = collection
        .modules
        .iter()
        .map(|item| item.name.clone())
        .collect::<Vec<_>>();
    let query_for_rows = query.clone();
    let status_for_rows = status.clone();
    let priority_for_rows = priority.clone();
    let label_for_rows = label.clone();
    let module_for_rows = module.clone();
    let sort_field_for_rows = sort_field.clone();
    let sort_dir_for_rows = sort_dir.clone();
    let group_for_rows = group_by.clone();
    let sub_tab_for_rows = issue_sub_tab.clone();
    let display_for_rows = display.clone();
    let lane_by = lane_by_control(cx, display.clone(), &project);
    let body = view! {
        owner =>
        public_issue_rows(
            project: project.clone(),
            query: $(query_for_rows.get()),
            layout: layout.clone(),
            filters: $((
                status_for_rows.get(),
                priority_for_rows.get(),
                label_for_rows.get(),
                module_for_rows.get(),
                sort_field_for_rows.get(),
                sort_dir_for_rows.get(),
                group_for_rows.get(),
                sub_tab_for_rows.get(),
            )),
            display: display_for_rows.clone()
        )
    }
    .boxed();
    let open_count = collection
        .issues
        .iter()
        .filter(|issue| {
            !matches!(
                issue.status,
                crate::db::models::Status::Done | crate::db::models::Status::Cancelled
            )
        })
        .count();
    let closed_count = collection.issues.len() - open_count;
    let tabs = [
        ("all", "All", Some(collection.issues.len())),
        ("recent", "Recent", None),
        ("open", "Open", Some(open_count)),
        ("closed", "Closed", Some(closed_count)),
    ]
    .into_iter()
    .map(|(tab, label, count)| {
        let selected = issue_sub_tab.clone();
        let handler_state = issue_sub_tab.clone();
        let handler_key = tab_key.clone();
        let browser = super::super::browser::bindings();
        let handler = expr!(|_event: Event| {
            if !browser.is_disposed() {
                handler_state.set(tab.to_owned());
                browser.store(handler_key.clone(), tab.to_owned());
            }
        });
        view! {
            cx =>
            <button
                type="button"
                class="inline-flex items-center gap-1.5 rounded-md px-3 py-2 text-caption text-[var(--text-muted)] hover:bg-[var(--bg-subtle)]"
                data-native-public-issue-tab=(tab)
                :data-native-public-selected-tab=$(selected.get())
                :aria-pressed=$(selected.get() == tab)
                data-topcoat-on:click=(handler.into_evaluated_and_js().1)
            >
                (label)
                if let Some(count) = count {
                    <span class="text-micro text-[var(--text-faint)]">
                        (count.to_string())
                    </span>
                }
            </button>
        }
        .boxed()
    })
    .collect::<Vec<_>>();
    view! {
        cx =>
        <main
            class="mx-auto flex min-h-0 w-full max-w-7xl flex-col gap-4 px-4 py-6 md:px-8"
            (preferences_attrs)
        >
            <header class="flex flex-wrap items-center justify-between gap-3">
                <h1 class="m-0 text-heading font-semibold">(title)</h1>
                <nav class="flex items-center gap-2" aria-label="Issue layout">
                    <a (list_href) aria-label="List view">"Issues"</a>
                    <a (board_href) aria-label="Board view">"Board"</a>
                </nav>
            </header>
            if !board {
                <nav
                    class="flex items-center gap-1 border-b border-solid border-[var(--border)]"
                    aria-label="Issue subsets"
                    data-native-public-issue-tabs=""
                >
                    for tab in tabs {
                        (tab)
                    }
                </nav>
            }
            <div
                class="flex flex-wrap items-end gap-3 rounded-lg border border-solid border-[var(--border)] p-3"
            >
                <label class="flex flex-col gap-1 text-caption">
                    "Search"
                    <input
                        id="public-issue-search"
                        type="search"
                        :value=$(query.get())
                        placeholder="Search issues"
                        class="rounded-md border border-solid border-[var(--border)] bg-[var(--surface)] px-3 py-2"
                        data-topcoat-on:input=(query_handler.into_evaluated_and_js().1)
                    />
                </label>
                <label class="flex flex-col gap-1 text-caption">
                    "Status"
                    <select
                        data-native-public-filter="status"
                        :value=$(status.get())
                        class="rounded-md border border-solid border-[var(--border)] bg-[var(--surface)] px-2 py-2"
                        data-topcoat-on:change=(status_handler.into_evaluated_and_js().1)
                    >
                        <option value="">"All statuses"</option>
                        <option value="@unresolved">"Unresolved"</option>
                        for status in model::STATUSES {
                            <option value=(status.as_str())>(status.as_str())</option>
                        }
                    </select>
                </label>
                <label class="flex flex-col gap-1 text-caption">
                    "Priority"
                    <select
                        data-native-public-filter="priority"
                        :value=$(priority.get())
                        class="rounded-md border border-solid border-[var(--border)] bg-[var(--surface)] px-2 py-2"
                        data-topcoat-on:change=(priority_handler.into_evaluated_and_js().1)
                    >
                        <option value="">"All priorities"</option>
                        for priority in model::PRIORITIES {
                            <option value=(priority.as_str())>
                                (priority.as_str())
                            </option>
                        }
                    </select>
                </label>
                <label class="flex flex-col gap-1 text-caption">
                    "Label"
                    <select
                        data-native-public-filter="label"
                        :value=$(label.get())
                        class="rounded-md border border-solid border-[var(--border)] bg-[var(--surface)] px-2 py-2"
                        data-topcoat-on:change=(label_handler.into_evaluated_and_js().1)
                    >
                        <option value="">"All labels"</option>
                        for label in labels {
                            <option value=(label.clone())>(label)</option>
                        }
                    </select>
                </label>
                <label class="flex flex-col gap-1 text-caption">
                    "Module"
                    <select
                        data-native-public-filter="module"
                        :value=$(module.get())
                        class="rounded-md border border-solid border-[var(--border)] bg-[var(--surface)] px-2 py-2"
                        data-topcoat-on:change=(module_handler.into_evaluated_and_js().1)
                    >
                        <option value="">"All modules"</option>
                        for module in modules {
                            <option value=(module.clone())>(module)</option>
                        }
                    </select>
                </label>
                <label class="flex flex-col gap-1 text-caption">
                    "Sort"
                    <select
                        data-native-public-filter="sort"
                        :value=$(sort_field.get())
                        class="rounded-md border border-solid border-[var(--border)] bg-[var(--surface)] px-2 py-2"
                        data-topcoat-on:change=(sort_field_handler.into_evaluated_and_js(

                        ).1)
                    >
                        <option value="priority">"Priority"</option>
                        <option value="age">"Created"</option>
                        <option value="updated">"Updated"</option>
                        <option value="number">"Number"</option>
                    </select>
                </label>
                <label class="flex flex-col gap-1 text-caption">
                    "Direction"
                    <select
                        data-native-public-filter="direction"
                        :value=$(sort_dir.get())
                        class="rounded-md border border-solid border-[var(--border)] bg-[var(--surface)] px-2 py-2"
                        data-topcoat-on:change=(sort_dir_handler.into_evaluated_and_js().1)
                    >
                        <option value="asc">"Ascending"</option>
                        <option value="desc">"Descending"</option>
                    </select>
                </label>
                <label class="flex flex-col gap-1 text-caption">
                    "Group by"
                    <select
                        data-native-public-filter="group"
                        :value=$(group_by.get())
                        class="rounded-md border border-solid border-[var(--border)] bg-[var(--surface)] px-2 py-2"
                        data-topcoat-on:change=(group_handler.into_evaluated_and_js().1)
                    >
                        <option value="status">"Status"</option>
                        <option value="priority">"Priority"</option>
                        <option value="module">"Module"</option>
                        <option value="none">"None"</option>
                    </select>
                </label>
                <div
                    class="flex flex-col gap-1 text-caption"
                    role="group"
                    aria-label="Density"
                    data-native-public-density-controls=""
                >
                    <span>"Density"</span>
                    <div class="flex gap-1">
                        for choice in density_buttons {
                            (choice)
                        }
                    </div>
                </div>
                if board {
                    (lane_by)
                }
                <button
                    type="button"
                    class="rounded-md px-3 py-2 text-caption text-[var(--accent)] hover:bg-[var(--bg-subtle)]"
                    data-native-public-clear=""
                    data-topcoat-on:click=(clear_handler.into_evaluated_and_js().1)
                >
                    "Clear filters"
                </button>
            </div>
            (body)
        </main>
    }
    .boxed()
}

#[shard("/public/__native/issues")]
async fn public_issue_rows(
    cx: &Cx,
    project: String,
    query: String,
    layout: String,
    filters: (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    ),
    display: Signal<String>,
) -> topcoat::Result<impl View> {
    let route = match layout.as_str() {
        "list" => super::super::public_route::Route::Issues { project },
        "board" => super::super::public_route::Route::Board { project },
        _ => return Err(crate::error::LificError::NotFound("not found".into()).into()),
    };
    let snapshot = super::super::session::read(cx, data::load(cx, &route))?;
    let Body::Issues(collection) = snapshot.body else {
        return Err(crate::error::LificError::NotFound("not found".into()).into());
    };
    let mut state: model::ViewState = serde_json::from_str(&display.get()).unwrap_or_default();
    if state.density != "comfortable" {
        state.density = "compact".to_owned();
    }
    if state.lane_by != "module" && state.lane_by != "priority" {
        state.lane_by = "none".to_owned();
    }
    state.search_query = query;
    state.filter_status = filters.0;
    state.filter_priority = filters.1;
    state.filter_label = filters.2;
    state.filter_module = filters.3;
    state.sort_field = filters.4;
    state.sort_dir = filters.5;
    state.group_by = filters.6;
    state.issue_sub_tab = filters.7;
    let selection = model::select(&collection, &state, &layout);
    let project_identifier = snapshot.project.identifier;
    let display_for_disclosures = display.clone();
    let disclosures = |disclosure: view::Disclosure<'_>| {
        disclosure_attributes(
            cx,
            display_for_disclosures.clone(),
            &project_identifier,
            disclosure,
        )
    };
    let region = view::region_with_disclosures(
        cx,
        &collection,
        &selection,
        Attributes::default(),
        view::Audience::Published(&project_identifier),
        Some(&disclosures),
    );
    let column_controls = if layout == "board" {
        board_column_visibility(cx, &selection, display, &project_identifier)
    } else {
        view! { cx => <div></div> }.boxed()
    };
    Ok(view! {
        cx =>
        <div class="flex min-h-0 flex-1 flex-col">
            (column_controls)
            (region)
        </div>
    })
}

fn display_choice<'a>(
    cx: &'a Cx,
    display: &Signal<String>,
    wire: &Signal<String>,
    state_key: &str,
    choice: &'static str,
    label: &'static str,
) -> BoxView<'a> {
    let browser = super::super::browser::bindings();
    let display_state = display.clone();
    let persisted_state = wire.clone();
    let storage_key = state_key.to_owned();
    let value = choice.to_owned();
    let handler = expr!(|_event: Event| {
        if !browser.is_disposed() {
            let next_display =
                browser.json_set_string(display_state.get(), "density".to_owned(), value.clone());
            display_state.set(next_display);
            let next_state =
                browser.json_set_string(persisted_state.get(), "density".to_owned(), value.clone());
            persisted_state.set(next_state.clone());
            browser.store(storage_key.clone(), next_state);
        }
    });
    let aria_display = display.clone();
    view! {
        cx =>
        <button
            type="button"
            class="rounded-md px-2 py-1 text-caption text-[var(--text-muted)] hover:bg-[var(--bg-subtle)] aria-pressed:bg-[var(--accent-subtle)] aria-pressed:text-[var(--accent)]"
            data-native-public-density=(choice)
            :aria-pressed=$(browser.json_string(
                    aria_display.get(),
                    "density".to_owned(),
                    "compact".to_owned(),
                )
                == choice)
            data-topcoat-on:click=(handler.into_evaluated_and_js().1)
        >
            (label)
        </button>
    }
    .boxed()
}

fn lane_by_control<'a>(cx: &'a Cx, display: Signal<String>, project: &str) -> BoxView<'a> {
    let browser = super::super::browser::bindings();
    let handler_state = display.clone();
    let handler_key = preferences::lane_by_storage_key(project);
    let handler = expr!(|event: Event| {
        if !browser.is_disposed() {
            let value = event.target.value.to_owned();
            let value = if value == "none" {
                "none".to_owned()
            } else if value == "module" {
                "module".to_owned()
            } else if value == "priority" {
                "priority".to_owned()
            } else {
                "".to_owned()
            };
            if !value.is_empty() {
                let next = browser.json_set_string(
                    handler_state.get(),
                    "laneBy".to_owned(),
                    value.clone(),
                );
                handler_state.set(next);
                browser.store(handler_key.clone(), value);
            }
        }
    });
    view! {
        cx =>
        <label class="flex flex-col gap-1 text-caption">
            "Swimlanes"
            <select
                aria-label="Swimlanes"
                data-native-public-lane-by=""
                class="rounded-md border border-solid border-[var(--border)] bg-[var(--surface)] px-2 py-2"
                :value=$(browser.json_string(
                    display.get(),
                    "laneBy".to_owned(),
                    "none".to_owned(),
                ))
                data-topcoat-on:change=(handler.into_evaluated_and_js().1)
            >
                <option value="none">"No swimlanes"</option>
                <option value="module">"Module"</option>
                <option value="priority">"Priority"</option>
            </select>
        </label>
    }
    .boxed()
}

fn toggle_array_attrs(
    cx: &Cx,
    display: Signal<String>,
    field: &'static str,
    value: String,
    storage_key: String,
    marker: (&'static str, String),
) -> Attributes {
    let browser = super::super::browser::bindings();
    let handler_state = display;
    let handler_field = field.to_owned();
    let handler_value = value;
    let handler_key = storage_key;
    let handler = expr!(|_event: Event| {
        if !browser.is_disposed() {
            let current = browser.json_array_field(handler_state.get(), handler_field.clone());
            let included = !browser.json_array_contains(current.clone(), handler_value.clone());
            let next_array =
                browser.json_array_set_string(current, handler_value.clone(), included);
            let next_state = browser.json_set_array(
                handler_state.get(),
                handler_field.clone(),
                next_array.clone(),
            );
            handler_state.set(next_state);
            browser.store(handler_key.clone(), next_array);
        }
    });
    let mut attrs = Attributes::with_capacity(2);
    attrs.insert(cx, marker.0, marker.1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

fn disclosure_attributes(
    cx: &Cx,
    display: Signal<String>,
    project: &str,
    disclosure: view::Disclosure<'_>,
) -> Attributes {
    match disclosure {
        view::Disclosure::Group { kind, key } => toggle_array_attrs(
            cx,
            display,
            "collapsedGroups",
            format!("{kind}:{key}"),
            preferences::collapsed_groups_storage_key(project),
            ("data-native-public-toggle-group", format!("{kind}:{key}")),
        ),
        view::Disclosure::Lane { key } => toggle_array_attrs(
            cx,
            display,
            "collapsedLanes",
            key.to_owned(),
            preferences::collapsed_lanes_storage_key(project),
            ("data-native-public-toggle-lane", key.to_owned()),
        ),
        view::Disclosure::Column(status) => toggle_array_attrs(
            cx,
            display,
            "collapsedColumns",
            status.as_str().to_owned(),
            preferences::collapsed_columns_storage_key(project),
            (
                "data-native-public-toggle-column",
                status.as_str().to_owned(),
            ),
        ),
    }
}

fn board_column_visibility<'a>(
    cx: &'a Cx,
    selection: &model::Selection,
    display: Signal<String>,
    project: &str,
) -> BoxView<'a> {
    let browser = super::super::browser::bindings();
    let buttons = model::STATUSES
        .into_iter()
        .map(|status| {
            let browser = browser.clone();
            let value = status.as_str().to_owned();
            let count = selection
                .issues
                .iter()
                .filter(|issue| issue.status == status)
                .count()
                .to_string();
            let attrs = toggle_array_attrs(
                cx,
                display.clone(),
                "hiddenStatuses",
                value.clone(),
                preferences::hidden_statuses_storage_key(project),
                ("data-native-public-column-visibility", value.clone()),
            );
            let state = display.clone();
            view! {
                cx =>
                <button
                    type="button"
                    class="inline-flex items-center gap-1.5 rounded px-2 py-1 text-caption font-medium text-[var(--text-muted)] hover:bg-[var(--bg-subtle)] aria-pressed:bg-[var(--chrome)] aria-pressed:text-[var(--text)]"
                    :aria-pressed=$(!browser.json_array_contains(
                        browser.json_array_field(
                            state.get(),
                            "hiddenStatuses".to_owned(),
                        ),
                        value.clone(),
                    ))
                    (attrs)
                >
                    (super::super::icons::status_icon(cx, status, 12))
                    <span class="capitalize">(value)</span>
                    <span class="text-micro text-[var(--text-faint)]">(count)</span>
                </button>
            }
            .boxed()
        })
        .collect::<Vec<_>>();
    view! {
        cx =>
        <div
            class="flex shrink-0 flex-wrap items-center gap-2 px-4 py-2"
            data-native-public-column-controls=""
        >
            <span
                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
            >
                "Columns"
            </span>
            <div
                class="flex flex-wrap items-center gap-0.5 rounded-md border border-solid border-[var(--border)] bg-[var(--bg-subtle)] p-0.5"
            >
                for button in buttons {
                    (button)
                }
            </div>
        </div>
    }
    .boxed()
}
