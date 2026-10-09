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
    pub(crate) issue: crate::db::models::Issue,
    pub(crate) account_id: i64,
    pub(crate) can_edit: bool,
    pub(crate) project_identifier: String,
    pub(crate) module_id: Option<i64>,
    pub(crate) module: String,
    pub(crate) modules: Vec<crate::db::models::Module>,
    pub(crate) labels: Vec<(String, Option<String>)>,
    pub(crate) label_catalog: Vec<crate::db::models::Label>,
    pub(crate) waits: Vec<crate::db::models::IssueWait>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) assignment: queries::assignees::Assignment,
    pub(crate) assignee_people: Vec<queries::assignees::IssueAssignee>,
}

/// The issue has already crossed the shared Viewer and relation boundary.
pub(crate) fn metadata(
    cx: &Cx,
    identifier: &str,
    identity: &Option<crate::resolve_caller::ResolvedIdentity>,
) -> Result<DocumentMetadata, LificError> {
    let conn = context::db(cx).read()?;
    let tx = conn.unchecked_transaction()?;
    let identity = crate::auth::refresh_identity(&tx, identity.as_ref())?;
    let user = crate::api::require_user(&identity)?;
    let issue_id = queries::resolve_identifier(&tx, identifier)?;
    let issue = queries::get_issue(&tx, issue_id)?;
    crate::authz::require_role_conn(&tx, &identity, issue.project_id, Role::Viewer)?;
    let can_edit =
        match crate::authz::require_role_conn(&tx, &identity, issue.project_id, Role::Maintainer) {
            Ok(()) => true,
            Err(LificError::Forbidden(_)) => false,
            Err(error) => return Err(error),
        };
    let modules = queries::list_modules(&tx, issue.project_id)?;
    let assignment = queries::assignees::assignment(&tx, issue.id)?;
    let settings = queries::settings::get(&tx)?;
    let member_ids = queries::members::list_members(&tx, issue.project_id)?
        .into_iter()
        .map(|member| member.user_id)
        .collect::<std::collections::HashSet<_>>();
    let mut assignee_people = queries::users::list_users(&tx)?
        .into_iter()
        .filter(|person| {
            person.is_active
                && !person.is_bot
                && (!settings.authz_enforced || person.is_admin || member_ids.contains(&person.id))
        })
        .map(|person| queries::assignees::IssueAssignee {
            user_id: person.id,
            username: person.username.clone(),
            display_name: (person.display_name != person.username).then_some(person.display_name),
        })
        .collect::<Vec<_>>();
    assignee_people.sort_by(|left, right| {
        (left.user_id != user.id)
            .cmp(&(right.user_id != user.id))
            .then_with(|| {
                assignment_person_name(left)
                    .to_lowercase()
                    .cmp(&assignment_person_name(right).to_lowercase())
            })
    });
    let project_identifier = queries::get_project(&tx, issue.project_id)?.identifier;
    let label_catalog = queries::list_labels(&tx, issue.project_id)?;
    let labels = issue
        .labels
        .iter()
        .map(|name| {
            let color = label_catalog
                .iter()
                .find(|label| label.name == *name)
                .map(|label| label.color.clone());
            (name.clone(), color)
        })
        .collect();
    tx.commit()?;
    let module = match issue.module_id {
        None => "None".to_owned(),
        Some(id) => modules
            .iter()
            .find(|module| module.id == id)
            .map_or_else(|| "Unknown".to_owned(), |module| module.name.clone()),
    };
    let waits = issue.waits.clone();
    let created_at = issue.created_at.clone();
    let updated_at = issue.updated_at.clone();
    Ok(DocumentMetadata {
        account_id: user.id,
        can_edit,
        project_identifier,
        module_id: issue.module_id,
        module,
        modules,
        labels,
        label_catalog,
        waits,
        created_at,
        updated_at,
        assignment,
        assignee_people,
        issue,
    })
}

fn assignment_person_name(person: &queries::assignees::IssueAssignee) -> &str {
    person
        .display_name
        .as_deref()
        .filter(|name| !name.is_empty())
        .unwrap_or(&person.username)
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
