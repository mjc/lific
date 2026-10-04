//! Authorized project catalog shared by REST and native views.

use std::collections::HashSet;

use crate::api::require_user;
use crate::{
    authz::{self, filter_visible},
    db::{DbPool, models::Project},
    error::LificError,
    resolve_caller::ResolvedIdentity,
};

pub(crate) fn sidebar_visibility(
    conn: &rusqlite::Connection,
    user_id: i64,
) -> Result<Option<HashSet<i64>>, LificError> {
    let fresh = crate::auth::fresh_caller(conn, user_id)?;
    let effective = authz::effective_user(conn, &Some(crate::auth::fresh_auth_user(&fresh)));
    if matches!(&effective, Some(user) if user.is_admin) || !authz::authz_enforced_conn(conn)? {
        return Ok(None);
    }
    let Some(user) = effective else {
        return Ok(Some(HashSet::new()));
    };
    Ok(Some(
        crate::db::queries::members::list_project_ids_for_user(conn, user.id)?
            .into_iter()
            .collect(),
    ))
}

pub(crate) fn normalize_sidebar_ranks(mut projects: Vec<Project>) -> Vec<Project> {
    for (position, project) in projects.iter_mut().enumerate() {
        project.sort_order = position as i64;
    }
    projects
}

pub(crate) fn list_visible_projects(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
) -> Result<Vec<Project>, LificError> {
    // A valid unbound key can precede the first user. There is no preference
    // owner in that case; preserve the existing visibility-filtered listing.
    if identity.is_none() {
        let visible = authz::visible_project_ids(db, identity)?;
        let conn = db.read()?;
        let projects = crate::db::queries::list_projects(&conn)?;
        return Ok(filter_visible(projects, &visible, |p| Some(p.id)));
    }
    let user = require_user(identity)?;
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let visible = sidebar_visibility(&tx, user.id)?;
    let projects = crate::db::queries::list_projects_for_user(&tx, user.id)?;
    let projects = normalize_sidebar_ranks(filter_visible(projects, &visible, |p| Some(p.id)));
    tx.commit()?;
    Ok(projects)
}
