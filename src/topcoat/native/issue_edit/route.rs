//! Initial private issue composition. Shared services own reads and writes.
//! Comments, attachments, metadata and collaboration compose here as their
//! native components become available; the field editor owns its draft scope.

use topcoat::{context::Cx, view::BoxView};

use crate::{
    authz,
    db::{models::Role, queries},
    error::LificError,
    services,
};

use super::super::super::shell::ParsedRoute;
use super::super::{context, session};
use super::{actions, controls};

pub(crate) struct DocumentMetadata {
    pub(crate) module_id: Option<i64>,
    pub(crate) module: String,
    pub(crate) labels: Vec<String>,
    pub(crate) waits: Vec<crate::db::models::IssueWait>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

/// The issue has already crossed the shared Viewer and relation boundary.
pub(crate) fn metadata(
    cx: &Cx,
    issue: &crate::db::models::Issue,
) -> Result<DocumentMetadata, LificError> {
    let module = match issue.module_id {
        None => "None".to_owned(),
        Some(id) => {
            let conn = context::db(cx).read()?;
            match queries::get_module_name(&conn, id) {
                Ok(name) => name,
                Err(LificError::NotFound(_)) => "Unknown".to_owned(),
                Err(error) => return Err(error),
            }
        }
    };
    Ok(DocumentMetadata {
        module_id: issue.module_id,
        module,
        labels: issue.labels.clone(),
        waits: issue.waits.clone(),
        created_at: issue.created_at.clone(),
        updated_at: issue.updated_at.clone(),
    })
}

struct AuthorizedDocument {
    caller: context::Caller,
    user: crate::db::models::AuthUser,
    snapshot: actions::Snapshot,
    can_edit: bool,
}

fn authorized_document(
    cx: &Cx,
    project: &str,
    identifier: &str,
) -> topcoat::Result<AuthorizedDocument> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    let db = context::db(cx);
    let issue = session::read(
        cx,
        services::issues::resolve_issue(db, &caller.identity, identifier),
    )?;
    let route_project = session::read(
        cx,
        (|| {
            let conn = db.read()?;
            queries::resolve_project_identifier(&conn, project)
        })(),
    )?;
    if route_project != issue.project_id {
        return Err(topcoat::router::error::not_found().into());
    }
    let can_edit =
        match authz::require_role(db, &caller.identity, issue.project_id, Role::Maintainer) {
            Ok(()) => true,
            Err(LificError::Forbidden(_)) => false,
            Err(error) => return session::read(cx, Err(error)),
        };
    Ok(AuthorizedDocument {
        caller,
        user,
        snapshot: actions::snapshot(issue),
        can_edit,
    })
}

/// Fresh authority for a replaceable issue region in the workspace shell.
pub(crate) fn content<'a>(
    cx: &'a Cx,
    project: &str,
    identifier: &str,
) -> topcoat::Result<BoxView<'a>> {
    let document = authorized_document(cx, project, identifier)?;
    Ok(controls::document_region(
        cx,
        &document.snapshot,
        document.can_edit,
        project,
    ))
}

pub(crate) fn screen<'a>(
    cx: &'a Cx,
    route: &ParsedRoute<'_>,
    project: &str,
    identifier: &str,
) -> topcoat::Result<BoxView<'a>> {
    let document = authorized_document(cx, project, identifier)?;
    let projects = session::read(
        cx,
        services::projects::list_visible_projects(context::db(cx), &document.caller.identity),
    )?;
    Ok(controls::document(
        cx,
        &document.snapshot,
        document.can_edit,
        &document.user,
        &projects,
        route,
    ))
}
