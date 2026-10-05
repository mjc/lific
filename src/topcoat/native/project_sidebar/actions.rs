//! Browser envelopes describe drafts/display state, never authority.
//! Every phase resolves the current cookie and immutable preference owner.
use super::super::{context, session};
use super::model::{self, Catalog, Completion, Direction, EditTarget, State, Token};
use crate::{
    db::models::{CreateProjectGroup, UpdateProjectGroup},
    error::LificError,
    realtime::RealtimeHub,
};
use topcoat::{
    context::{Cx, app_context},
    runtime::procedure,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) enum Write {
    Refresh {
        token: Token,
    },
    OrderProjects {
        token: Token,
        ids: Vec<i64>,
    },
    OrderGroups {
        token: Token,
        ids: Vec<i64>,
    },
    SaveGroup {
        token: Token,
        target: EditTarget,
        name: String,
    },
    DeleteGroup {
        token: Token,
        id: i64,
    },
    Assign {
        token: Token,
        project: i64,
        group: Option<i64>,
    },
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Frozen {
    receipt: Option<String>,
    write: Write,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct Applied {
    catalog: Option<Catalog>,
    groups_ready: bool,
    error: Option<String>,
    warning: String,
}

pub(super) fn error(error: LificError) -> String {
    match error {
        LificError::BadRequest(message)
        | LificError::Conflict(message)
        | LificError::NotFound(message)
        | LificError::Forbidden(message)
        | LificError::TooManyRequests(message) => message,
        error => {
            tracing::warn!(error = %error, "native sidebar command failed");
            "Couldn't save project sidebar changes. Try again.".into()
        }
    }
}
pub(super) fn encode<T: serde::Serialize>(value: &T) -> Result<String, LificError> {
    serde_json::to_string(value)
        .map_err(|error| LificError::Internal(format!("sidebar state encoding failed: {error}")))
}
fn parse<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, LificError> {
    if value.len() > 256 * 1024 {
        return Err(LificError::BadRequest(
            "Sidebar state is too large. Reload this page.".into(),
        ));
    }
    serde_json::from_str(value)
        .map_err(|_| LificError::BadRequest("Sidebar state changed. Reload this page.".into()))
}
fn caller(cx: &Cx, account: i64) -> Result<context::Caller, LificError> {
    let caller = context::caller(cx)?;
    let user = crate::api::require_user(&caller.identity)?;
    if user.id != account {
        return Err(LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        ));
    }
    let conn = context::db(cx).read()?;
    crate::auth::fresh_caller(&conn, account)?;
    Ok(caller)
}
pub(super) fn decoded(
    cx: &Cx,
    account: i64,
    wire: &str,
) -> Result<(context::Caller, State), LificError> {
    let caller = caller(cx, account)?;
    let mut state: State = parse(wire)?;
    if !state.valid_envelope() {
        return Err(LificError::BadRequest(
            "Sidebar state changed. Reload this page.".into(),
        ));
    }
    if state.catalog.owner != account {
        return Err(LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        ));
    }
    // Display metadata always comes from current visibility; the submitted
    // order/disclosure/draft cannot introduce a hidden project's name or link.
    let reads = crate::services::project_sidebar::load(context::db(cx), &caller.identity)?;
    let mut catalog = Catalog::from(&reads);
    catalog.generation = state.catalog.generation;
    let old_projects = state.catalog.project_ids();
    let old_groups = state.catalog.group_ids();
    catalog.projects.sort_by_key(|project| {
        old_projects
            .iter()
            .position(|id| *id == project.id)
            .unwrap_or(usize::MAX)
    });
    catalog.groups.sort_by_key(|group| {
        old_groups
            .iter()
            .position(|id| *id == group.id)
            .unwrap_or(usize::MAX)
    });
    // Group storage failure retains the prior same-owner presentation only.
    // It grants no capability; every later mutation queries owned rows itself.
    if !reads.groups_ready {
        catalog.groups = state.catalog.groups.clone();
    }
    state.catalog = catalog.normalize();
    state.groups_ready = reads.groups_ready;
    Ok((caller, state))
}

