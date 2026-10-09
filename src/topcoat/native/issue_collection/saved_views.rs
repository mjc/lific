//! Native personal issue views, backed by the shared owner-scoped queries.

use super::{
    super::{browser, context, session},
    controls,
    data::Collection,
};
use crate::db::{
    models::{CreateSavedView, SavedView, UpdateSavedView},
    queries,
};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, procedure, record, signal},
    view::{BoxView, View, ViewExt, component, view},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Layout {
    #[default]
    List,
    Board,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ViewConfig {
    version: u8,
    layout: Layout,
    filter_status: String,
    filter_priority: String,
    filter_label: String,
    filter_module: String,
    filter_assignee: String,
    search_query: String,
    sort_field: String,
    sort_dir: String,
    group_by: String,
    density: String,
    lane_by: String,
    hidden_statuses: Vec<String>,
}

impl Default for ViewConfig {
    fn default() -> Self {
        Self::parse("{}").expect("empty saved view config is valid")
    }
}

impl ViewConfig {
    fn parse(raw: &str) -> Result<Self, ()> {
        let parsed: serde_json::Value = serde_json::from_str(raw).map_err(|_| ())?;
        let object = parsed.as_object().ok_or(())?;
        let string = |key: &str, fallback: &str| {
            object
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or(fallback)
                .to_owned()
        };
        let layout = if string("layout", "list") == "board" {
            Layout::Board
        } else {
            Layout::List
        };
        let sort_field = string("sortField", "priority");
        let group_by = string("groupBy", "status");
        let lane_by = string("laneBy", "none");
        Ok(Self {
            version: 1,
            layout,
            filter_status: string("filterStatus", ""),
            filter_priority: string("filterPriority", ""),
            filter_label: string("filterLabel", ""),
            filter_module: string("filterModule", ""),
            filter_assignee: normalize_assignee(&string("filterAssignee", "")),
            search_query: string("searchQuery", ""),
            sort_field: if ["priority", "age", "number", "updated"].contains(&sort_field.as_str()) {
                sort_field
            } else {
                "priority".into()
            },
            sort_dir: if string("sortDir", "asc") == "desc" {
                "desc".into()
            } else {
                "asc".into()
            },
            group_by: if ["status", "priority", "module", "none"].contains(&group_by.as_str()) {
                group_by
            } else {
                "status".into()
            },
            density: if string("density", "compact") == "comfortable" {
                "comfortable".into()
            } else {
                "compact".into()
            },
            lane_by: if ["module", "priority"].contains(&lane_by.as_str()) {
                lane_by
            } else {
                "none".into()
            },
            hidden_statuses: object
                .get("hiddenStatuses")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect(),
        })
    }

    fn serialize(&self) -> String {
        serde_json::to_string(self).expect("view config serializes")
    }

    fn differs_from(&self, other: &Self) -> bool {
        self.layout != other.layout
            || self.filter_status != other.filter_status
            || self.filter_priority != other.filter_priority
            || self.filter_label != other.filter_label
            || self.filter_module != other.filter_module
            || self.filter_assignee != other.filter_assignee
            || self.search_query != other.search_query
            || self.sort_field != other.sort_field
            || self.sort_dir != other.sort_dir
            || self.group_by != other.group_by
            || self.density != other.density
            || self.lane_by != other.lane_by
            || sorted(&self.hidden_statuses) != sorted(&other.hidden_statuses)
    }
}

fn normalize_assignee(value: &str) -> String {
    if matches!(value, "none" | "human" | "me")
        || value.strip_prefix('@').is_some_and(|name| !name.is_empty())
    {
        value.to_owned()
    } else {
        String::new()
    }
}

fn snapshot(wire: &str, lane: &str, hidden: &str, layout: &str) -> ViewConfig {
    let mut config = ViewConfig::parse(wire).unwrap_or_default();
    config.layout = if layout == "board" {
        Layout::Board
    } else {
        Layout::List
    };
    config.version = 1;
    config.lane_by = lane.to_owned();
    config.hidden_statuses = serde_json::from_str(hidden).unwrap_or_default();
    config
}

fn sorted(values: &[String]) -> Vec<&str> {
    let mut result = values.iter().map(String::as_str).collect::<Vec<_>>();
    result.sort_unstable();
    result
}

fn wire_for(config: &ViewConfig) -> String {
    let mut value = serde_json::to_value(config).expect("view config serializes");
    if let Some(object) = value.as_object_mut() {
        object.remove("version");
        object.remove("layout");
        object.remove("laneBy");
        object.remove("hiddenStatuses");
    }
    serde_json::to_string(&value).expect("view state serializes")
}

fn authorize_snapshot(
    conn: &rusqlite::Connection,
    identity: &Option<crate::resolve_caller::ResolvedIdentity>,
    account: i64,
    project_id: i64,
) -> Result<i64, crate::error::LificError> {
    let identity = crate::auth::refresh_identity(conn, identity.as_ref())?;
    let user = crate::api::require_user(&identity)?;
    if user.id != account {
        return Err(crate::error::LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        ));
    }
    crate::authz::require_role_conn(conn, &identity, project_id, crate::db::models::Role::Viewer)?;
    Ok(user.id)
}

fn default_restore_ready(hydrated: bool, already_checked: bool) -> bool {
    hydrated && !already_checked
}

