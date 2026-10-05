//! Project bundle export preparation shared by REST and native download routes.
use super::export::{PreparedExport, blocking_export, stream_response};
use crate::{
    authz,
    db::{DbPool, models::Role},
    error::LificError,
    resolve_caller::ResolvedIdentity,
};
use axum::response::Response;

/// Refresh account flags and permissions at the snapshot/stream boundary.
fn authorize(
    conn: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
) -> Result<Option<ResolvedIdentity>, LificError> {
    let fresh = identity
        .as_ref()
        .map(|caller| {
            crate::auth::fresh_caller(conn, caller.user.id)
                .map(|user| crate::auth::fresh_identity(&user, caller.transport))
        })
        .transpose()?;
    authz::require_role_conn(conn, &fresh, project_id, Role::Viewer)?;
    Ok(fresh)
}

pub(crate) async fn project(
    db: DbPool,
    identity: &Option<ResolvedIdentity>,
    identifier: String,
    format: Option<String>,
) -> Result<Response, LificError> {
    if let Some(format) = format.as_deref()
        && !matches!(format, "json" | "zip")
    {
        return Err(LificError::BadRequest(
            "invalid export format. Expected 'zip' or 'json'".into(),
        ));
    }
    // A mutable identifier is resolved only here. Every later boundary keeps
    // the selected row ID, including a worker that runs after a rename.
    let project_id = {
        let conn = db.read()?;
        let project_id = crate::db::queries::resolve_project_identifier(&conn, &identifier)?;
        authorize(&conn, identity, project_id)?;
        project_id
    };
    let slot = db.acquire_export_slot()?;
    let format = format.unwrap_or_else(|| "zip".into());
    let work_db = db.clone();
    let work_identity = identity.clone();
    let (bundle, slot) = blocking_export(slot, move || {
        let conn = work_db.read()?;
        let tx = conn.unchecked_transaction()?;
        let fresh = authorize(&tx, &work_identity, project_id)?;
        let visible = authz::visible_project_ids_conn(&tx, &fresh)?;
        let bundle =
            crate::export::export_project_snapshot_by_id(&tx, project_id, visible.as_ref())?;
        tx.commit()?;
        Ok(bundle)
    })
    .await?;
    let (prepared, slot) = match format.as_str() {
        "json" => blocking_export(slot, move || PreparedExport::json(&bundle)).await?,
        "zip" => blocking_export(slot, move || PreparedExport::zip(&bundle)).await?,
        _ => unreachable!("format was validated before export"),
    };
    // Preparation can also queue; denied callers receive no response bytes.
    {
        let conn = db.read()?;
        authorize(&conn, identity, project_id)?;
    }
    stream_response(prepared, slot).await
}

#[cfg(test)]
#[path = "export_project_tests.rs"]
mod tests;