/// Return the next pure model state and, when needed, a frozen write payload.
/// The browser publishes optimistic order before invoking `apply`.
// Topcoat expr! passes native editor/write inputs as individual primitives.
#[allow(clippy::too_many_arguments)]
#[procedure("/__native_sidebar/prepare")]
pub(super) async fn prepare(
    cx: &Cx,
    account: i64,
    wire: String,
    command: String,
    id: i64,
    value: String,
    draft: String,
    focus: String,
) -> topcoat::Result<(bool, String, String, String, String)> {
    let (_, mut state) = session::read(cx, decoded(cx, account, &wire))?;
    if state.edit.as_ref().is_some_and(|edit| edit.draft != draft) {
        state.draft(draft);
    }
    let mut return_focus = String::new();
    let decision = (|| -> Result<Option<Write>, LificError> {
        match command.as_str() {
            "toggle_project" => {
                if !state.catalog.projects.iter().any(|p| p.id == id) {
                    return Err(LificError::NotFound("Project unavailable".into()));
                }
                state.toggle_project(id);
                Ok(None)
            }
            "toggle_group" => {
                if !state.catalog.groups.iter().any(|g| g.id == id) {
                    return Err(LificError::NotFound("Group unavailable".into()));
                }
                state.toggle_group(id);
                Ok(None)
            }
            "new_group" => {
                let project = (id > 0).then_some(id);
                if project.is_some_and(|id| !state.catalog.projects.iter().any(|p| p.id == id)) {
                    return Err(LificError::NotFound("Project unavailable".into()));
                }
                state.begin_edit(EditTarget::New { project }, focus);
                Ok(None)
            }
            "rename_group" => {
                state.begin_edit(EditTarget::Existing(id), focus);
                Ok(None)
            }
            "cancel_edit" => {
                return_focus = state.cancel_edit().unwrap_or_default();
                Ok(None)
            }
            "save_group" => {
                state.draft(value);
                Ok(state.begin_save().and_then(|(target, name)| {
                    state.save_token().map(|token| Write::SaveGroup {
                        token,
                        target,
                        name,
                    })
                }))
            }
            "project_up" | "project_down" => {
                let direction = if command == "project_up" {
                    Direction::Up
                } else {
                    Direction::Down
                };
                let ids = model::move_project(&state.catalog, id, direction).ok_or_else(|| {
                    LificError::BadRequest("Project is already at the end of this group.".into())
                })?;
                Ok(state
                    .begin_order(Some(&ids), None)
                    .map(|token| Write::OrderProjects { token, ids }))
            }
            "group_up" | "group_down" => {
                let direction = if command == "group_up" {
                    Direction::Up
                } else {
                    Direction::Down
                };
                let ids = model::move_group(&state.catalog, id, direction).ok_or_else(|| {
                    LificError::BadRequest("Group is already at the end of the sidebar.".into())
                })?;
                Ok(state
                    .begin_order(None, Some(&ids))
                    .map(|token| Write::OrderGroups { token, ids }))
            }
            "drag_start" => {
                state.dragging = true;
                Ok(None)
            }
            "drag_cancel" => {
                state.dragging = false;
                Ok(None)
            }
            "drag_finish" => {
                let zone: Vec<i64> = parse(&value)?;
                let ids = model::drag_order(&state.catalog, id, &zone).ok_or_else(|| {
                    LificError::BadRequest("Project order changed. Try again.".into())
                })?;
                Ok(state
                    .begin_order(Some(&ids), None)
                    .map(|token| Write::OrderProjects { token, ids }))
            }
            "delete_group" => Ok(state
                .begin_order(None, None)
                .map(|token| Write::DeleteGroup { token, id })),
            "assign" => {
                let group = if value.is_empty() {
                    None
                } else {
                    Some(
                        value
                            .parse()
                            .map_err(|_| LificError::BadRequest("Invalid group".into()))?,
                    )
                };
                Ok(state.begin_order(None, None).map(|token| Write::Assign {
                    token,
                    project: id,
                    group,
                }))
            }
            "refresh" => Ok(state.begin_refresh().map(|token| Write::Refresh { token })),
            "reveal" => {
                state.reveal((!value.is_empty()).then_some(value.as_str()));
                Ok(None)
            }
            "disconnect" => {
                state.disconnect();
                Ok(None)
            }
            _ => Err(LificError::BadRequest("Unknown sidebar action".into())),
        }
    })();
    match decision {
        Ok(write) => Ok((
            true,
            encode(&state)?,
            state
                .edit
                .as_ref()
                .map(|edit| edit.draft.clone())
                .unwrap_or_default(),
            return_focus,
            match write {
                None => String::new(),
                Some(write) => {
                    let receipt = if matches!(write, Write::Refresh { .. }) {
                        None
                    } else {
                        Some(
                            app_context::<super::receipts::SidebarWriteStore>(cx)
                                .reserve(account, write.clone())?,
                        )
                    };
                    encode(&Frozen { receipt, write })?
                }
            },
        )),
        Err(error) => Ok((
            false,
            self::error(error),
            String::new(),
            String::new(),
            String::new(),
        )),
    }
}

