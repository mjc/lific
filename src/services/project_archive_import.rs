//! Whole-project archive uploads shared by native and REST adapters.

use std::path::PathBuf;
use std::time::Duration;

use axum::extract::Multipart;
use axum::http::{HeaderMap, StatusCode};
use rusqlite::Connection;
use tokio::io::AsyncWriteExt;

use super::export::blocking_export;
use super::project_archive_export::{require_human_session, session_user};
use crate::db::DbPool;
use crate::db::models::User;
use crate::error::LificError;
use crate::project_archive::{self, Limits};
use crate::realtime::{RealtimeEvent, RealtimeHub};
use crate::storage::AttachmentStore;

/// Compressed WEB limit plus multipart framing, shared by both HTTP adapters.
pub(crate) const ARCHIVE_UPLOAD_BODY_LIMIT: usize =
    Limits::WEB.max_compressed as usize + 1024 * 1024;

/// The only accepted multipart field. Anything else, including a second copy
/// of this one, is refused rather than partially honored.
const ARCHIVE_FIELD: &str = "archive";

/// A slot is held for the whole upload, so neither bound can be open-ended.
const UPLOAD_MAX_DURATION: Duration = Duration::from_secs(120);
const UPLOAD_IDLE_TIMEOUT: Duration = Duration::from_secs(20);

fn oversize(limits: Limits) -> LificError {
    LificError::PayloadTooLarge(format!(
        "a project archive upload may not exceed {} bytes",
        limits.max_compressed
    ))
}

fn not_admin() -> LificError {
    LificError::Forbidden("only an admin can import a project archive".into())
}

/// Instance admin in both authorization modes: this creates a project and
/// grants a lead membership, which the legacy mode has no role model for.
pub(crate) fn authorize_import(
    conn: &Connection,
    token: &str,
    user_id: i64,
) -> Result<User, LificError> {
    let user = session_user(conn, token, user_id)?;
    if !user.is_admin {
        return Err(not_admin());
    }
    Ok(user)
}

#[derive(serde::Serialize)]
pub(crate) struct ImportedProject {
    pub(crate) id: i64,
    pub(crate) identifier: String,
    /// Always false. Format 1 carries no publication flag and the importer
    /// never sets one; publishing is a separate, deliberate decision.
    pub(crate) is_public: bool,
}

#[derive(serde::Serialize)]
pub(crate) struct ImportReport {
    pub(crate) project: String,
    pub(crate) rows: std::collections::BTreeMap<&'static str, usize>,
    pub(crate) blobs: usize,
    /// At most [`MAX_REPORTED_REFERENCES`] messages. A hostile or merely
    /// enormous archive can generate tens of thousands, and a response nobody
    /// can render is not a better answer than a truncated one.
    pub(crate) external_references: Vec<String>,
    /// How many were generated in total, so a client can say "showing 100 of
    /// 12,431" rather than silently implying it has them all.
    pub(crate) external_reference_count: usize,
}

#[derive(serde::Serialize)]
pub(crate) struct ImportResponse {
    pub(crate) project: ImportedProject,
    pub(crate) report: ImportReport,
}

/// The report is a response body, not a log file. The full list is stored on
/// `project_archive_provenance` and comes back out in the next archive.
const MAX_REPORTED_REFERENCES: usize = 100;

