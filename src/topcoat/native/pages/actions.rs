//! Authenticated native page writes. Services own authorization and SQLite.
use super::super::{context, session};
use crate::{
    db::{
        models::{CreateFolder, CreatePage, Folder, Page, UpdatePage},
        queries::ResourceTable,
    },
    error::LificError,
    realtime::RealtimeHub,
};
use topcoat::{
    context::{Cx, app_context},
    runtime::{procedure, record},
};

#[record]
#[derive(Clone)]
pub(super) struct Outcome {
    pub status: Result<String, String>,
    pub page_id: Option<i64>,
    pub identifier: Option<String>,
    pub title: Option<String>,
    pub content: Option<String>,
    pub seq: Option<i64>,
}

#[record]
#[derive(Clone)]
pub(super) struct StatusOutcome {
    pub status: Result<String, String>,
    pub page_status: Option<String>,
    pub seq: Option<i64>,
}

#[record]
#[derive(Clone)]
pub(super) struct MetadataOutcome {
    pub status: Result<String, String>,
    pub page_status: Option<String>,
    pub pinned: Option<bool>,
    pub seq: Option<i64>,
}

#[record]
#[derive(Clone)]
pub(super) struct MoveOutcome {
    pub status: Result<String, String>,
}

#[record]
#[derive(Clone)]
pub(super) struct FolderOutcome {
    pub status: Result<String, String>,
    pub folder_id: Option<i64>,
    pub folder_name: Option<String>,
}

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

#[procedure("/__native_pages/create-folder")]
pub(super) async fn create_folder(
    cx: &Cx,
    account: i64,
    project_id: i64,
    name: String,
    parent_value: String,
) -> topcoat::Result<FolderOutcome> {
    let caller = session::read(cx, context::caller(cx))?;
    match crate::api::require_user(&caller.identity) {
        Ok(user) if user.id == account => {}
        Ok(_) => return Ok(failed_folder("forbidden")),
        Err(LificError::Forbidden(message)) if message == "authentication required" => {
            return Ok(failed_folder("reauth"));
        }
        Err(error) => {
            let result = classify(error);
            return Ok(FolderOutcome {
                status: result.status,
                folder_id: None,
                folder_name: None,
            });
        }
    }
    let parent_id = if parent_value.is_empty() || parent_value == "0" {
        None
    } else {
        match parent_value.parse::<i64>() {
            Ok(id) if id > 0 => Some(id),
            _ => return Ok(failed_folder("Invalid parent folder.")),
        }
    };
    let result = caller
        .scope(async {
            crate::services::structure::commit_create(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                project_id,
                |conn| {
                    crate::db::queries::create_folder(
                        conn,
                        &CreateFolder {
                            project_id,
                            parent_id,
                            name,
                        },
                    )
                },
            )
        })
        .await;
    Ok(result.map_or_else(
        |error| {
            let result = classify(error);
            FolderOutcome {
                status: result.status,
                folder_id: None,
                folder_name: None,
            }
        },
        folder_outcome,
    ))
}

#[procedure("/__native_pages/delete-folder")]
pub(super) async fn delete_folder(
    cx: &Cx,
    account: i64,
    project_id: i64,
    folder_id: i64,
) -> topcoat::Result<FolderOutcome> {
    let caller = session::read(cx, context::caller(cx))?;
    match crate::api::require_user(&caller.identity) {
        Ok(user) if user.id == account => {}
        Ok(_) => return Ok(failed_folder("forbidden")),
        Err(LificError::Forbidden(message)) if message == "authentication required" => {
            return Ok(failed_folder("reauth"));
        }
        Err(error) => return Ok(failed_folder(error.client_message())),
    }
    let result = caller
        .scope(async {
            crate::services::structure::commit_delete(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                ResourceTable::Folders,
                folder_id,
                |conn, owning_project_id| {
                    if owning_project_id != project_id {
                        return Err(LificError::Forbidden(
                            "This folder belongs to a different project.".into(),
                        ));
                    }
                    crate::db::queries::delete_folder(conn, folder_id)
                },
            )
        })
        .await;
    Ok(result.map_or_else(
        |error| failed_folder(error.client_message()),
        |_| FolderOutcome {
            status: Ok("deleted".into()),
            folder_id: None,
            folder_name: None,
        },
    ))
}

