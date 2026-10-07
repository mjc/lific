//! Authenticated native page writes. Services own authorization and SQLite.
use super::super::{context, session};
use crate::{
    db::models::{CreatePage, Page, UpdatePage},
    error::LificError,
    realtime::RealtimeHub,
};
use topcoat::{
    context::{Cx, app_context},
    runtime::procedure,
};

pub(super) type Outcome = (
    Result<String, String>,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
);

#[procedure("/__native_pages/create")]
pub(super) async fn create(
    cx: &Cx,
    account: i64,
    project_id: i64,
    title: String,
) -> topcoat::Result<Outcome> {
    let caller = session::read(cx, context::caller(cx))?;
    match crate::api::require_user(&caller.identity) {
        Ok(user) if user.id == account => {}
        Ok(_) => return Ok(failed("forbidden")),
        Err(LificError::Forbidden(message)) if message == "authentication required" => {
            return Ok(failed("reauth"));
        }
        Err(error) => return Ok(classify(error)),
    }
    let result = caller
        .scope(async {
            crate::services::pages::commit_create(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                CreatePage {
                    project_id: Some(project_id),
                    title,
                    ..Default::default()
                },
            )
        })
        .await;
    Ok(result.map_or_else(classify, page_outcome))
}

#[procedure("/__native_pages/save")]
pub(super) async fn save(
    cx: &Cx,
    account: i64,
    page_id: i64,
    title: String,
    content: String,
    expected_seq: i64,
) -> topcoat::Result<Outcome> {
    let caller = session::read(cx, context::caller(cx))?;
    match crate::api::require_user(&caller.identity) {
        Ok(user) if user.id == account => {}
        Ok(_) => return Ok(failed("forbidden")),
        Err(LificError::Forbidden(message)) if message == "authentication required" => {
            return Ok(failed("reauth"));
        }
        Err(error) => return Ok(classify(error)),
    }
    let result = caller
        .scope(async {
            crate::services::pages::commit_update(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                page_id,
                UpdatePage {
                    title: Some(title),
                    content: Some(content),
                    expected_seq: Some(expected_seq),
                    ..Default::default()
                },
            )
        })
        .await;
    Ok(match result {
        Ok(page) => page_outcome(page),
        Err(LificError::UpdateConflict { current, .. }) => match serde_json::from_value(*current) {
            Ok(page) => {
                let mut result = page_outcome(page);
                result.0 = Err("conflict".into());
                result
            }
            Err(error) => failed(&format!("Couldn't read conflicting page: {error}")),
        },
        Err(error) => classify(error),
    })
}

#[procedure("/__native_pages/delete")]
pub(super) async fn delete(cx: &Cx, account: i64, page_id: i64) -> topcoat::Result<Outcome> {
    let caller = session::read(cx, context::caller(cx))?;
    match crate::api::require_user(&caller.identity) {
        Ok(user) if user.id == account => {}
        Ok(_) => return Ok(failed("forbidden")),
        Err(LificError::Forbidden(message)) if message == "authentication required" => {
            return Ok(failed("reauth"));
        }
        Err(error) => return Ok(classify(error)),
    }
    let result = caller
        .scope(async {
            crate::services::pages::commit_delete(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                page_id,
            )
        })
        .await;
    Ok(result.map_or_else(classify, |_| {
        (Ok("deleted".into()), Some(page_id), None, None, None, None)
    }))
}

fn page_outcome(page: Page) -> Outcome {
    (
        Ok("saved".into()),
        Some(page.id),
        Some(page.identifier),
        Some(page.title),
        Some(page.content),
        Some(page.seq),
    )
}

fn failed(message: &str) -> Outcome {
    (Err(message.into()), None, None, None, None, None)
}

fn classify(error: LificError) -> Outcome {
    match error {
        LificError::Forbidden(message) if message == "authentication required" => failed("reauth"),
        LificError::Forbidden(_) => failed("forbidden"),
        LificError::NotFound(_) => failed("This page no longer exists."),
        LificError::BadRequest(message) => failed(&message),
        error => {
            tracing::warn!(error=%error, "native page action failed");
            failed("Couldn't save the page. Try again.")
        }
    }
}
