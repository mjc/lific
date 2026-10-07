use crate::db::models::{CreateLabel, Priority, Status};
use topcoat::{
    context::{Cx, app_context},
    runtime::procedure,
};

use super::super::{context, session};

#[procedure("/__native_issue_create/create")]
pub(super) async fn create(
    cx: &Cx,
    expected_account: i64,
    project_id: i64,
    title: String,
    description: String,
    status: String,
    priority: String,
    module_id: i64,
    labels: Vec<String>,
) -> topcoat::Result<(bool, String)> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != expected_account {
        return session::read(
            cx,
            Err(crate::error::LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let status = match status.parse::<Status>() {
        Ok(status) => status,
        Err(error) => return Ok((false, error)),
    };
    let priority = match priority.parse::<Priority>() {
        Ok(priority) => priority,
        Err(error) => return Ok((false, error)),
    };
    let input = match (super::model::Draft {
        project_id,
        title,
        description,
        status,
        priority,
        module_id: (module_id > 0).then_some(module_id),
        labels,
    })
    .input()
    {
        Ok(input) => input,
        Err(error) => return Ok((false, error.to_string())),
    };
    let result = caller
        .scope(async {
            crate::services::issues::commit_issue_create(
                context::db(cx),
                app_context::<crate::realtime::RealtimeHub>(cx),
                &caller.identity,
                input,
            )
        })
        .await;
    match result {
        Ok(issue) => Ok((true, issue.identifier)),
        Err(error) => Ok((false, error.to_string())),
    }
}

#[procedure("/__native_issue_create/create_label")]
pub(super) async fn create_label(
    cx: &Cx,
    expected_account: i64,
    project_id: i64,
    name: String,
    color: String,
) -> topcoat::Result<(bool, String, String)> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != expected_account {
        return session::read(
            cx,
            Err(crate::error::LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let input = CreateLabel {
        project_id,
        name,
        color,
    };
    let result = caller
        .scope(async {
            crate::services::issues::commit_issue_label_create(
                context::db(cx),
                app_context::<crate::realtime::RealtimeHub>(cx),
                &caller.identity,
                input,
            )
        })
        .await;
    match result {
        Ok(label) => Ok((true, label.name, label.color)),
        Err(error) => Ok((false, error.to_string(), String::new())),
    }
}