#[derive(Clone, Copy)]
enum PageSaveField {
    Title,
    Content,
}

impl PageSaveField {
    fn update(self, value: String, expected_seq: i64) -> UpdatePage {
        match self {
            Self::Title => UpdatePage {
                title: Some(value),
                expected_seq: Some(expected_seq),
                ..Default::default()
            },
            Self::Content => UpdatePage {
                content: Some(value),
                expected_seq: Some(expected_seq),
                ..Default::default()
            },
        }
    }
}

async fn commit_page_save(
    cx: &Cx,
    account: i64,
    page_id: i64,
    update: UpdatePage,
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
                update,
            )
        })
        .await;
    Ok(match result {
        Ok(page) => page_outcome(page),
        Err(LificError::UpdateConflict { current, .. }) => match serde_json::from_value(*current) {
            Ok(page) => {
                let mut result = page_outcome(page);
                result.status = Err("conflict".into());
                result
            }
            Err(error) => failed(&format!("Couldn't read conflicting page: {error}")),
        },
        Err(error) => classify(error),
    })
}

async fn save_field(
    cx: &Cx,
    account: i64,
    page_id: i64,
    value: String,
    expected_seq: i64,
    field: PageSaveField,
) -> topcoat::Result<Outcome> {
    commit_page_save(cx, account, page_id, field.update(value, expected_seq)).await
}

#[procedure("/__native_pages/save_title")]
pub(super) async fn save_title(
    cx: &Cx,
    account: i64,
    page_id: i64,
    title: String,
    expected_seq: i64,
) -> topcoat::Result<Outcome> {
    save_field(
        cx,
        account,
        page_id,
        title,
        expected_seq,
        PageSaveField::Title,
    )
    .await
}

#[procedure("/__native_pages/save_content")]
pub(super) async fn save_content(
    cx: &Cx,
    account: i64,
    page_id: i64,
    content: String,
    expected_seq: i64,
) -> topcoat::Result<Outcome> {
    save_field(
        cx,
        account,
        page_id,
        content,
        expected_seq,
        PageSaveField::Content,
    )
    .await
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
    commit_page_save(
        cx,
        account,
        page_id,
        UpdatePage {
            title: Some(title),
            content: Some(content),
            expected_seq: Some(expected_seq),
            ..Default::default()
        },
    )
    .await
}

#[procedure("/__native_pages/status")]
pub(super) async fn set_status(
    cx: &Cx,
    account: i64,
    page_id: i64,
    status: String,
    expected_seq: i64,
) -> topcoat::Result<StatusOutcome> {
    let outcome = update_metadata(
        cx,
        account,
        page_id,
        expected_seq,
        PageMetadata::Status(status),
    )
    .await?;
    Ok(StatusOutcome {
        status: outcome.status,
        page_status: outcome.page_status,
        seq: outcome.seq,
    })
}

#[procedure("/__native_pages/pin")]
pub(super) async fn set_pinned(
    cx: &Cx,
    account: i64,
    page_id: i64,
    pinned: bool,
    expected_seq: i64,
) -> topcoat::Result<MetadataOutcome> {
    update_metadata(
        cx,
        account,
        page_id,
        expected_seq,
        PageMetadata::Pinned(pinned),
    )
    .await
}

#[procedure("/__native_pages/move")]
pub(super) async fn move_to_folder(
    cx: &Cx,
    account: i64,
    page_id: i64,
    folder_value: String,
) -> topcoat::Result<MoveOutcome> {
    let caller = session::read(cx, context::caller(cx))?;
    match crate::api::require_user(&caller.identity) {
        Ok(user) if user.id == account => {}
        Ok(_) => return Ok(failed_move("forbidden")),
        Err(LificError::Forbidden(message)) if message == "authentication required" => {
            return Ok(failed_move("reauth"));
        }
        Err(error) => return Ok(failed_move(&classify(error).status.unwrap_err())),
    }
    let folder_id = if folder_value.is_empty() {
        None
    } else {
        match folder_value.parse::<i64>() {
            Ok(folder_id) if folder_id > 0 => Some(folder_id),
            _ => return Ok(failed_move("Invalid folder selection.")),
        }
    };
    let result = caller
        .scope(async {
            crate::services::pages::commit_update(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                page_id,
                UpdatePage {
                    folder_id: Some(folder_id),
                    ..Default::default()
                },
            )
        })
        .await;
    Ok(result.map_or_else(
        |error| failed_move(&classify(error).status.unwrap_err()),
        |_| MoveOutcome {
            status: Ok("saved".to_owned()),
        },
    ))
}

