//! Shared project-form roster, personal groups, and post-create assignment.

use rusqlite::Connection;

use crate::{
    api::require_user,
    authz,
    db::{
        DbPool,
        models::{ProjectGroup, Role, User},
        queries,
    },
    error::LificError,
    realtime::{RealtimeEvent, RealtimeHub},
    resolve_caller::ResolvedIdentity,
};

/// The REST roster fields needed by the native lead selector.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct LeadOption {
    pub(crate) id: i64,
    pub(crate) username: String,
    pub(crate) display_name: String,
    pub(crate) is_admin: bool,
    /// Inactive humans remain available to the roster and pinned lead picker.
    pub(crate) is_active: bool,
    pub(crate) created_at: String,
}

impl From<User> for LeadOption {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            username: user.username,
            display_name: user.display_name,
            is_admin: user.is_admin,
            is_active: user.is_active,
            created_at: user.created_at,
        }
    }
}

fn lead_rows(conn: &Connection) -> Result<Vec<LeadOption>, LificError> {
    Ok(queries::users::list_users(conn)?
        .into_iter()
        .filter(|user| !user.is_bot)
        .map(LeadOption::from)
        .collect())
}

/// Preserve the authenticated roster policy, including inactive humans.
pub(crate) fn list_leads(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
) -> Result<Vec<LeadOption>, LificError> {
    require_user(identity)?;
    let conn = db.read()?;
    lead_rows(&conn)
}

pub(crate) fn group_rows(conn: &Connection, user_id: i64) -> Result<Vec<ProjectGroup>, LificError> {
    let visible = super::projects::sidebar_visibility(conn, user_id)?;
    let mut groups = queries::project_groups::list_groups(conn, user_id)?;
    if let Some(ids) = &visible {
        for group in &mut groups {
            group.project_ids.retain(|id| ids.contains(id));
        }
    }
    Ok(groups)
}

/// Resolve fresh caller authority and personal membership from one snapshot.
pub(crate) fn list_groups(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
) -> Result<Vec<ProjectGroup>, LificError> {
    let caller = require_user(identity)?;
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let groups = group_rows(&tx, caller.id)?;
    tx.commit()?;
    Ok(groups)
}

pub(crate) fn form_catalog(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
) -> Result<(Vec<LeadOption>, Vec<ProjectGroup>), LificError> {
    let caller = require_user(identity)?;
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let leads = lead_rows(&tx)?;
    let groups = group_rows(&tx, caller.id)?;
    tx.commit()?;
    Ok((leads, groups))
}

/// Group assignment remains a separate commit after project creation.
/// The native entry preserves the form's existing concrete group argument.
pub(crate) fn assign_created_project(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
    group_id: i64,
) -> Result<(), LificError> {
    assign_project(db, realtime, identity, project_id, Some(group_id))
}

/// Viewer access and group ownership are required for personal filing.
pub(crate) fn assign_project(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
    group_id: Option<i64>,
) -> Result<(), LificError> {
    // Preserve the endpoint's Viewer-before-identity preflight errors.
    authz::require_role(db, identity, project_id, Role::Viewer)?;
    let caller = require_user(identity)?;
    db.transaction(|tx| {
        // A socket's captured caller can outlive a demotion or deactivation.
        // Recheck authority alongside the mutation under the writer.
        let fresh = crate::auth::fresh_caller(tx, caller.id)?;
        let fresh_identity = identity.as_ref().map(|identity| ResolvedIdentity {
            user: crate::auth::fresh_auth_user(&fresh),
            transport: identity.transport,
        });
        authz::require_role_conn(tx, &fresh_identity, project_id, Role::Viewer)?;
        queries::project_groups::assign_project(tx, caller.id, project_id, group_id)
    })?;
    realtime.send_to_users(RealtimeEvent::ProjectGroupsChanged, vec![caller.id]);
    Ok(())
}

#[cfg(test)]
#[path = "project_form_tests.rs"]
mod tests;