pub(crate) async fn upload(
    db: DbPool,
    store: AttachmentStore,
    realtime: RealtimeHub,
    identity: &Option<crate::resolve_caller::ResolvedIdentity>,
    headers: &HeaderMap,
    multipart: Multipart,
    limits: Limits,
) -> Result<ImportResponse, LificError> {
    let caller = require_human_session(&db, identity, headers)?;
    if !caller.is_admin {
        return Err(not_admin());
    }
    // Capacity before body: a request that will not run should not cost the
    // instance a spooled upload.
    let slot = db.acquire_archive_slot()?;
    let (temp_dir, path) = spool_archive(multipart, limits).await?;

    let work_db = db.clone();
    let token = caller.token.clone();
    let user_id = caller.user_id;
    // `temp_dir` moves into the closure and dies with it, so a cancelled
    // request cannot pull the staged upload out from under a running import.
    // No timeout wraps this: an import that commits must never be reported as
    // a failure, because the caller would then retry a project that exists.
    let (outcome, slot) = blocking_export(slot, move || {
        let _staged = temp_dir;
        let outcome = project_archive::import_with(&work_db, &store, &path, limits, &|tx| {
            let admin = authorize_import(tx, &token, user_id)?;
            Ok(project_archive::Grant {
                user_id: admin.id,
                // This person, through the browser. The CLI's actorless
                // grant is the CLI's answer, not this one.
                transport: crate::actor::Transport::Web,
                actor_user_id: Some(admin.id),
            })
        })?;
        // The commit and notification outlive a disconnected HTTP request.
        realtime.send(RealtimeEvent::ProjectUpdated {
            project_id: outcome.project_id,
        });
        Ok(outcome)
    })
    .await?;
    drop(slot);

    let mut external_references = outcome.report.external_references;
    external_references.truncate(MAX_REPORTED_REFERENCES);
    Ok(ImportResponse {
        project: ImportedProject {
            id: outcome.project_id,
            identifier: outcome.report.project.clone(),
            is_public: false,
        },
        report: ImportReport {
            project: outcome.report.project,
            rows: outcome.rows_by_table,
            blobs: outcome.report.blobs,
            external_references,
            external_reference_count: outcome.external_reference_count,
        },
    })
}

/// Stream the uploaded archive to a private temp file.
///
/// Nothing the client says about the payload is trusted: not the filename,
/// not the content type, not a declared length. The bytes are counted as they
/// arrive and the upload is cut off the moment it passes the ceiling.
async fn spool_archive(
    mut multipart: Multipart,
    limits: Limits,
) -> Result<(tempfile::TempDir, PathBuf), LificError> {
    let temp_dir = tempfile::tempdir()
        .map_err(|error| LificError::Internal(format!("create upload temp dir: {error}")))?;
    let path = temp_dir.path().join("upload.tar.gz");
    let spooled = tokio::time::timeout(
        UPLOAD_MAX_DURATION,
        read_archive_field(&mut multipart, &path, limits),
    )
    .await
    .map_err(|_| LificError::BadRequest("the upload took too long".into()))?;
    spooled?;
    Ok((temp_dir, path))
}

async fn read_archive_field(
    multipart: &mut Multipart,
    path: &std::path::Path,
    limits: Limits,
) -> Result<(), LificError> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(path)
        .await
        .map_err(|error| LificError::Internal(format!("stage upload: {error}")))?;

    let mut written = 0u64;
    let mut seen = false;
    while let Some(mut field) = idle(multipart.next_field(), limits).await?? {
        if seen || field.name() != Some(ARCHIVE_FIELD) {
            return Err(LificError::BadRequest(
                "send exactly one multipart field named 'archive'".into(),
            ));
        }
        seen = true;
        while let Some(chunk) = idle(field.chunk(), limits).await?? {
            written = written.saturating_add(chunk.len() as u64);
            if written > limits.max_compressed {
                return Err(oversize(limits));
            }
            file.write_all(&chunk)
                .await
                .map_err(|error| LificError::Internal(format!("stage upload: {error}")))?;
        }
    }
    if !seen {
        return Err(LificError::BadRequest(
            "send exactly one multipart field named 'archive'".into(),
        ));
    }
    file.flush()
        .await
        .map_err(|error| LificError::Internal(format!("stage upload: {error}")))?;
    file.sync_all()
        .await
        .map_err(|error| LificError::Internal(format!("stage upload: {error}")))
}

/// Bound one read of the request body, so a client that opens a connection
/// and then trickles cannot hold the archive slot for the full deadline.
async fn idle<T>(
    read: impl Future<Output = Result<T, axum::extract::multipart::MultipartError>>,
    limits: Limits,
) -> Result<Result<T, LificError>, LificError> {
    match tokio::time::timeout(UPLOAD_IDLE_TIMEOUT, read).await {
        Err(_) => Err(LificError::BadRequest("the upload stalled".into())),
        Ok(Ok(value)) => Ok(Ok(value)),
        Ok(Err(error)) => Ok(Err(multipart_error(&error, limits))),
    }
}

/// Size is a 413, framing is a 400. The message is ours, never the caller's
/// bytes echoed back.
fn multipart_error(error: &axum::extract::multipart::MultipartError, limits: Limits) -> LificError {
    if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        oversize(limits)
    } else {
        LificError::BadRequest("malformed multipart upload".into())
    }
}
