//! Membership policy shared by native and REST adapters. Credentials stay server-side.
use crate::{
    actor::Transport,
    authz,
    db::{
        DbPool,
        models::{MemberWithUser, ProjectMember, Role},
        queries::members,
    },
    error::LificError,
    realtime::{RealtimeEvent, RealtimeHub},
    resolve_caller::ResolvedIdentity,
};

pub(crate) fn list(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project: i64,
) -> Result<Vec<MemberWithUser>, LificError> {
    let caller = crate::api::require_user(identity)?;
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let user = crate::auth::fresh_caller(&tx, caller.id)?;
    let fresh = Some(crate::auth::fresh_identity(&user, Transport::Web));
    authz::require_role_conn(&tx, &fresh, project, Role::Viewer)?;
    let rows = members::list_members_with_users(&tx, project)?;
    tx.commit()?;
    Ok(rows)
}
fn recent(
    tx: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    token: Option<&str>,
) -> Result<Option<ResolvedIdentity>, LificError> {
    let user = crate::api::require_user(identity)?;
    let token =
        token.ok_or_else(|| LificError::Forbidden("recent authentication required".into()))?;
    let user = crate::auth::revalidate_recent_session(tx, token, user.id)?;
    Ok(Some(crate::auth::fresh_identity(&user, Transport::Web)))
}
fn current(
    tx: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
) -> Result<Option<ResolvedIdentity>, LificError> {
    let caller = crate::api::require_user(identity)?;
    let user = crate::auth::fresh_caller(tx, caller.id)?;
    Ok(Some(crate::auth::fresh_identity(&user, Transport::Web)))
}
fn parse_role(raw: &str) -> Result<Role, LificError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "viewer" => Ok(Role::Viewer),
        "maintainer" => Ok(Role::Maintainer),
        "lead" => Ok(Role::Lead),
        other => Err(LificError::BadRequest(format!("unknown role '{other}'"))),
    }
}
pub(crate) fn add(
    db: &DbPool,
    hub: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    token: Option<&str>,
    project: i64,
    user: i64,
    role: &str,
) -> Result<ProjectMember, LificError> {
    authz::require_role(db, identity, project, Role::Lead)?;
    let row = db.transaction(|tx| {
        let fresh = recent(tx, identity, token)?;
        authz::require_role_conn(tx, &fresh, project, Role::Lead)?;
        members::add_member(tx, project, user, role)
    })?;
    hub.send(RealtimeEvent::ProjectUpdated {
        project_id: project,
    });
    Ok(row)
}
pub(crate) fn change_role(
    db: &DbPool,
    hub: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    token: Option<&str>,
    project: i64,
    user: i64,
    role: &str,
) -> Result<ProjectMember, LificError> {
    authz::require_role(db, identity, project, Role::Lead)?;
    let requested = parse_role(role)?;
    let row = db.transaction(|tx| {
        let previous = members::get_member_role(tx, project, user)?;
        let fresh = if previous.is_none_or(|role| requested > role) {
            recent(tx, identity, token)?
        } else {
            current(tx, identity)?
        };
        authz::require_role_conn(tx, &fresh, project, Role::Lead)?;
        members::change_role(tx, project, user, role)
    })?;
    hub.send(RealtimeEvent::ProjectUpdated {
        project_id: project,
    });
    Ok(row)
}
pub(crate) fn remove(
    db: &DbPool,
    hub: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    project: i64,
    user: i64,
) -> Result<(), LificError> {
    authz::require_role(db, identity, project, Role::Lead)?;
    db.transaction(|tx| {
        let fresh = current(tx, identity)?;
        authz::require_role_conn(tx, &fresh, project, Role::Lead)?;
        members::remove_member_guarded(tx, project, user)
    })?;
    hub.send(RealtimeEvent::ProjectUpdated {
        project_id: project,
    });
    Ok(())
}
#[cfg(test)]
#[path = "project_members_tests.rs"]
mod tests;
