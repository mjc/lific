use super::*;
use crate::{
    db::{
        models::{Issue, Label, Module, UpdateIssue},
        queries,
    },
    error::LificError,
};
use topcoat::{context::app_context, runtime::procedure};

pub(super) struct Data {
    pub(super) issue: Issue,
    pub(super) modules: Vec<Module>,
    pub(super) labels: Vec<Label>,
    pub(super) editable: bool,
    pub(super) comments: usize,
    pub(super) comments_partial: bool,
}

pub(super) fn load(
    cx: &Cx,
    caller: &context::Caller,
    identifier: &str,
) -> Result<Data, LificError> {
    let conn = context::db(cx).read()?;
    let tx = conn.unchecked_transaction()?;
    let identity = crate::auth::refresh_identity(&tx, caller.identity.as_ref())?;
    let issue = crate::services::issues::resolve_issue_conn(&tx, &identity, identifier)?;
    let authority =
        crate::services::project_authority::load_conn(&tx, &identity, issue.project_id)?;
    let (modules, labels) =
        crate::services::issues::issue_create_catalog_conn(&tx, &identity, issue.project_id)?;
    let comments = queries::comments::list_comments_page(
        &tx,
        queries::comments::CommentParent::Issue(issue.id),
        None,
        None,
        Some(50),
        Some(0),
    )?;
    tx.commit()?;
    Ok(Data {
        issue,
        modules,
        labels,
        editable: authority.can_edit_content,
        comments: comments.items.len(),
        comments_partial: comments.has_more,
    })
}

pub(super) fn checked_caller(cx: &Cx, account: i64) -> Result<context::Caller, LificError> {
    let caller = context::caller(cx)?;
    let user = crate::api::require_user(&caller.identity)?;
    if user.id != account {
        return Err(LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        ));
    }
    Ok(caller)
}

pub(super) type SaveResult = (bool, String, i64, String, String, String, String, String);
fn confirmed(issue: Issue, error: String) -> SaveResult {
    (
        error.is_empty(),
        error,
        issue.seq,
        issue.title,
        issue.status.to_string(),
        issue.priority.to_string(),
        issue
            .module_id
            .map_or_else(String::new, |id| id.to_string()),
        issue.updated_at,
    )
}

#[procedure("/__native_issue_peek/save")]
pub(crate) async fn save(
    cx: &Cx,
    account: i64,
    identifier: String,
    field: String,
    value: String,
    observed_seq: i64,
) -> topcoat::Result<SaveResult> {
    let caller = session::read(cx, checked_caller(cx, account))?;
    let db = context::db(cx);
    let issue = session::read(
        cx,
        crate::services::issues::resolve_issue(db, &caller.identity, &identifier),
    )?;
    session::read(
        cx,
        crate::authz::require_role(
            db,
            &caller.identity,
            issue.project_id,
            crate::db::models::Role::Maintainer,
        ),
    )?;
    let mut patch = UpdateIssue {
        expected_seq: Some(observed_seq),
        ..Default::default()
    };
    match field.as_str() {
        "title" => {
            let title = value.trim();
            if title.is_empty() || title == issue.title {
                return Ok(confirmed(issue, String::new()));
            }
            patch.title = Some(title.to_owned());
        }
        "status" => patch.status = Some(value.parse().map_err(LificError::BadRequest)?),
        "priority" => patch.priority = Some(value.parse().map_err(LificError::BadRequest)?),
        "module" => {
            patch.module_id = Some(if value.is_empty() {
                None
            } else {
                Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| LificError::BadRequest("invalid module".into()))?,
                )
            })
        }
        _ => return Err(LificError::BadRequest("invalid preview field".into()).into()),
    }
    let outcome = caller
        .scope(async {
            crate::services::issues::commit_issue_update(
                db,
                app_context::<crate::realtime::RealtimeHub>(cx),
                &caller.identity,
                issue.id,
                patch,
            )
        })
        .await;
    match outcome {
        Ok(issue) => Ok(confirmed(issue, String::new())),
        Err(LificError::UpdateConflict { current, .. }) => {
            let current = serde_json::from_value(*current).map_err(|error| {
                LificError::Internal(format!("invalid conflict snapshot: {error}"))
            })?;
            Ok(confirmed(
                current,
                "This issue changed elsewhere. Review the current values and try again.".into(),
            ))
        }
        Err(error) => session::read(cx, Err(error)),
    }
}