fn active_view_drift(rows: &[ViewRow], id: i64, current: &ViewConfig) -> bool {
    rows.iter()
        .find(|row| row.id == id)
        .and_then(|row| ViewConfig::parse(&row.config).ok())
        .is_some_and(|saved| saved.differs_from(current))
}

#[record]
#[derive(Clone, Default)]
pub(crate) struct ViewRow {
    pub id: i64,
    pub name: String,
    pub config: String,
    pub is_default: bool,
}

#[record]
#[derive(Clone, Default)]
pub(crate) struct ConfigState {
    pub found: bool,
    pub view_id: i64,
    pub wire: String,
    pub lane: String,
    pub hidden: String,
    pub layout: String,
}

#[record]
#[derive(Clone, Default)]
pub(crate) struct ViewReply {
    pub ok: bool,
    pub error: String,
    pub view: ViewRow,
}

#[record]
#[derive(Clone, Default)]
pub(crate) struct MutationReply {
    pub ok: bool,
    pub error: String,
}

impl From<SavedView> for ViewRow {
    fn from(view: SavedView) -> Self {
        Self {
            id: view.id,
            name: view.name,
            config: view.config,
            is_default: view.is_default,
        }
    }
}

#[procedure("/__native_issue_views/list")]
async fn list_views(cx: &Cx, account: i64, project_id: i64) -> topcoat::Result<Vec<ViewRow>> {
    let caller = session::read(cx, context::caller(cx))?;
    let conn = context::db(cx).read()?;
    let tx = conn.unchecked_transaction()?;
    let _authorized_user = session::read(
        cx,
        authorize_snapshot(&tx, &caller.identity, account, project_id),
    )?;
    Ok(queries::views::list_views(&tx, project_id, account)?
        .into_iter()
        .map(Into::into)
        .collect())
}

#[procedure("/__native_issue_views/default")]
async fn default_config(cx: &Cx, account: i64, project_id: i64) -> topcoat::Result<ConfigState> {
    let caller = session::read(cx, context::caller(cx))?;
    let conn = context::db(cx).read()?;
    let tx = conn.unchecked_transaction()?;
    session::read(
        cx,
        authorize_snapshot(&tx, &caller.identity, account, project_id),
    )?;
    let rows = queries::views::list_views(&tx, project_id, account)?;
    let row = rows.iter().find(|row| row.is_default);
    if row.is_none() {
        return Ok(ConfigState::default());
    }
    let row = row.unwrap();
    let parsed = ViewConfig::parse(&row.config);
    if parsed.is_err() {
        return Ok(ConfigState::default());
    }
    let config = parsed.unwrap();
    Ok(ConfigState {
        found: true,
        view_id: row.id,
        wire: wire_for(&config),
        lane: config.lane_by,
        hidden: serde_json::to_string(&config.hidden_statuses).unwrap_or_else(|_| "[]".into()),
        layout: if config.layout == Layout::Board {
            "board"
        } else {
            "list"
        }
        .into(),
    })
}

/// Authorize and commit a personal view before notifying its owner.
fn commit_view<T>(
    cx: &Cx,
    account: i64,
    project_id: i64,
    write: impl FnOnce(&rusqlite::Connection) -> Result<T, crate::error::LificError>,
) -> topcoat::Result<Result<T, crate::error::LificError>> {
    let caller = session::read(cx, context::caller(cx))?;
    let result = context::db(cx).transaction(|conn| {
        let user_id = authorize_snapshot(conn, &caller.identity, account, project_id)?;
        Ok((write(conn)?, user_id))
    });
    match result {
        Ok((value, user_id)) => {
            topcoat::context::app_context::<crate::realtime::RealtimeHub>(cx).send_to_users(
                crate::realtime::RealtimeEvent::ProjectUpdated { project_id },
                vec![user_id],
            );
            Ok(Ok(value))
        }
        Err(error @ crate::error::LificError::Forbidden(_)) => session::read(cx, Err(error)),
        Err(error) => {
            if matches!(
                &error,
                crate::error::LificError::Database(_) | crate::error::LificError::Internal(_)
            ) {
                tracing::error!(%error, "saved view write failed");
            }
            Ok(Err(error))
        }
    }
}

#[procedure("/__native_issue_views/create")]
#[allow(clippy::too_many_arguments)] // Topcoat procedures serialize each reactive config slice as a typed argument.
async fn create_view(
    cx: &Cx,
    account: i64,
    project_id: i64,
    name: String,
    wire: String,
    lane: String,
    hidden: String,
    layout: String,
) -> topcoat::Result<ViewReply> {
    let config = snapshot(&wire, &lane, &hidden, &layout).serialize();
    let input = CreateSavedView {
        name,
        config,
        is_default: false,
    };
    let result = commit_view(cx, account, project_id, |conn| {
        queries::views::create_view(conn, project_id, account, &input)
    })?;
    Ok(match result {
        Ok(view) => ViewReply {
            ok: true,
            error: String::new(),
            view: view.into(),
        },
        Err(error) => ViewReply {
            ok: false,
            error: error.client_message().to_owned(),
            view: ViewRow::default(),
        },
    })
}

