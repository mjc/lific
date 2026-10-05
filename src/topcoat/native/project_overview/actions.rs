//! Native commands re-resolve cookie authority; display signals grant nothing.

use super::super::{context, session};
use super::model::{self, Field};
use crate::{error::LificError, realtime::RealtimeHub};
use topcoat::{
    context::{Cx, app_context},
    runtime::procedure,
};

pub(super) type FieldOutcome = (Result<String, String>, String, String, String, String);

pub(super) fn error_message(error: LificError) -> String {
    match error {
        LificError::BadRequest(message)
        | LificError::Forbidden(message)
        | LificError::Conflict(message)
        | LificError::NotFound(message)
        | LificError::TooManyRequests(message) => message,
        error => {
            tracing::error!(error = %error, "native overview command failed");
            "Couldn't save changes. Try again.".into()
        }
    }
}

#[procedure("/__native_overview/save_field")]
pub(super) async fn save_field(
    cx: &Cx,
    account_id: i64,
    project_id: i64,
    field: String,
    value: String,
) -> topcoat::Result<FieldOutcome> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account_id {
        return Ok((
            Err("Your account changed. Reload this page.".into()),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ));
    }
    let db = context::db(cx);
    // Gate no-ops too; the DOM's role snapshot is never a write capability.
    let decision = (|| {
        crate::authz::require_role(
            db,
            &caller.identity,
            project_id,
            crate::db::models::Role::Lead,
        )?;
        let field = match field.as_str() {
            "name" => Field::Name,
            "description" => Field::Description,
            "emoji" => Field::Emoji,
            "identifier" => Field::Identifier,
            _ => return Err(LificError::BadRequest("Unknown project field".into())),
        };
        let saved = {
            let conn = db.read()?;
            crate::db::queries::get_project(&conn, project_id)?
        };
        Ok((saved.clone(), model::field_patch(field, &value, &saved)))
    })();
    let result = match decision {
        Ok((saved, None)) => Ok(("unchanged", saved)),
        Ok((_, Some(input))) => {
            caller
                .scope(async {
                    crate::services::project_overview::update(
                        db,
                        app_context::<RealtimeHub>(cx),
                        &caller.identity,
                        caller.session_token.as_deref(),
                        project_id,
                        input,
                    )
                    .map(|project| ("saved", project))
                })
                .await
        }
        Err(error) => Err(error),
    };
    match result {
        Ok((status, project)) => Ok((
            Ok(status.into()),
            project.name,
            project.description,
            project.emoji.unwrap_or_default(),
            project.identifier,
        )),
        Err(error) => Ok((
            Err(error_message(error)),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        )),
    }
}

#[procedure("/__native_overview/assign_group")]
pub(super) async fn assign_group(
    cx: &Cx,
    account_id: i64,
    project_id: i64,
    group_value: String,
) -> topcoat::Result<Result<String, String>> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account_id {
        return Ok(Err("Your account changed. Reload this page.".into()));
    }
    let group_id = if group_value.is_empty() {
        None
    } else {
        match group_value.parse::<i64>() {
            Ok(id) if id > 0 => Some(id),
            _ => return Ok(Err("Invalid sidebar group".into())),
        }
    };
    Ok(caller
        .scope(async {
            crate::services::project_form::assign_project(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                project_id,
                group_id,
            )
            .map(|()| "saved".into())
            .map_err(error_message)
        })
        .await)
}
