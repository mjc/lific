//! Whole-project archive gates and preparation shared by native and REST adapters.
use super::export::{PreparedExport, blocking_export, stream_response};
use crate::{
    authz,
    db::{
        DbPool,
        models::{Role, User},
        queries,
    },
    error::LificError,
    project_archive::{self, Limits},
    storage::AttachmentStore,
};
use axum::{
    http::{HeaderMap, HeaderValue, header},
    response::Response,
};
use rusqlite::Connection;
fn with_read<T>(
    db: &DbPool,
    read: impl FnOnce(&Connection) -> Result<T, LificError>,
) -> Result<T, LificError> {
    let conn = db.read()?;
    read(&conn)
}
fn denied() -> LificError {
    LificError::Forbidden("this action requires a signed-in browser session".into())
}

/// A caller that presented a live browser session.
pub(crate) struct SessionCaller {
    /// The session's own user. Immutable for the rest of the request: every
    /// later check re-reads the database and compares against this id.
    pub(crate) user_id: i64,
    pub(crate) token: String,
    /// Instance admin as of the read that produced this value. Never reused
    /// as the authority for a write; the transaction re-reads it.
    pub(crate) is_admin: bool,
}

/// The whole credential policy in one function, so the routes and the
/// in-transaction re-checks cannot drift apart. `validate_session` covers
/// expiry and deactivation; `is_bot` closes the door on a connected tool that
/// somehow holds a session of its own.
pub(crate) fn session_user(
    conn: &Connection,
    token: &str,
    expected_user_id: i64,
) -> Result<User, LificError> {
    let user = queries::users::validate_session(conn, token).map_err(|_| denied())?;
    if user.id != expected_user_id || user.is_bot || !user.is_active {
        return Err(denied());
    }
    Ok(user)
}

/// The gate every route runs first. `session_bearer_token` keeps API keys,
/// operator keys and OAuth tokens out; comparing the session's user to the
/// middleware's identity keeps a token swapped between the two reads out.
pub(crate) fn require_human_session(
    db: &DbPool,
    identity: &Option<crate::resolve_caller::ResolvedIdentity>,
    headers: &HeaderMap,
) -> Result<SessionCaller, LificError> {
    let caller = crate::api::require_user(identity).map_err(|_| denied())?;
    let token = crate::auth::session_bearer_token(headers)?;
    let user = with_read(db, |conn| session_user(conn, &token, caller.id))?;
    Ok(SessionCaller {
        user_id: user.id,
        token,
        is_admin: user.is_admin,
    })
}

/// Re-run the export gate on `conn` against the session as the database has
/// it right now, not against the middleware's snapshot.
pub(crate) fn authorize_export(
    conn: &Connection,
    token: &str,
    user_id: i64,
    project_id: i64,
) -> Result<(), LificError> {
    let user = session_user(conn, token, user_id)?;
    let identity = Some(crate::auth::fresh_identity(
        &user,
        crate::actor::Transport::Web,
    ));
    authz::require_role_conn(conn, &identity, project_id, Role::Lead)
}

/// Live browser capability; this carries no credential to the caller.
pub(crate) fn capability(
    db: &DbPool,
    identity: &Option<crate::resolve_caller::ResolvedIdentity>,
    headers: &HeaderMap,
) -> Result<(i64, bool), LificError> {
    let user = require_human_session(db, identity, headers)?;
    Ok((user.user_id, user.is_admin))
}
/// Recheck a fresh HTTP cookie after the completed download and before saving it.
pub(crate) fn verify_owner(
    db: &DbPool,
    identity: &Option<crate::resolve_caller::ResolvedIdentity>,
    headers: &HeaderMap,
    expected_owner: i64,
    project: i64,
) -> Result<(), LificError> {
    let caller = require_human_session(db, identity, headers)?;
    if caller.user_id != expected_owner {
        return Err(LificError::Forbidden(
            "Your account changed. Download the archive again after signing in.".into(),
        ));
    }
    with_read(db, |conn| {
        authorize_export(conn, &caller.token, caller.user_id, project)
    })
}
pub(crate) async fn download(
    db: DbPool,
    store: AttachmentStore,
    identity: &Option<crate::resolve_caller::ResolvedIdentity>,
    identifier: String,
    headers: HeaderMap,
    expected_project: Option<i64>,
    limits: Limits,
) -> Result<Response, LificError> {
    let caller = require_human_session(&db, identity, &headers)?;
    // The path segment is resolved to a row ID once, and everything after
    // this point uses the ID. An identifier is a mutable label: re-resolving
    // it in the snapshot could land on a project this caller was never
    // authorized for.
    let project_id = with_read(&db, |conn| {
        queries::resolve_project_identifier(conn, &identifier)
    })?;
    if expected_project.is_some_and(|expected| expected != project_id) {
        return Err(LificError::Forbidden(
            "The project changed. Download the archive again.".into(),
        ));
    }
    // Denied before a slot is taken, so a caller with no access cannot make
    // the instance refuse somebody else's archive.
    with_read(&db, |conn| {
        authorize_export(conn, &caller.token, caller.user_id, project_id)
    })?;

    let slot = db.acquire_archive_slot()?;
    let temp_dir = tempfile::tempdir()
        .map_err(|error| LificError::Internal(format!("create archive temp dir: {error}")))?;
    let path = temp_dir.path().join("archive.tar.gz");

    let work_db = db.clone();
    let work_path = path.clone();
    let token = caller.token.clone();
    let user_id = caller.user_id;
    // The temp directory is owned by the blocking closure, so a client that
    // disconnects mid-export cannot delete the file the worker is writing.
    let (report, temp_dir, slot) = blocking_export(slot, move || {
        let report = project_archive::export_by_id_with(
            &work_db,
            &store,
            project_id,
            &work_path,
            limits,
            // Re-checked inside the snapshot, before a single issue body,
            // comment or audit row is read.
            &|conn| authorize_export(conn, &token, user_id, project_id),
        )?;
        Ok((report, temp_dir))
    })
    .await
    .map(|((report, temp_dir), slot)| (report, temp_dir, slot))?;

    // A queued export can land long after it was authorized, so the gate runs
    // once more before any byte is sent.
    with_read(&db, |conn| {
        authorize_export(conn, &caller.token, caller.user_id, project_id)
    })?;

    let mut extra_headers = HeaderMap::new();
    extra_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    extra_headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    extra_headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    stream_response(
        PreparedExport {
            temp_dir,
            path,
            content_type: HeaderValue::from_static("application/gzip"),
            // The identifier the snapshot itself saw, so the filename always
            // names the project whose bytes these are.
            download_name: Some(format!("{}.lific.tar.gz", report.project)),
            extra_headers,
        },
        slot,
    )
    .await
}