#[procedure("/__native_issue_views/update")]
#[allow(clippy::too_many_arguments)] // Keep independently persisted view fields as typed procedure arguments.
async fn update_view(
    cx: &Cx,
    account: i64,
    project_id: i64,
    view_id: i64,
    name: Option<String>,
    wire: Option<String>,
    lane: Option<String>,
    hidden: Option<String>,
    layout: Option<String>,
    is_default: Option<bool>,
) -> topcoat::Result<ViewReply> {
    let config = match (wire, lane, hidden, layout) {
        (Some(wire), Some(lane), Some(hidden), Some(layout)) => {
            Some(snapshot(&wire, &lane, &hidden, &layout).serialize())
        }
        _ => None,
    };
    let patch = UpdateSavedView {
        name,
        config,
        is_default,
    };
    let result = commit_view(cx, account, project_id, |conn| {
        queries::views::update_view(conn, view_id, project_id, account, &patch)
    })?;
    Ok(match result {
        Ok(view) => ViewReply {
            ok: true,
            error: String::new(),
            view: view.into(),
        },
        Err(error) => ViewReply {
            ok: false,
            error: error.client_message().to_owned(),
            view: ViewRow::default(),
        },
    })
}

#[procedure("/__native_issue_views/delete")]
async fn delete_view(
    cx: &Cx,
    account: i64,
    project_id: i64,
    view_id: i64,
) -> topcoat::Result<MutationReply> {
    let result = commit_view(cx, account, project_id, |conn| {
        queries::views::delete_view(conn, view_id, project_id, account)
    })?;
    Ok(match result {
        Ok(()) => MutationReply {
            ok: true,
            error: String::new(),
        },
        Err(error) => MutationReply {
            ok: false,
            error: error.client_message().to_owned(),
        },
    })
}

const BUTTON: &str = "rounded-md px-2 py-1.5 text-caption text-[var(--text-muted)] hover:bg-[var(--bg-subtle)] hover:text-[var(--text)]";

fn event_attrs(
    cx: &Cx,
    id: &str,
    event: &str,
    handler: topcoat::runtime::Js,
) -> topcoat::view::Attributes {
    controls::event(cx, id, event, handler)
}