/// Shared service commit. Neither a submitted group ID nor a model token is
/// permission: queries own groups and assignment rechecks Viewer in its Tx.
#[procedure("/__native_sidebar/apply")]
pub(super) async fn apply(cx: &Cx, account: i64, frozen: String) -> topcoat::Result<String> {
    let caller = session::read(cx, caller(cx, account))?;
    let frozen: Frozen = session::read(cx, parse(&frozen))?;
    let result = caller
        .scope(async {
            match frozen.receipt {
                Some(key) => app_context::<super::receipts::SidebarWriteStore>(cx).execute(
                    context::db(cx),
                    app_context::<RealtimeHub>(cx),
                    &caller.identity,
                    &key,
                ),
                None if matches!(frozen.write, Write::Refresh { .. }) => {
                    Ok(super::receipts::Commit {
                        error: None,
                        warning: String::new(),
                    })
                }
                None => Err(LificError::BadRequest(
                    "Sidebar write receipt is required.".into(),
                )),
            }
        })
        .await;
    let commit = session::read(cx, result)?;
    Ok(encode(&applied(cx, &caller, commit))?)
}

/// Resolve the same reserved command after a failed transport; never start a new write.
#[procedure("/__native_sidebar/recover")]
pub(super) async fn recover(cx: &Cx, account: i64, frozen: String) -> topcoat::Result<String> {
    let caller = session::read(cx, caller(cx, account))?;
    let frozen: Frozen = session::read(cx, parse(&frozen))?;
    let result = caller
        .scope(async {
            match frozen.receipt {
                Some(key) => app_context::<super::receipts::SidebarWriteStore>(cx).recover(
                    context::db(cx),
                    &caller.identity,
                    &key,
                ),
                None if matches!(frozen.write, Write::Refresh { .. }) => {
                    Ok(super::receipts::Commit {
                        error: Some("The refresh wasn't confirmed. Try refreshing again.".into()),
                        warning: String::new(),
                    })
                }
                None => Err(LificError::BadRequest(
                    "Sidebar write receipt is required.".into(),
                )),
            }
        })
        .await;
    let commit = session::read(cx, result)?;
    Ok(encode(&applied(cx, &caller, commit))?)
}

fn applied(cx: &Cx, caller: &context::Caller, commit: super::receipts::Commit) -> Applied {
    if commit.error.is_some() {
        return Applied {
            catalog: None,
            groups_ready: false,
            error: commit.error,
            warning: commit.warning,
        };
    }
    match crate::services::project_sidebar::load(context::db(cx), &caller.identity) {
        Ok(reads) => Applied {
            catalog: Some(Catalog::from(&reads)),
            groups_ready: reads.groups_ready,
            error: None,
            warning: commit.warning,
        },
        Err(error) => {
            tracing::warn!(error=%error,"sidebar changed but following read failed");
            Applied {
                catalog: None,
                groups_ready: false,
                error: None,
                warning: if commit.warning.is_empty() {
                    "Changes saved, but the sidebar couldn't refresh. Try refreshing.".into()
                } else {
                    commit.warning
                },
            }
        }
    }
}

/// Merge with the current browser model, not the stale pre-write envelope.
#[procedure("/__native_sidebar/finish")]
pub(super) async fn finish(
    cx: &Cx,
    account: i64,
    wire: String,
    frozen: String,
    _result: String,
) -> topcoat::Result<(String, String)> {
    let (caller, mut state) = session::read(cx, decoded(cx, account, &wire))?;
    let frozen: Frozen = session::read(cx, parse(&frozen))?;
    let (write, commit) = session::read(
        cx,
        match frozen.receipt {
            Some(key) => app_context::<super::receipts::SidebarWriteStore>(cx).confirmed(
                context::db(cx),
                &caller.identity,
                &key,
            ),
            None if matches!(frozen.write, Write::Refresh { .. }) => Ok((
                frozen.write,
                super::receipts::Commit {
                    error: None,
                    warning: String::new(),
                },
            )),
            None => Err(LificError::BadRequest(
                "Sidebar write receipt is required.".into(),
            )),
        },
    )?;
    let return_focus = merge(&mut state, write, applied(cx, &caller, commit))?;
    Ok((encode(&state)?, return_focus))
}

