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

use super::super::{context, session};
use super::{actions, controls, delete_menu};

pub(crate) struct DocumentMetadata {
    pub(crate) project_identifier: String,
    pub(crate) module_id: Option<i64>,
    pub(crate) module: String,
    pub(crate) modules: Vec<crate::db::models::Module>,
    pub(crate) labels: Vec<(String, Option<String>)>,
    pub(crate) waits: Vec<crate::db::models::IssueWait>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

/// The issue has already crossed the shared Viewer and relation boundary.
pub(crate) fn metadata(
    cx: &Cx,
    issue: &crate::db::models::Issue,
) -> Result<DocumentMetadata, LificError> {
    let modules = {
        let conn = context::db(cx).read()?;
        queries::list_modules(&conn, issue.project_id)?
    };
    let module = match issue.module_id {
        None => "None".to_owned(),
        Some(id) => modules
            .iter()
            .find(|module| module.id == id)
            .map_or_else(|| "Unknown".to_owned(), |module| module.name.clone()),
    };
    let project_identifier = {
        let conn = context::db(cx).read()?;
        queries::get_project(&conn, issue.project_id)?.identifier
    };
    let labels = {
        let conn = context::db(cx).read()?;
        let project_labels = queries::list_labels(&conn, issue.project_id)?;
        issue
            .labels
            .iter()
            .map(|name| {
                let color = project_labels
                    .iter()
                    .find(|label| label.name == *name)
                    .map(|label| label.color.clone());
                (name.clone(), color)
            })
            .collect()
    };
    Ok(DocumentMetadata {
        project_identifier,
        module_id: issue.module_id,
        module,
        modules,
        labels,
        waits: issue.waits.clone(),
        created_at: issue.created_at.clone(),
        updated_at: issue.updated_at.clone(),
    })
}

struct AuthorizedDocument {
    snapshot: actions::Snapshot,
    delete_request: delete_menu::Request,
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
    let delete_request = delete_menu::Request {
        account_id: user.id,
        issue_id: issue.id,
        identifier: issue.identifier.clone(),
        list_path: format!("/{project}/issues"),
        detail_path: format!("/{project}/issues/{}", issue.identifier),
    };
    Ok(AuthorizedDocument {
        snapshot: actions::snapshot(issue),
        delete_request,
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
        &document.delete_request,
    ))
}