pub(super) fn control<'a>(
    cx: &'a Cx,
    collection: &Collection,
    state: &controls::State,
    layout: &str,
) -> BoxView<'a> {
    let open = signal(cx, || false);
    let loading = signal(cx, || false);
    let busy = signal(cx, || false);
    let message = signal(cx, String::new);
    let views = signal(cx, Vec::<ViewRow>::new);
    let mode = signal(cx, || "menu".to_owned());
    let name = signal(cx, String::new);
    let selected = signal(cx, || 0_i64);
    let active = signal(cx, || 0_i64);
    let account = state.account;
    let project_id = collection.project.id;
    let project = collection.project.identifier.clone();
    let current_layout = layout.to_owned();
    let wire = state.wire.clone();
    let lane = state.lane.clone();
    let hidden = state.hidden.clone();
    let storage_key = state.storage_key.clone();
    let hydrated = state.hydrated.clone();
    let root_id = format!("native-saved-views-{project}");
    let active_key = format!("lific:views:active:{project}");
    let marker = format!("lific:views:session-checked:{project}");
    let lane_key = format!("lific:board:lanes:{project}");
    let hidden_key = format!("lific:board:hidden-statuses:{project}");
    let layout_key = format!("lific:list:layout:{project}");
    let board_path = format!("/{project}/board");
    let list_path = format!("/{project}/issues");
    let browser = browser::bindings();

    let toggle_failed_loading = loading.clone();
    let toggle_run_loading = loading.clone();
    let toggle = expr!(|_event: Event| {
        let _failed = || {
            if !browser.is_disposed() {
                toggle_failed_loading.set(false);
            }
        };
        let _run = async || {
            if !browser.is_disposed() {
                open.set(!open.get());
                if open.get() {
                    toggle_run_loading.set(true);
                    let loaded = list_views(account, project_id).await;
                    if !browser.is_disposed() {
                        views.set(loaded);
                        toggle_run_loading.set(false);
                    }
                }
            }
        };
        raw!(
            "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
            ()
        )
    });
    let toggle_attrs = event_attrs(
        cx,
        "saved-views-toggle",
        "click",
        toggle.into_evaluated_and_js().1,
    );

    let mount = expr!(|_event: Event| {
        let _failed = || {};
        let _run = async || {
            if !browser.is_disposed() {
                let saved_active = raw!(
                    r#"(()=>{try{return cx.hydrate({t:'i64',bits:64,v:BigInt(sessionStorage.getItem(${active_key}.toString())||'0').toString()})}catch{return cx.hydrate(0)}})()"#,
                    0_i64
                );
                active.set(saved_active);
                let _dismiss = |outside: bool| {
                    if outside {
                        open.set(false);
                        mode.set("menu".to_owned());
                    }
                };
                raw!(
                    "window.addEventListener('click', event => ${_dismiss}(cx.hydrate(!document.getElementById(${root_id}.toString())?.contains(event.target))), {signal:cx.abortSignal}); window.addEventListener('keydown', event => { if(event.key === 'Escape') ${_dismiss}(true); }, {signal:cx.abortSignal})",
                    (),
                );
                let loaded = list_views(account, project_id).await;
                if browser.is_disposed() {
                    return;
                }
                views.set(loaded);
            }
        };
        raw!(
            "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
            ()
        )
    });
    let mount_attrs = event_attrs(
        cx,
        "saved-views-mount",
        "mount",
        mount.into_evaluated_and_js().1,
    );

    let default_view = expr!(|_event: Event| {
        active.set(0_i64);
        open.set(false);
        mode.set("menu".to_owned());
        let _removed = raw!(
            r#"cx.hydrate((()=>{try{sessionStorage.removeItem(${active_key}.toString());return true}catch{return false}})())"#,
            false
        );
    });
    let default_attrs = event_attrs(
        cx,
        "saved-views-default",
        "click",
        default_view.into_evaluated_and_js().1,
    );

    let rows = {
        let views = views.clone();
        let project = project.clone();
        let current_layout = current_layout.clone();
        let wire = wire.clone();
        let lane = lane.clone();
        let hidden = hidden.clone();
        let storage_key = storage_key.clone();
        let active = active.clone();
        let open = open.clone();
        let busy = busy.clone();
        let selected = selected.clone();
        let name = name.clone();
        let message = message.clone();
        let mode = mode.clone();
        view! {
            cx =>
            saved_view_rows(
                rows: views,
                account: account,
                project_id: project_id,
                project: project.clone(),
                layout: current_layout.clone(),
                wire: wire.clone(),
                lane: lane.clone(),
                hidden: hidden.clone(),
                storage_key: storage_key.clone(),
                active: active.clone(),
                open: open.clone(),
                busy: busy.clone(),
                selected: selected.clone(),
                name: name.clone(),
                message: message.clone(),
                mode: mode.clone()
            )
        }
        .boxed()
    };

    let start_create = expr!(|_event: Event| {
        name.set("".to_owned());
        message.set("".to_owned());
        mode.set("create".to_owned());
        raw!(
            r#"queueMicrotask(()=>document.getElementById("native-saved-view-name")?.focus())"#,
            ()
        );
    });
    let start_create_attrs = event_attrs(
        cx,
        "saved-view-create",
        "click",
        start_create.into_evaluated_and_js().1,
    );
    let input = expr!(|event: Event| {
        name.set(event.target.value.to_owned());
    });
    let input_attrs = event_attrs(
        cx,
        "saved-view-name-input",
        "input",
        input.into_evaluated_and_js().1,
    );
    let keydown = expr!(|event: Event| {
        if event.key == "Escape" {
            event.prevent_default();
            mode.set("menu".to_owned());
            message.set("".to_owned());
        }
        if event.key == "Enter" {
            event.prevent_default();
            let _clicked = raw!(
                r#"document.getElementById("native-saved-view-submit")?.click()"#,
                false
            );
        }
    });
    let keydown_attrs = event_attrs(
        cx,
        "saved-view-name-keydown",
        "keydown",
        keydown.into_evaluated_and_js().1,
    );
    let failed_busy = busy.clone();
    let failed_message = message.clone();
    let run_busy = busy.clone();
    let run_message = message.clone();
    let submit = expr!(|_event: Event| {
        let _failed = || {
            if !browser.is_disposed() {
                failed_busy.set(false);
                failed_message.set("The saved view could not be updated. Try again.".to_owned());
            }
        };
        let _run = async || {
            if run_busy.get() {
                return;
            }
            let trimmed = name.get().trim().to_owned();
            if trimmed.is_empty() {
                run_message.set("Name is required.".to_owned());
                return;
            }
            run_busy.set(true);
            run_message.set("".to_owned());
            let creating = mode.get() == "create";
            let result = if creating {
                create_view(
                    account,
                    project_id,
                    trimmed,
                    wire.get(),
                    lane.get(),
                    hidden.get(),
                    current_layout.clone(),
                )
                .await
            } else {
                update_view(
                    account,
                    project_id,
                    selected.get(),
                    Some(trimmed),
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .await
            };
            if browser.is_disposed() {
                return;
            }
            run_busy.set(false);
            if result.ok {
                let saved = result.view;
                if creating {
                    active.set(saved.id);
                    let _id = saved.id;
                    let _stored = raw!(
                        r#"cx.hydrate((()=>{try{sessionStorage.setItem(${active_key}.toString(),${_id}.toString());return true}catch{return false}})())"#,
                        false
                    );
                }
                mode.set("menu".to_owned());
                let loaded = list_views(account, project_id).await;
                if browser.is_disposed() {
                    return;
                }
                views.set(loaded);
            } else {
                run_message.set(result.error);
            }
        };
        raw!(
            "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
            ()
        )
    });
    let submit_attrs = event_attrs(
        cx,
        "saved-view-submit",
        "click",
        submit.into_evaluated_and_js().1,
    );
    let escape = expr!(|event: Event| {
        if event.key == "Escape" {
            if open.get() {
                event.prevent_default();
                open.set(false);
                mode.set("menu".to_owned());
            }
        }
    });
    let escape_attrs = event_attrs(
        cx,
        "saved-views-escape",
        "keydown",
        escape.into_evaluated_and_js().1,
    );

    let label_view = {
        let rows = views.clone();
        let active = active.clone();
        view! { cx => active_saved_view_label(rows: rows, active: active) }.boxed()
    };
    let update_controls = {
        let rows = views.clone();
        let active = active.clone();
        let wire = wire.clone();
        let lane = lane.clone();
        let hidden = hidden.clone();
        let layout = current_layout.clone();
        view! {
            cx =>
            active_saved_view_update(
                rows: rows,
                account: account,
                project_id: project_id,
                active: active,
                wire: wire,
                lane: lane,
                hidden: hidden,
                layout: layout
            )
        }
        .boxed()
    };
    let default_restore_view = {
        let wire = wire.clone();
        let lane = lane.clone();
        let hidden = hidden.clone();
        let active = active.clone();
        let layout = current_layout.clone();
        view! {
            cx =>
            default_restore_gate(
                hydrated: hydrated,
                account: account,
                project_id: project_id,
                layout: layout,
                wire: wire,
                lane: lane,
                hidden: hidden,
                active: active,
                storage_key: storage_key,
                active_key: active_key,
                marker: marker,
                lane_key: lane_key,
                hidden_key: hidden_key,
                layout_key: layout_key,
                board_path: board_path,
                list_path: list_path
            )
        }
        .boxed()
    };
    let drift_indicator = {
        view! {
            cx =>
            active_saved_view_indicator(
                rows: views,
                active: active,
                wire: wire,
                lane: lane,
                hidden: hidden,
                layout: current_layout
            )
        }
        .boxed()
    };
    view! {
        cx =>
        <div
            id=(root_id)
            class="relative"
            data-native-saved-views=(project)
            (mount_attrs)
            (escape_attrs)
        >
            <button
                type="button"
                class=(BUTTON)
                aria-label="Saved views"
                (toggle_attrs)
            >
                (label_view)
                (drift_indicator)
            </button>
            <div
                class="absolute right-0 top-full z-40 mt-1.5 w-64 rounded-lg border border-[var(--border)] bg-[var(--surface)] p-2 shadow-lg"
                :hidden=$(!open.get())
            >
                <h2
                    class="px-2 py-1 text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                >
                    "Saved views"
                </h2>
                <button type="button" class=(BUTTON) (default_attrs)>
                    "Default view"
                </button>
                <div
                    :hidden=$(!loading.get())
                    class="px-2 py-2 text-caption text-[var(--text-faint)]"
                >
                    "Loading…"
                </div>
                (rows)
                <div class="my-1 border-t border-[var(--border)]"></div>
                (update_controls)
                <button type="button" class=(BUTTON) (start_create_attrs)>
                    "Save current as new view"
                </button>
                <div :hidden=$(mode.get() == "menu") class="px-2 py-2">
                    <input
                        id="native-saved-view-name"
                        class="w-full rounded-md border border-[var(--border)] bg-[var(--bg)] px-2 py-1.5 text-body-sm"
                        placeholder="View name"
                        :value=$(name.get())
                        (input_attrs)
                        (keydown_attrs)
                    />
                    <p class="py-1 text-caption text-[var(--error)]" role="alert">
                        $(message.get())
                    </p>
                    <button
                        id="native-saved-view-submit"
                        type="button"
                        class=(BUTTON)
                        :disabled=$(busy.get())
                        (submit_attrs)
                    >
                        $(if mode.get() == "create" { "Save" } else { "Rename" })
                    </button>
                </div>
            </div>
            (default_restore_view)
        </div>
    }.boxed()
}

