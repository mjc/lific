//! Owned sidebar reads and writes shared by native chrome and REST adapters.

use crate::{
    api::require_user,
    db::{
        DbPool,
        models::{AuthUser, CreateProjectGroup, Project, ProjectGroup, UpdateProjectGroup},
    },
    error::LificError,
    realtime::{RealtimeEvent, RealtimeHub},
    resolve_caller::ResolvedIdentity,
};

#[derive(Debug)]
pub(crate) struct Catalog {
    pub(crate) user: AuthUser,
    pub(crate) projects: Vec<Project>,
    pub(crate) groups: Vec<ProjectGroup>,
    pub(crate) groups_ready: bool,
    pub(crate) group_error: String,
}

/// Personal project order and owned groups come from the same SQLite snapshot.
pub(crate) fn load(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
) -> Result<Catalog, LificError> {
    let caller = require_user(identity)?;
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let user = crate::auth::fresh_caller(&tx, caller.id)?;
    let visible = super::projects::sidebar_visibility(&tx, user.id)?;
    let projects = crate::db::queries::list_projects_for_user(&tx, user.id)?;
    let projects = super::projects::normalize_sidebar_ranks(crate::authz::filter_visible(
        projects,
        &visible,
        |project| Some(project.id),
    ));
    let (groups, groups_ready, group_error) = match super::project_form::group_rows(&tx, user.id) {
        Ok(groups) => (groups, true, String::new()),
        Err(error @ LificError::Forbidden(_)) => return Err(error),
        Err(error) => {
            tracing::warn!(error = %error, owner = user.id, "native sidebar groups unavailable");
            (
                Vec::new(),
                false,
                "Couldn't refresh project groups. Try again.".into(),
            )
        }
    };
    tx.commit()?;
    Ok(Catalog {
        user: crate::auth::fresh_auth_user(&user),
        projects,
        groups,
        groups_ready,
        group_error,
    })
}

pub(crate) fn create_group(
    db: &DbPool,
    hub: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    input: CreateProjectGroup,
) -> Result<ProjectGroup, LificError> {
    let user = require_user(identity)?;
    let group = db.transaction(|tx| {
        crate::auth::fresh_caller(tx, user.id)?;
        crate::db::queries::project_groups::create_group(tx, user.id, &input)
    })?;
    hub.send_to_users(RealtimeEvent::ProjectGroupsChanged, vec![user.id]);
    Ok(group)
}

pub(crate) fn rename_group(
    db: &DbPool,
    hub: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
    input: UpdateProjectGroup,
) -> Result<ProjectGroup, LificError> {
    let user = require_user(identity)?;
    let group = db.transaction(|tx| {
        crate::auth::fresh_caller(tx, user.id)?;
        crate::db::queries::project_groups::update_group(tx, id, user.id, &input)?;
        // Rename responses must not disclose projects hidden by a fresh demotion.
        super::project_form::group_rows(tx, user.id)?
            .into_iter()
            .find(|group| group.id == id)
            .ok_or_else(|| LificError::NotFound("Group not found".into()))
    })?;
    hub.send_to_users(RealtimeEvent::ProjectGroupsChanged, vec![user.id]);
    Ok(group)
}

pub(crate) fn delete_group(
    db: &DbPool,
    hub: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
) -> Result<bool, LificError> {
    let user = require_user(identity)?;
    let deleted = db.transaction(|tx| {
        crate::auth::fresh_caller(tx, user.id)?;
        crate::db::queries::project_groups::delete_group(tx, id, user.id)
    })?;
    hub.send_to_users(RealtimeEvent::ProjectGroupsChanged, vec![user.id]);
    Ok(deleted)
}

pub(crate) fn reorder_groups(
    db: &DbPool,
    hub: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    ids: &[i64],
) -> Result<Vec<ProjectGroup>, LificError> {
    let user = require_user(identity)?;
    let groups = db.transaction(|tx| {
        crate::auth::fresh_caller(tx, user.id)?;
        crate::db::queries::project_groups::reorder_groups(tx, user.id, ids)?;
        super::project_form::group_rows(tx, user.id)
    })?;
    hub.send_to_users(RealtimeEvent::ProjectGroupsChanged, vec![user.id]);
    Ok(groups)
}

/// Submitted visible projects precede omitted visible projects in their current order.
pub(crate) fn reorder_projects(
    db: &DbPool,
    hub: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    ids: &[i64],
) -> Result<Vec<Project>, LificError> {
    let user = require_user(identity)?;
    let projects = db.transaction(|tx| {
        let visible = super::projects::sidebar_visibility(tx, user.id)?;
        crate::db::queries::reorder_projects(tx, user.id, ids, &visible)
    })?;
    hub.send_to_users(RealtimeEvent::ProjectsReordered, vec![user.id]);
    Ok(super::projects::normalize_sidebar_ranks(projects))
}

#[cfg(test)]
mod tests;
