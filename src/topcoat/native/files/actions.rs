//! Fresh-authority procedures for the Files manager.
use topcoat::{
    context::{Cx, app_context},
    runtime::procedure,
};

use super::super::{context, session};

type EntityWire = (String, i64, Option<String>, String, Option<i64>);
type AttachmentWire = (
    i64,
    String,
    String,
    String,
    i64,
    Option<i64>,
    Option<String>,
    Option<String>,
    String,
    Vec<EntityWire>,
);
type PageWire = (Vec<AttachmentWire>, bool, i64, i64);
type OrphanWire = (
    i64,
    String,
    String,
    i64,
    Option<i64>,
    Option<String>,
    String,
    i64,
    i64,
);
type WhereUsedWire = (Vec<EntityWire>, Vec<(i64, String, Vec<EntityWire>)>);

#[procedure("/__native_files/query")]
async fn query_files(
    cx: &Cx,
    account: i64,
    project_id: i64,
    mime_class: Option<String>,
    uploader: String,
    sort: String,
    offset: i64,
) -> topcoat::Result<PageWire> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return session::read(
            cx,
            Err(crate::error::LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    if offset < 0 || !["created_at", "size", "filename"].contains(&sort.as_str()) {
        return Err(topcoat::router::error::bad_request("invalid Files query").into());
    }
    if mime_class.as_deref().is_some_and(|value| {
        !["image", "video", "audio", "text", "pdf", "archive", "other"].contains(&value)
    }) {
        return Err(topcoat::router::error::bad_request("invalid Files MIME filter").into());
    }
    let query = crate::db::models::ProjectAttachmentQuery {
        mime_class,
        uploader: (!uploader.is_empty()).then_some(uploader),
        sort: Some(sort),
        limit: Some(super::model::PAGE_SIZE),
        offset: Some(offset),
        ..Default::default()
    };
    let db = context::db(cx).clone();
    let identity = caller.identity.clone();
    let page = session::read(
        cx,
        caller
            .scope(async move {
                crate::services::files::list_project_files(&db, &identity, project_id, &query)
            })
            .await,
    )?;
    Ok((
        page.items.into_iter().map(attachment_wire).collect(),
        page.has_more,
        page.total_count,
        page.total_bytes,
    ))
}

#[procedure("/__native_files/orphans")]
async fn query_orphans(
    cx: &Cx,
    account: i64,
    project_id: i64,
) -> topcoat::Result<(Vec<OrphanWire>, i64, i64)> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return session::read(
            cx,
            Err(crate::error::LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let db = context::db(cx).clone();
    let identity = caller.identity.clone();
    let list = session::read(
        cx,
        caller
            .scope(async move {
                crate::services::files::list_project_orphans(&db, &identity, project_id)
            })
            .await,
    )?;
    Ok((
        list.items
            .into_iter()
            .map(|item| {
                (
                    item.id,
                    item.filename,
                    item.mime,
                    item.size_bytes,
                    item.uploader_id,
                    item.uploader,
                    item.uploaded_at,
                    item.age_seconds,
                    item.seconds_until_sweep,
                )
            })
            .collect(),
        list.grace_seconds,
        list.total_bytes,
    ))
}

#[procedure("/__native_files/where_used")]
async fn where_used(cx: &Cx, account: i64, attachment_id: i64) -> topcoat::Result<WhereUsedWire> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return session::read(
            cx,
            Err(crate::error::LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let db = context::db(cx).clone();
    let identity = caller.identity.clone();
    let value = session::read(
        cx,
        caller
            .scope(async move { crate::services::files::where_used(&db, &identity, attachment_id) })
            .await,
    )?;
    Ok((
        value.entities.into_iter().map(entity_wire).collect(),
        value
            .duplicates
            .into_iter()
            .map(|duplicate| {
                (
                    duplicate.attachment_id,
                    duplicate.filename,
                    duplicate.entities.into_iter().map(entity_wire).collect(),
                )
            })
            .collect(),
    ))
}

fn entity_wire(entity: crate::db::models::LinkedEntity) -> EntityWire {
    (
        entity.entity_type,
        entity.entity_id,
        entity.identifier,
        entity.title,
        entity.page_id,
    )
}

fn attachment_wire(item: crate::db::models::ProjectAttachment) -> AttachmentWire {
    (
        item.id,
        item.filename,
        item.mime,
        item.mime_class,
        item.size_bytes,
        item.uploader_id,
        item.uploader,
        item.uploader_display_name,
        item.created_at,
        item.entities.into_iter().map(entity_wire).collect(),
    )
}

#[procedure("/__native_files/delete")]
pub(super) async fn delete(cx: &Cx, account: i64, attachment_id: i64) -> topcoat::Result<()> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return session::read(
            cx,
            Err(crate::error::LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let db = context::db(cx).clone();
    let hub = app_context::<crate::realtime::RealtimeHub>(cx).clone();
    let store = app_context::<crate::storage::AttachmentStore>(cx).clone();
    let identity = caller.identity.clone();
    session::read(
        cx,
        caller
            .scope(async move {
                crate::services::files::delete(&db, &hub, &store, &identity, attachment_id)
            })
            .await,
    )
}