fn failed_move(message: &str) -> MoveOutcome {
    MoveOutcome {
        status: Err(message.to_owned()),
    }
}

fn folder_outcome(folder: Folder) -> FolderOutcome {
    FolderOutcome {
        status: Ok("saved".into()),
        folder_id: Some(folder.id),
        folder_name: Some(folder.name),
    }
}

fn failed_folder(message: &str) -> FolderOutcome {
    FolderOutcome {
        status: Err(message.into()),
        folder_id: None,
        folder_name: None,
    }
}

enum PageMetadata {
    Status(String),
    Pinned(bool),
}

async fn update_metadata(
    cx: &Cx,
    account: i64,
    page_id: i64,
    expected_seq: i64,
    metadata: PageMetadata,
) -> topcoat::Result<MetadataOutcome> {
    let caller = session::read(cx, context::caller(cx))?;
    match crate::api::require_user(&caller.identity) {
        Ok(user) if user.id == account => {}
        Ok(_) => return Ok(failed_metadata("forbidden")),
        Err(LificError::Forbidden(message)) if message == "authentication required" => {
            return Ok(failed_metadata("reauth"));
        }
        Err(error) => return Ok(classify_metadata(error)),
    }
    let update = match metadata {
        PageMetadata::Status(status)
            if matches!(
                status.as_str(),
                "draft" | "active" | "complete" | "archived"
            ) =>
        {
            UpdatePage {
                status: Some(status),
                expected_seq: Some(expected_seq),
                ..Default::default()
            }
        }
        PageMetadata::Status(_) => return Ok(failed_metadata("Invalid page status.")),
        PageMetadata::Pinned(pinned) => UpdatePage {
            pinned: Some(pinned),
            expected_seq: Some(expected_seq),
            ..Default::default()
        },
    };
    let result = caller
        .scope(async {
            crate::services::pages::commit_update(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                page_id,
                update,
            )
        })
        .await;
    Ok(match result {
        Ok(page) => metadata_outcome(Ok("saved".into()), page),
        Err(LificError::UpdateConflict { current, .. }) => {
            match serde_json::from_value::<Page>(*current) {
                Ok(page) => metadata_outcome(Err("conflict".into()), page),
                Err(error) => failed_metadata(&format!("Couldn't read conflicting page: {error}")),
            }
        }
        Err(error) => classify_metadata(error),
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
    Ok(result.map_or_else(classify, |_| Outcome {
        status: Ok("deleted".into()),
        page_id: Some(page_id),
        identifier: None,
        title: None,
        content: None,
        seq: None,
    }))
}

fn page_outcome(page: Page) -> Outcome {
    Outcome {
        status: Ok("saved".into()),
        page_id: Some(page.id),
        identifier: Some(page.identifier),
        title: Some(page.title),
        content: Some(page.content),
        seq: Some(page.seq),
    }
}

fn failed(message: &str) -> Outcome {
    Outcome {
        status: Err(message.into()),
        page_id: None,
        identifier: None,
        title: None,
        content: None,
        seq: None,
    }
}

fn failed_metadata(message: &str) -> MetadataOutcome {
    MetadataOutcome {
        status: Err(message.into()),
        page_status: None,
        pinned: None,
        seq: None,
    }
}

fn metadata_outcome(status: Result<String, String>, page: Page) -> MetadataOutcome {
    MetadataOutcome {
        status,
        page_status: Some(page.status),
        pinned: Some(page.pinned),
        seq: Some(page.seq),
    }
}

fn classify_metadata(error: LificError) -> MetadataOutcome {
    let outcome = classify(error);
    MetadataOutcome {
        status: outcome.status,
        page_status: None,
        pinned: None,
        seq: None,
    }
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