#[component]
#[allow(clippy::too_many_arguments)]
async fn default_restore_gate(
    cx: &Cx,
    hydrated: Signal<bool>,
    account: i64,
    project_id: i64,
    layout: String,
    wire: Signal<String>,
    lane: Signal<String>,
    hidden: Signal<String>,
    active: Signal<i64>,
    storage_key: String,
    active_key: String,
    marker: String,
    lane_key: String,
    hidden_key: String,
    layout_key: String,
    board_path: String,
    list_path: String,
) -> topcoat::Result<impl View> {
    let browser = browser::bindings();
    let failure_marker = marker.clone();
    let run_marker = marker;
    let attrs_handler = expr!(|_event: Event| {
        let _failed = || {
            if !browser.is_disposed() {
                let _removed = raw!(
                    r#"cx.hydrate((()=>{try{sessionStorage.removeItem(${failure_marker}.toString());return true}catch{return false}})())"#,
                    false
                );
            }
        };
        let _run = async || {
            if !browser.is_disposed() {
                let checked = raw!(
                    r#"cx.hydrate((()=>{try{return sessionStorage.getItem(${run_marker}.toString())!==null}catch{return true}})())"#,
                    false
                );
                if hydrated.get() {
                    if !checked {
                        let marked = raw!(
                            r#"cx.hydrate((()=>{try{sessionStorage.setItem(${run_marker}.toString(),"1");return true}catch{return false}})())"#,
                            false
                        );
                        if marked {
                            let config = default_config(account, project_id).await;
                            if !browser.is_disposed() {
                                if config.found {
                                    wire.set(config.wire.clone());
                                    lane.set(config.lane.clone());
                                    hidden.set(config.hidden.clone());
                                    browser.store(storage_key.clone(), config.wire);
                                    browser.store(lane_key.clone(), config.lane);
                                    browser.store(hidden_key.clone(), config.hidden);
                                    browser.store(layout_key.clone(), config.layout.clone());
                                    active.set(config.view_id);
                                    let _id = config.view_id;
                                    let _stored = raw!(
                                        r#"cx.hydrate((()=>{try{sessionStorage.setItem(${active_key}.toString(),${_id}.toString());return true}catch{return false}})())"#,
                                        false
                                    );
                                    if config.layout != layout {
                                        if config.layout == "board" {
                                            let _navigation = browser.navigate(board_path.clone());
                                        } else {
                                            let _navigation = browser.navigate(list_path.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };
        raw!(
            "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
            ()
        )
    });
    let attrs = event_attrs(
        cx,
        "saved-views-default-restore",
        "mount",
        attrs_handler.into_evaluated_and_js().1,
    );
    Ok(view! {
        cx =>
        if hydrated.get() {
            <span hidden="" (attrs)></span>
        }
    })
}

#[component]
async fn active_saved_view_indicator(
    cx: &Cx,
    rows: Signal<Vec<ViewRow>>,
    active: Signal<i64>,
    wire: Signal<String>,
    lane: Signal<String>,
    hidden: Signal<String>,
    layout: String,
) -> topcoat::Result<impl View> {
    let current = snapshot(&wire.get(), &lane.get(), &hidden.get(), &layout);
    let visible = active_view_drift(&rows.get(), active.get(), &current);
    Ok(view! {
        cx =>
        if visible {
            <span
                class="ml-1 size-1.5 rounded-full bg-[var(--accent)]"
                title="Active view has unsaved changes"
            ></span>
        }
    })
}

#[component]
#[allow(clippy::too_many_arguments)]
async fn saved_view_rows(
    cx: &Cx,
    rows: Signal<Vec<ViewRow>>,
    account: i64,
    project_id: i64,
    project: String,
    layout: String,
    wire: Signal<String>,
    lane: Signal<String>,
    hidden: Signal<String>,
    storage_key: String,
    active: Signal<i64>,
    open: Signal<bool>,
    busy: Signal<bool>,
    selected: Signal<i64>,
    name: Signal<String>,
    message: Signal<String>,
    mode: Signal<String>,
) -> topcoat::Result<impl View> {
    let browser = browser::bindings();
    let active_key = format!("lific:views:active:{project}");
    let lane_key = format!("lific:board:lanes:{project}");
    let hidden_key = format!("lific:board:hidden-statuses:{project}");
    let layout_key = format!("lific:list:layout:{project}");
    let mut rendered = Vec::new();
    for item in rows.get() {
        let parsed = ViewConfig::parse(&item.config).ok();
        let next_wire = parsed.as_ref().map(wire_for).unwrap_or_default();
        let next_lane = parsed
            .as_ref()
            .map(|config| config.lane_by.clone())
            .unwrap_or_default();
        let next_hidden = parsed
            .as_ref()
            .and_then(|config| serde_json::to_string(&config.hidden_statuses).ok())
            .unwrap_or_else(|| "[]".to_owned());
        let target_layout = parsed.as_ref().map_or_else(
            || "list".to_owned(),
            |config| {
                if config.layout == Layout::Board {
                    "board"
                } else {
                    "list"
                }
                .to_owned()
            },
        );
        let change_layout = target_layout != layout;
        let board_path = format!("/{project}/board");
        let list_path = format!("/{project}/issues");
        let can_apply = parsed.is_some();
        let default_icon = if item.is_default { "★" } else { "☆" };
        let apply_wire = wire.clone();
        let apply_lane = lane.clone();
        let apply_hidden = hidden.clone();
        let apply_active = active.clone();
        let apply_open = open.clone();
        let apply = expr!(|_event: Event| {
            if can_apply {
                apply_wire.set(next_wire.clone());
                apply_lane.set(next_lane.clone());
                apply_hidden.set(next_hidden.clone());
                browser.store(storage_key.clone(), next_wire.clone());
                browser.store(lane_key.clone(), next_lane.clone());
                browser.store(hidden_key.clone(), next_hidden.clone());
                browser.store(layout_key.clone(), target_layout.clone());
                apply_active.set(item.id);
                let _id = item.id;
                let _stored = raw!(
                    r#"cx.hydrate((()=>{try{sessionStorage.setItem(${active_key}.toString(),${_id}.toString());return true}catch{return false}})())"#,
                    false
                );
                if change_layout {
                    if target_layout == "board" {
                        let _navigation = browser.navigate(board_path.clone());
                    } else {
                        let _navigation = browser.navigate(list_path.clone());
                    }
                }
            }
            apply_open.set(false);
        });
        let apply_attrs = event_attrs(
            cx,
            &format!("saved-view-apply-{}", item.id),
            "click",
            apply.into_evaluated_and_js().1,
        );
        let rename_selected = selected.clone();
        let rename_name = name.clone();
        let rename_message = message.clone();
        let rename_mode = mode.clone();
        let rename = expr!(|_event: Event| {
            rename_selected.set(item.id);
            rename_name.set(item.name.clone());
            rename_message.set("".to_owned());
            rename_mode.set("rename".to_owned());
            raw!(
                r#"queueMicrotask(()=>document.getElementById("native-saved-view-name")?.focus())"#,
                ()
            );
        });
        let rename_attrs = event_attrs(
            cx,
            &format!("saved-view-rename-{}", item.id),
            "click",
            rename.into_evaluated_and_js().1,
        );
        let default_busy = busy.clone();
        let default_rows = rows.clone();
        let default_message = message.clone();
        let default_browser = browser::bindings();
        let failed_busy = default_busy.clone();
        let failed_message = default_message.clone();
        let run_busy = default_busy.clone();
        let run_message = default_message.clone();
        let default = expr!(|_event: Event| {
            let _failed = || {
                if !default_browser.is_disposed() {
                    failed_busy.set(false);
                    failed_message.set("Couldn't update the default view. Try again.".to_owned());
                }
            };
            let _run = async || {
                run_busy.set(true);
                let response = update_view(
                    account,
                    project_id,
                    item.id,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(!item.is_default),
                )
                .await;
                if default_browser.is_disposed() {
                    return;
                }
                if response.ok {
                    let loaded = list_views(account, project_id).await;
                    if default_browser.is_disposed() {
                        return;
                    }
                    default_rows.set(loaded);
                } else {
                    run_message.set(response.error);
                }
                run_busy.set(false);
            };
            raw!(
                "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                ()
            );
        });
        let default_attrs = event_attrs(
            cx,
            &format!("saved-view-default-{}", item.id),
            "click",
            default.into_evaluated_and_js().1,
        );
        let remove_busy = busy.clone();
        let remove_rows = rows.clone();
        let remove_active = active.clone();
        let remove_message = message.clone();
        let remove_browser = browser::bindings();
        let failed_busy = remove_busy.clone();
        let failed_message = remove_message.clone();
        let run_busy = remove_busy.clone();
        let run_message = remove_message.clone();
        let remove = expr!(|_event: Event| {
            let _failed = || {
                if !remove_browser.is_disposed() {
                    failed_busy.set(false);
                    failed_message.set("Couldn't delete the saved view. Try again.".to_owned());
                }
            };
            let _run = async || {
                run_busy.set(true);
                let response = delete_view(account, project_id, item.id).await;
                if remove_browser.is_disposed() {
                    return;
                }
                if response.ok {
                    if remove_active.get() == item.id {
                        remove_active.set(0_i64);
                        let _removed = raw!(
                            r#"cx.hydrate((()=>{try{sessionStorage.removeItem(${active_key}.toString());return true}catch{return false}})())"#,
                            false
                        );
                    }
                    let loaded = list_views(account, project_id).await;
                    if remove_browser.is_disposed() {
                        return;
                    }
                    remove_rows.set(loaded);
                } else {
                    run_message.set(response.error);
                }
                run_busy.set(false);
            };
            raw!(
                "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                ()
            )
        });
        let remove_attrs = event_attrs(
            cx,
            &format!("saved-view-delete-{}", item.id),
            "click",
            remove.into_evaluated_and_js().1,
        );
        let row_busy = busy.clone();
        rendered.push(
            view! {
                cx =>
                <div
                    class="flex items-center gap-1 rounded-md hover:bg-[var(--bg-subtle)]"
                >
                    <button
                        type="button"
                        class="min-w-0 flex-1 truncate px-2 py-1.5 text-left text-body-sm"
                        (apply_attrs)
                    >
                        (item.name.clone())
                    </button>
                    <button
                        type="button"
                        class=(BUTTON)
                        title="Rename"
                        :disabled=$(row_busy.get())
                        (rename_attrs)
                    >
                        "✎"
                    </button>
                    <button
                        type="button"
                        class=(BUTTON)
                        title="Set or unset default"
                        :disabled=$(row_busy.get())
                        (default_attrs)
                    >
                        (default_icon)
                    </button>
                    <button
                        type="button"
                        class=(BUTTON)
                        title="Delete"
                        :disabled=$(row_busy.get())
                        (remove_attrs)
                    >
                        "×"
                    </button>
                </div>
            }
            .boxed(),
        );
    }
    Ok(view! {
        cx =>
        for row in rendered {
            (row)
        }
    })
}

#[component]
async fn active_saved_view_update(
    cx: &Cx,
    rows: Signal<Vec<ViewRow>>,
    account: i64,
    project_id: i64,
    active: Signal<i64>,
    wire: Signal<String>,
    lane: Signal<String>,
    hidden: Signal<String>,
    layout: String,
) -> topcoat::Result<impl View> {
    let id = active.get();
    let saved = rows.get().into_iter().find(|row| row.id == id);
    let current = snapshot(&wire.get(), &lane.get(), &hidden.get(), &layout);
    let drift = saved
        .as_ref()
        .and_then(|row| ViewConfig::parse(&row.config).ok())
        .is_some_and(|config| config.differs_from(&current));
    let update_rows = rows;
    let browser = browser::bindings();
    let handler = expr!(|_event: Event| {
        let _failed = || ();
        let _run = async || {
            if drift {
                let response = update_view(
                    account,
                    project_id,
                    id,
                    None,
                    Some(wire.get()),
                    Some(lane.get()),
                    Some(hidden.get()),
                    Some(layout.clone()),
                    None,
                )
                .await;
                if browser.is_disposed() {
                    return;
                }
                if response.ok {
                    let loaded = list_views(account, project_id).await;
                    if browser.is_disposed() {
                        return;
                    }
                    update_rows.set(loaded);
                }
            }
        };
        raw!(
            "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
            ()
        );
    });
    let attrs = event_attrs(
        cx,
        "saved-view-update-active",
        "click",
        handler.into_evaluated_and_js().1,
    );
    Ok(view! {
        cx =>
        if drift {
            <button type="button" class=(BUTTON) (attrs)>"Update active view"</button>
        }
    })
}

#[component]
async fn active_saved_view_label(
    cx: &Cx,
    rows: Signal<Vec<ViewRow>>,
    active: Signal<i64>,
) -> topcoat::Result<impl View> {
    let id = active.get();
    let label = rows
        .get()
        .into_iter()
        .find(|row| row.id == id)
        .map_or_else(|| "Views".to_owned(), |row| row.name);
    Ok(view! { cx => (label) })
}

#[cfg(test)]
mod tests {
    use super::{
        Layout, ViewConfig, ViewRow, active_view_drift, authorize_snapshot, default_restore_ready,
    };
    use crate::actor::Transport;

    #[test]
    fn saved_view_authority_checks_account_and_current_project_role_in_snapshot() {
        let (db, _, _, _, viewer, outsider, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let viewer_identity = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
        let outsider_identity = Some(crate::auth::fresh_identity(&outsider, Transport::Web));
        {
            let conn = db.read().unwrap();
            let tx = conn.unchecked_transaction().unwrap();
            assert!(authorize_snapshot(&tx, &viewer_identity, viewer.id, project_id).is_ok());
            assert!(authorize_snapshot(&tx, &viewer_identity, outsider.id, project_id).is_err());
            assert!(authorize_snapshot(&tx, &outsider_identity, outsider.id, project_id).is_err());
        }
        db.transaction(|conn| {
            conn.execute(
                "DELETE FROM project_members WHERE project_id=?1 AND user_id=?2",
                rusqlite::params![project_id, viewer.id],
            )?;
            Ok(())
        })
        .unwrap();
        let conn = db.read().unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        assert!(authorize_snapshot(&tx, &viewer_identity, viewer.id, project_id).is_err());
    }

    #[test]
    fn saved_view_config_round_trips_list_and_board_preferences() {
        let raw = r#"{"version":1,"layout":"board","filterStatus":"open","filterPriority":"high","filterLabel":"bug","filterModule":"core","filterAssignee":"@alice","searchQuery":"timeout","sortField":"age","sortDir":"desc","groupBy":"priority","density":"comfortable","laneBy":"module","hiddenStatuses":["done","closed"]}"#;
        let config = ViewConfig::parse(raw).unwrap();
        assert_eq!(config.layout, Layout::Board);
        assert_eq!(config.filter_status, "open");
        assert_eq!(config.filter_assignee, "@alice");
        assert_eq!(config.lane_by, "module");
        assert_eq!(ViewConfig::parse(&config.serialize()).unwrap(), config);
    }

    #[test]
    fn default_view_restore_waits_for_hydration_without_consuming_the_session_check() {
        assert!(!default_restore_ready(false, false));
        assert!(default_restore_ready(true, false));
        assert!(!default_restore_ready(true, true));
    }

    #[test]
    fn active_view_indicator_tracks_config_drift_instead_of_active_identity() {
        let saved = ViewConfig::default();
        let row = ViewRow {
            id: 42,
            name: "Inbox".into(),
            config: saved.serialize(),
            is_default: false,
        };
        assert!(!active_view_drift(std::slice::from_ref(&row), 42, &saved));
        assert!(active_view_drift(
            std::slice::from_ref(&row),
            42,
            &ViewConfig {
                layout: Layout::Board,
                ..saved.clone()
            }
        ));
        assert!(!active_view_drift(
            &[row],
            7,
            &ViewConfig {
                layout: Layout::Board,
                ..saved
            }
        ));
    }

    #[test]
    fn saved_view_config_tolerates_old_and_malformed_shapes_with_issue_defaults() {
        let config =
            ViewConfig::parse(r#"{"filterStatus":"open","hiddenStatuses":["done",4,null]}"#)
                .unwrap();
        assert_eq!(config.layout, Layout::List);
        assert_eq!(config.filter_status, "open");
        assert_eq!(config.hidden_statuses, vec!["done"]);
        let config =
            ViewConfig::parse(r#"{"layout":9,"sortField":[],"filterAssignee":"@"}"#).unwrap();
        assert_eq!(config.sort_field, "priority");
        assert_eq!(config.filter_assignee, "");
    }

    #[test]
    fn saved_view_config_recovers_invalid_fields_independently_and_normalizes_assignees() {
        let config = ViewConfig::parse(r#"{"layout":"foreign","filterAssignee":"alice","sortDir":"sideways","groupBy":false,"laneBy":"status","density":"dense"}"#).unwrap();
        assert_eq!(config.layout, Layout::List);
        assert_eq!(config.filter_assignee, "");
        assert_eq!(config.sort_dir, "asc");
        assert_eq!(config.group_by, "status");
        assert_eq!(config.lane_by, "none");
        assert_eq!(config.density, "compact");
    }

    #[test]
    fn saved_view_drift_includes_layout_and_hidden_statuses_but_ignores_set_order() {
        let mut a = ViewConfig::default();
        let mut b = a.clone();
        a.hidden_statuses = vec!["done".into(), "closed".into()];
        b.hidden_statuses = vec!["closed".into(), "done".into()];
        assert!(!a.differs_from(&b));
        b.layout = Layout::Board;
        assert!(a.differs_from(&b));
    }
}
