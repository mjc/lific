//! LIF-467: whole-project archives over HTTP.
//!
//! An archive is a project's entire history, tombstones and audit log
//! included, so only a live browser session reaches these routes: never an API
//! key, operator key, OAuth token, bot, or the first-admin identity an
//! authentication-disabled instance hands a credential-less request. The
//! session is re-read at every decision point, including inside the
//! transaction that reads the history or writes the project.
//!
//! Export needs Lead or instance admin. Import needs instance admin in both
//! authorization modes, because it creates a project and grants a lead
//! membership. Limits come from [`Limits::WEB`] and are published by
//! `GET /api/project-archives`.

use axum::Extension;
use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::db::DbPool;
use crate::error::LificError;
use crate::project_archive::Limits;
use crate::realtime::RealtimeHub;
use crate::storage::AttachmentStore;

#[cfg(test)]
use crate::services::project_archive_export::authorize_export;
use crate::services::project_archive_export::require_human_session;
#[cfg(test)]
use crate::services::project_archive_import::authorize_import;

pub(super) use crate::services::project_archive_import::ARCHIVE_UPLOAD_BODY_LIMIT;

#[cfg(test)]
tokio::task_local! {
    /// Test-only profile, so the refusal paths can be proven with kilobytes
    /// instead of a 128 MiB fixture. Task-scoped, not global, so parallel
    /// tests cannot see each other's.
    static TEST_WEB_LIMITS: Limits;
}

/// The profile this request runs under.
fn web_limits() -> Limits {
    #[cfg(test)]
    if let Ok(limits) = TEST_WEB_LIMITS.try_with(|limits| *limits) {
        return limits;
    }
    Limits::WEB
}

// GET /api/project-archives
#[derive(serde::Serialize)]
pub(super) struct ArchiveCapabilities {
    /// From the same freshly-read session user the import gate uses, so it
    /// cannot promise something the import will refuse.
    can_import: bool,
    max_upload_bytes: u64,
    max_expanded_bytes: u64,
    max_metadata_bytes: u64,
    max_blob_bytes: u64,
    /// Combined blob bytes. Not derivable from the others, and an archive can
    /// be under every one of them and still fail this.
    max_blob_total_bytes: u64,
    max_rows: usize,
    max_blobs: usize,
}

pub(super) async fn archive_capabilities(
    State(db): State<DbPool>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    headers: HeaderMap,
) -> Result<axum::Json<ArchiveCapabilities>, LificError> {
    let caller = require_human_session(&db, &identity, &headers)?;
    let limits = web_limits();
    Ok(axum::Json(ArchiveCapabilities {
        can_import: caller.is_admin,
        max_upload_bytes: limits.max_compressed,
        max_expanded_bytes: limits.max_expanded,
        max_metadata_bytes: limits.max_metadata,
        max_blob_bytes: limits.max_blob,
        max_blob_total_bytes: limits.max_blob_total,
        max_rows: limits.max_rows,
        max_blobs: limits.max_blobs,
    }))
}

// GET /api/project-archives/{identifier}
pub(super) async fn export_project_archive(
    State(db): State<DbPool>,
    Extension(store): Extension<AttachmentStore>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Path(identifier): Path<String>,
    headers: HeaderMap,
) -> Result<Response, LificError> {
    crate::services::project_archive_export::download(
        db,
        store,
        &identity,
        identifier,
        headers,
        None,
        web_limits(),
    )
    .await
}

// POST /api/project-archives
pub(super) async fn import_project_archive(
    State(db): State<DbPool>,
    Extension(store): Extension<AttachmentStore>,
    Extension(realtime): Extension<RealtimeHub>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    headers: HeaderMap,
    // Last, because this is what consumes the request body. Every check above
    // runs before a byte of a potentially 128 MiB upload is parsed.
    multipart: Multipart,
) -> Result<Response, LificError> {
    let report = crate::services::project_archive_import::upload(
        db,
        store,
        realtime,
        &identity,
        &headers,
        multipart,
        web_limits(),
    )
    .await?;
    Ok((StatusCode::CREATED, axum::Json(report)).into_response())
}

#[cfg(test)]
mod tests;