/// The actual finish procedure uses this transition after fresh owner/outcome reads.
fn merge(state: &mut State, write: Write, mut applied: Applied) -> Result<String, LificError> {
    let (token, kind, failure) = match &write {
        Write::Refresh { token } => (*token, Completion::Refresh, None),
        Write::SaveGroup { token, .. } => (*token, Completion::Save, None),
        Write::OrderProjects { token, .. } => (
            *token,
            Completion::Order,
            Some("Project order wasn't saved"),
        ),
        Write::OrderGroups { token, .. } => {
            (*token, Completion::Order, Some("Group order wasn't saved"))
        }
        Write::DeleteGroup { token, .. } | Write::Assign { token, .. } => {
            (*token, Completion::Order, None)
        }
    };
    if !state.admits_completion(token, kind) {
        return Ok(String::new());
    }
    if let Some(catalog) = &mut applied.catalog {
        catalog.generation =
            state.catalog.generation.checked_add(1).ok_or_else(|| {
                LificError::BadRequest("Sidebar changed. Reload this page.".into())
            })?;
        if !applied.groups_ready {
            catalog.groups = state.catalog.groups.clone();
        }
    }
    let mut return_focus = String::new();
    match write {
        Write::Refresh { token } => {
            if let Some(catalog) = applied.catalog {
                if state.complete_refresh(token, catalog) {
                    state.groups_ready = applied.groups_ready;
                }
            } else if let Some(error) = applied.error {
                state.error = error;
            }
        }
        Write::SaveGroup { token, .. } => {
            let failed = applied.error.is_some();
            let result = applied.error.map_or(Ok(()), Err);
            return_focus = state.finish_save_token(token, result).unwrap_or_default();
            if failed && let Some(edit) = &state.edit {
                return_focus = if edit.return_focus.ends_with("-phone") {
                    "native-sidebar-phone-group-name"
                } else {
                    "native-sidebar-desktop-group-name"
                }
                .to_owned();
            }
            if let Some(catalog) = applied.catalog {
                state.accept_partial(catalog, applied.groups_ready);
            }
        }
        Write::OrderProjects { token, .. }
        | Write::OrderGroups { token, .. }
        | Write::DeleteGroup { token, .. }
        | Write::Assign { token, .. } => {
            let result = if let Some(error) = applied.error {
                let error = if let Some(failure) = failure {
                    format!("{failure}: {error}")
                } else {
                    error
                };
                Err(error)
            } else {
                Ok(applied.catalog.unwrap_or_else(|| state.catalog.clone()))
            };
            if state.finish_order(token, result) {
                state.groups_ready = applied.groups_ready;
            }
        }
    }
    if !applied.warning.is_empty() {
        state.error = applied.warning;
    }
    Ok(return_focus)
}

/// Single domain commit owner for normal sends and bounded receipt replay.
pub(super) fn commit(
    db: &crate::db::DbPool,
    hub: &RealtimeHub,
    identity: &Option<crate::resolve_caller::ResolvedIdentity>,
    write: &Write,
) -> Result<String, LificError> {
    let mut warning = String::new();
    match write {
        Write::Refresh { .. } => {}
        Write::OrderProjects { ids, .. } => {
            crate::services::project_sidebar::reorder_projects(db, hub, identity, ids)?;
        }
        Write::OrderGroups { ids, .. } => {
            crate::services::project_sidebar::reorder_groups(db, hub, identity, ids)?;
        }
        Write::DeleteGroup { id, .. } => {
            crate::services::project_sidebar::delete_group(db, hub, identity, *id)?;
        }
        Write::Assign { project, group, .. } => {
            crate::services::project_form::assign_project(db, hub, identity, *project, *group)?;
        }
        Write::SaveGroup { target, name, .. } => match target {
            EditTarget::Existing(id) => {
                crate::services::project_sidebar::rename_group(
                    db,
                    hub,
                    identity,
                    *id,
                    UpdateProjectGroup {
                        name: Some(name.clone()),
                    },
                )?;
            }
            EditTarget::New { project } => {
                let group = crate::services::project_sidebar::create_group(
                    db,
                    hub,
                    identity,
                    CreateProjectGroup { name: name.clone() },
                )?;
                if let Some(project) = project
                    && let Err(error) = crate::services::project_form::assign_project(
                        db,
                        hub,
                        identity,
                        *project,
                        Some(group.id),
                    )
                {
                    warning = format!(
                        "Group created, but the project wasn't moved into it: {}",
                        self::error(error)
                    );
                }
            }
        },
    }
    Ok(warning)
}

#[cfg(test)]
#[path = "finish_merge_tests.rs"]
mod finish_merge_tests;
