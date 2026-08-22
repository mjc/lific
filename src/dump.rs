//! LIF-266: backup as an interface — `lific dump` / `lific restore`.
//!
//! The data set is no longer a single file. Since attachments (LIF-262) it is
//! `lific.db` plus a content-addressed `attachments/` sidecar dir living beside
//! it (blobs named by their sha256; in-progress writes carry a `.tmp`
//! extension — see [`crate::storage`]). A DB snapshot alone would restore
//! metadata rows pointing at missing blobs.
//!
//! This module follows the `gitea dump` pattern: one command produces one
//! self-contained, timestamped `lific_YYYYMMDD_HHMMSS.tar.gz` archive with
//! everything needed to restore — the DB, every attachment blob, and a
//! `manifest.json` describing what's inside. [`restore`] validates and unpacks
//! it back into a data dir. The interval backup task (`src/backup.rs`) emits
//! the *same* artifact via [`write_dump`], so there is exactly one backup shape.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::DbPool;
use crate::error::LificError;

/// The DB filename inside every archive, independent of the on-disk name.
pub const ARCHIVE_DB_NAME: &str = "lific.db";
/// The manifest filename inside every archive.
pub const ARCHIVE_MANIFEST_NAME: &str = "manifest.json";
/// The prefix under which attachment blobs are stored inside the archive.
pub const ARCHIVE_ATTACHMENTS_PREFIX: &str = "attachments/";

/// Restore limits apply to the uncompressed archive contents. They prevent a
/// tiny gzip/tar upload from allocating unbounded memory or filling the data
/// volume before validation can reject it.
pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
pub const MAX_DB_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_ATTACHMENT_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_TOTAL_RESTORE_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_RESTORE_ENTRIES: u64 = 10_000;
const ATTACHMENTS_SCHEMA_VERSION: i64 = 31;
const TAR_ENTRY_OVERHEAD: u64 = 1024;
const MAX_ARCHIVE_DECOMPRESSED_BYTES: u64 =
    MAX_TOTAL_RESTORE_BYTES + MAX_MANIFEST_BYTES + (MAX_RESTORE_ENTRIES + 2) * TAR_ENTRY_OVERHEAD;

/// Metadata describing an archive's contents. Serialized as `manifest.json`
/// at the root of every dump so a restore can validate compatibility and print
/// a summary without opening the DB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Crate version that produced the archive.
    pub lific_version: String,
    /// Highest applied migration version in the snapshotted DB. A restore onto
    /// an older binary (lower [`crate::db::migrate::latest_version`]) is
    /// refused.
    pub schema_version: i64,
    /// ISO 8601 UTC timestamp the dump was taken.
    pub created_at: String,
    /// Size of the snapshotted DB file in bytes.
    pub db_size_bytes: u64,
    /// Number of attachment blobs included.
    pub attachment_count: u64,
    /// Total bytes across all attachment blobs.
    pub attachment_bytes: u64,
}

/// A UTC timestamp in the archive filename convention (`YYYYMMDD_HHMMSS`),
/// matching the legacy backup naming so both schemes sort together.
pub fn archive_timestamp() -> String {
    chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string()
}

/// The default archive filename for a given DB stem and timestamp, e.g.
/// `lific_20260101_120000.tar.gz`.
pub fn archive_filename(db_stem: &str, timestamp: &str) -> String {
    format!("{db_stem}_{timestamp}.tar.gz")
}

/// Resolve the attachments sidecar dir for a database path (mirrors
/// [`crate::storage::AttachmentStore::from_db_path`]).
fn attachments_dir_for(db_path: &Path) -> PathBuf {
    match db_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join("attachments"),
        _ => PathBuf::from("attachments"),
    }
}

/// Take a consistent snapshot of the live DB into `dest` using `VACUUM INTO`.
///
/// `VACUUM INTO` runs on a read connection, holds no long writer lock, and
/// compacts + snapshots in one step — safe while the server is running. The
/// destination must not already exist (SQLite requirement).
fn snapshot_db(pool: &DbPool, dest: &Path) -> Result<(), LificError> {
    if dest.exists() {
        std::fs::remove_file(dest)
            .map_err(|e| LificError::Internal(format!("clear snapshot target: {e}")))?;
    }
    let conn = pool.read()?;
    // Parameterized VACUUM INTO with the destination path as a bound value.
    conn.execute("VACUUM INTO ?1", [&dest.to_string_lossy()])
        .map_err(|e| LificError::Internal(format!("VACUUM INTO snapshot failed: {e}")))?;
    Ok(())
}

/// Set 0600 permissions on a file (owner-only) on Unix. No-op elsewhere.
fn set_owner_only(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Write a self-contained dump archive to `out_path`.
///
/// Shared code path used by both `lific dump` and the interval backup task.
/// Produces a gzip-compressed tar containing `lific.db` (a consistent snapshot
/// via `VACUUM INTO`), every non-`.tmp` attachment blob under `attachments/`,
/// and `manifest.json`. The finished file is chmod 0600 (it contains the whole
/// DB).
///
/// Returns the [`Manifest`] that was written, so callers can log/print it.
pub fn write_dump(pool: &DbPool, db_path: &Path, out_path: &Path) -> Result<Manifest, LificError> {
    // Snapshot the DB to a temp file next to the output, so the archive holds a
    // consistent point-in-time copy rather than a possibly-mid-write live file.
    let tmp_db = out_path.with_extension("dbsnapshot.tmp");
    snapshot_db(pool, &tmp_db)?;

    // Staging path for the archive itself; the closure writes here and
    // atomically renames into place on success. Declared out here so the
    // error path below can clean up a partial archive (LIF-329).
    let tmp_archive = out_path.with_extension("archive.tmp");

    // Guard: always clean the temp snapshot even on the error paths below.
    let result = (|| {
        let db_size_bytes = std::fs::metadata(&tmp_db).map(|m| m.len()).unwrap_or(0);

        // Gather attachment blobs (skip .tmp in-progress writes).
        let attachments_dir = attachments_dir_for(db_path);
        let mut blobs: Vec<(String, PathBuf, u64)> = Vec::new();
        let mut attachment_bytes: u64 = 0;
        if attachments_dir.is_dir() {
            for entry in std::fs::read_dir(&attachments_dir)
                .map_err(|e| LificError::Internal(format!("read attachments dir: {e}")))?
            {
                let entry = entry
                    .map_err(|e| LificError::Internal(format!("read attachments entry: {e}")))?;
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) == Some("tmp") {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                attachment_bytes += size;
                blobs.push((name.to_string(), path.clone(), size));
            }
        }

        let manifest = Manifest {
            lific_version: env!("CARGO_PKG_VERSION").to_string(),
            schema_version: schema_version(pool),
            created_at: chrono::Utc::now().to_rfc3339(),
            db_size_bytes,
            attachment_count: blobs.len() as u64,
            attachment_bytes,
        };
        let manifest_json = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| LificError::Internal(format!("serialize manifest: {e}")))?;

        // Build the archive into a temp file, then atomically rename into place
        // so a partial write is never observed at the final path.
        {
            let file = std::fs::File::create(&tmp_archive)
                .map_err(|e| LificError::Internal(format!("create archive: {e}")))?;
            let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut tar = tar::Builder::new(enc);

            // manifest.json
            append_bytes(&mut tar, ARCHIVE_MANIFEST_NAME, &manifest_json)?;
            // lific.db (from the snapshot file)
            tar.append_path_with_name(&tmp_db, ARCHIVE_DB_NAME)
                .map_err(|e| LificError::Internal(format!("append db to archive: {e}")))?;
            // attachments/<sha256>
            for (name, path, _size) in &blobs {
                let entry_name = format!("{ARCHIVE_ATTACHMENTS_PREFIX}{name}");
                tar.append_path_with_name(path, &entry_name)
                    .map_err(|e| LificError::Internal(format!("append attachment {name}: {e}")))?;
            }

            let enc = tar
                .into_inner()
                .map_err(|e| LificError::Internal(format!("finalize tar: {e}")))?;
            enc.finish()
                .map_err(|e| LificError::Internal(format!("finalize gzip: {e}")))?;
        }

        set_owner_only(&tmp_archive)
            .map_err(|e| LificError::Internal(format!("chmod archive: {e}")))?;
        std::fs::rename(&tmp_archive, out_path)
            .map_err(|e| LificError::Internal(format!("finalize archive: {e}")))?;

        Ok(manifest)
    })();

    let _ = std::fs::remove_file(&tmp_db);
    if result.is_err() {
        // A failure after the archive staging file was created would otherwise
        // strand a partial `*.archive.tmp` that rotation never touches
        // (LIF-329).
        let _ = std::fs::remove_file(&tmp_archive);
    }
    result
}

/// Append raw bytes as a tar entry with the given name.
fn append_bytes<W: std::io::Write>(
    tar: &mut tar::Builder<W>,
    name: &str,
    bytes: &[u8],
) -> Result<(), LificError> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    header.set_mtime(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    );
    header.set_cksum();
    tar.append_data(&mut header, name, bytes)
        .map_err(|e| LificError::Internal(format!("append {name}: {e}")))
}

/// Read the highest applied migration version from the DB (0 if unavailable).
fn schema_version(pool: &DbPool) -> i64 {
    pool.read()
        .ok()
        .and_then(|conn| {
            conn.query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _migrations",
                [],
                |row| row.get::<_, i64>(0),
            )
            .ok()
        })
        .unwrap_or(0)
}

// ── dump (CLI) ───────────────────────────────────────────────

/// Result of a `lific dump`, for printing/JSON output.
pub struct DumpResult {
    pub archive_path: PathBuf,
    pub manifest: Manifest,
}

/// Run `lific dump`: resolve the output path (file or directory), snapshot the
/// DB at `db_path`, and write the archive. `out` may be `None` (current dir), a
/// directory (default filename inside it), or a full target file path.
pub fn run_dump(db_path: &Path, out: Option<&Path>) -> Result<DumpResult, LificError> {
    let pool = crate::db::open(db_path)?;

    let db_stem = db_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("lific");
    let default_name = archive_filename(db_stem, &archive_timestamp());

    let archive_path = match out {
        None => std::env::current_dir()
            .map_err(|e| LificError::Internal(format!("resolve current dir: {e}")))?
            .join(&default_name),
        Some(p) => {
            // A path that exists as a dir, or ends with a separator, is a
            // target directory → use the default filename inside it.
            if p.is_dir() {
                p.join(&default_name)
            } else {
                p.to_path_buf()
            }
        }
    };

    if let Some(parent) = archive_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|e| LificError::Internal(format!("create output dir: {e}")))?;
    }

    let manifest = write_dump(&pool, db_path, &archive_path)?;
    Ok(DumpResult {
        archive_path,
        manifest,
    })
}

// ── restore (CLI) ────────────────────────────────────────────

/// Summary returned by a successful restore for printing/JSON.
#[derive(Debug)]
pub struct RestoreResult {
    pub manifest: Manifest,
    pub attachment_count: u64,
    pub db_path: PathBuf,
    /// Where the pre-existing DB was moved, if `--force` displaced one.
    pub moved_existing_to: Option<PathBuf>,
}

/// Validate that an attachment entry name is a bare hash under `attachments/`
/// with no path traversal. Returns the bare filename on success.
fn validate_attachment_entry(name: &str) -> Result<String, LificError> {
    let rest = name
        .strip_prefix(ARCHIVE_ATTACHMENTS_PREFIX)
        .ok_or_else(|| LificError::BadRequest(format!("unexpected archive entry: {name}")))?;
    // Reject empty, nested paths, parent refs, absolute paths, or anything with
    // a separator — a blob name is a bare sha256 hex string.
    if rest.is_empty()
        || rest.contains('/')
        || rest.contains('\\')
        || rest.contains("..")
        || rest.starts_with('.')
    {
        return Err(LificError::BadRequest(format!(
            "rejected attachment entry (path traversal or invalid name): {name}"
        )));
    }
    if !crate::storage::valid_sha256(rest) {
        return Err(LificError::BadRequest(format!(
            "rejected attachment entry (expected lowercase SHA-256): {name}"
        )));
    }
    Ok(rest.to_string())
}

fn validate_manifest_limits(manifest: &Manifest) -> Result<(), LificError> {
    if manifest.db_size_bytes > MAX_DB_BYTES {
        return Err(LificError::BadRequest(format!(
            "archive database exceeds restore limit ({} > {} bytes)",
            manifest.db_size_bytes, MAX_DB_BYTES
        )));
    }
    if manifest.attachment_count > MAX_RESTORE_ENTRIES {
        return Err(LificError::BadRequest(format!(
            "archive has too many attachments ({} > {})",
            manifest.attachment_count, MAX_RESTORE_ENTRIES
        )));
    }
    if manifest.attachment_bytes > MAX_TOTAL_RESTORE_BYTES
        || manifest.db_size_bytes > MAX_TOTAL_RESTORE_BYTES - manifest.attachment_bytes
    {
        return Err(LificError::BadRequest(
            "archive contents exceed total restore size limit".into(),
        ));
    }
    Ok(())
}

fn read_entry_bounded<R: Read>(
    entry: &mut tar::Entry<'_, R>,
    max_bytes: u64,
    label: &str,
) -> Result<Vec<u8>, LificError> {
    let size = entry.size();
    if size > max_bytes {
        return Err(LificError::BadRequest(format!(
            "{label} exceeds restore limit ({size} > {max_bytes} bytes)"
        )));
    }
    let mut buf = Vec::with_capacity(size as usize);
    entry
        .read_to_end(&mut buf)
        .map_err(|e| LificError::BadRequest(format!("read {label}: {e}")))?;
    Ok(buf)
}

fn bounded_archive_with_limit(
    file: std::fs::File,
    max_decompressed_bytes: u64,
) -> tar::Archive<std::io::Take<flate2::read::GzDecoder<std::fs::File>>> {
    let decoder = flate2::read::GzDecoder::new(file);
    tar::Archive::new(decoder.take(max_decompressed_bytes))
}

fn bounded_archive(
    file: std::fs::File,
) -> tar::Archive<std::io::Take<flate2::read::GzDecoder<std::fs::File>>> {
    bounded_archive_with_limit(file, MAX_ARCHIVE_DECOMPRESSED_BYTES)
}

fn validate_tar_entry_type<R: Read>(entry: &tar::Entry<'_, R>) -> Result<(), LificError> {
    if !entry.header().entry_type().is_file() {
        return Err(LificError::BadRequest(
            "archive contains an unsupported tar entry type".into(),
        ));
    }
    Ok(())
}

fn copy_entry_bounded<R: Read, W: Write>(
    entry: &mut tar::Entry<'_, R>,
    output: &mut W,
    max_bytes: u64,
    total_bytes: &mut u64,
    label: &str,
) -> Result<u64, LificError> {
    let size = entry.size();
    if size > max_bytes {
        return Err(LificError::BadRequest(format!(
            "{label} exceeds restore limit ({size} > {max_bytes} bytes)"
        )));
    }
    let new_total = total_bytes
        .checked_add(size)
        .ok_or_else(|| LificError::BadRequest("archive size overflow".into()))?;
    if new_total > MAX_TOTAL_RESTORE_BYTES {
        return Err(LificError::BadRequest(
            "archive contents exceed total restore size limit".into(),
        ));
    }

    let copied = std::io::copy(entry, output)
        .map_err(|e| LificError::BadRequest(format!("read {label}: {e}")))?;
    if copied != size {
        return Err(LificError::BadRequest(format!(
            "read {label}: expected {size} bytes, got {copied}"
        )));
    }
    *total_bytes = new_total;
    Ok(copied)
}

fn hash_file(path: &Path) -> Result<String, LificError> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| LificError::BadRequest(format!("open attachment: {e}")))?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 8192];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| LificError::BadRequest(format!("read attachment: {e}")))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    let digest = digest.finalize();
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn validate_attachment_schema(conn: &rusqlite::Connection) -> Result<(), LificError> {
    let sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'attachments'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| LificError::BadRequest(format!("read staged attachment schema: {e}")))?;
    let probe = rusqlite::Connection::open_in_memory()
        .map_err(|e| LificError::BadRequest(format!("create staged schema probe: {e}")))?;
    probe
        .execute_batch("CREATE TABLE users (id INTEGER PRIMARY KEY);")
        .map_err(|e| LificError::BadRequest(format!("create staged schema probe: {e}")))?;
    probe
        .execute_batch(&sql)
        .map_err(|e| LificError::BadRequest(format!("create staged schema probe: {e}")))?;

    let insert = |sha256: &str, mime: &str, size_bytes: i64| {
        probe.execute(
            "INSERT INTO attachments (sha256, filename, mime, size_bytes)
             VALUES (?1, 'schema-probe', ?2, ?3)",
            rusqlite::params![sha256, mime, size_bytes],
        )
    };
    let valid_sha = crate::storage::AttachmentStore::hash_bytes(b"schema-probe");
    let invalid_mime_sha = "b".repeat(64);
    let invalid_size_sha = "c".repeat(64);
    insert(&valid_sha, "text/plain", 0).map_err(|e| {
        LificError::BadRequest(format!("staged attachment schema rejects valid metadata: {e}"))
    })?;

    for (sha256, mime, size_bytes) in [
        ("../outside", "text/plain", 0),
        (invalid_mime_sha.as_str(), "application/octet-stream", 0),
        (invalid_size_sha.as_str(), "text/plain", -1),
    ] {
        if insert(sha256, mime, size_bytes).is_ok() {
            return Err(LificError::BadRequest(
                "staged attachment table is missing required integrity constraints".into(),
            ));
        }
    }
    Ok(())
}

/// Validate the extracted SQLite file before it can replace the live DB. This
/// catches corrupt archives and ensures every metadata attachment reference is
/// a safe content-addressed filename with matching staged bytes.
fn validate_staged_database(staging: &Path, manifest: &Manifest) -> Result<(), LificError> {
    let db = staging.join(ARCHIVE_DB_NAME);
    let conn =
        rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| LificError::BadRequest(format!("open staged database: {e}")))?;
    let check: String = conn
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|e| LificError::BadRequest(format!("validate staged database: {e}")))?;
    if check != "ok" {
        return Err(LificError::BadRequest(format!(
            "staged database integrity check failed: {check}"
        )));
    }

    let schema_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM _migrations",
            [],
            |row| row.get(0),
        )
        .map_err(|e| LificError::BadRequest(format!("read staged schema version: {e}")))?;
    if schema_version != manifest.schema_version {
        return Err(LificError::BadRequest(format!(
            "staged database schema version {schema_version} does not match manifest {}",
            manifest.schema_version
        )));
    }
    if schema_version > crate::db::migrate::latest_version() {
        return Err(LificError::BadRequest(format!(
            "staged database schema version {schema_version} is newer than this binary"
        )));
    }

    let has_attachments: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'attachments')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| LificError::BadRequest(format!("inspect staged schema: {e}")))?;
    if !has_attachments
        && (schema_version >= ATTACHMENTS_SCHEMA_VERSION
            || manifest.attachment_count != 0
            || manifest.attachment_bytes != 0)
    {
        return Err(LificError::BadRequest(
            "staged database is missing the attachments table".into(),
        ));
    }

    if has_attachments && schema_version < ATTACHMENTS_SCHEMA_VERSION {
        return Err(LificError::BadRequest(
            "staged database has attachment tables at an unsupported schema version".into(),
        ));
    }

    if has_attachments {
        validate_attachment_schema(&conn)?;
        let mime_placeholders = vec!["?"; crate::storage::ALLOWED_MIMES.len()].join(", ");
        let mut stmt = conn
            .prepare(&format!(
                "SELECT sha256, MIN(size_bytes), MAX(size_bytes),
                        SUM(CASE WHEN mime IN ({mime_placeholders}) THEN 0 ELSE 1 END)
                 FROM attachments
                 GROUP BY sha256"
            ))
            .map_err(|e| LificError::BadRequest(format!("read staged attachment metadata: {e}")))?;
        let rows = stmt
            .query_map(
                rusqlite::params_from_iter(crate::storage::ALLOWED_MIMES.iter().copied()),
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .map_err(|e| LificError::BadRequest(format!("read staged attachment metadata: {e}")))?;
        for row in rows {
            let (sha, min_size, max_size, invalid_mimes) = row.map_err(|e| {
                LificError::BadRequest(format!("read staged attachment metadata: {e}"))
            })?;
            if !crate::storage::valid_sha256(&sha)
                || min_size < 0
                || min_size != max_size
                || max_size as u64 > MAX_ATTACHMENT_BYTES
                || invalid_mimes != 0
            {
                return Err(LificError::BadRequest(
                    "staged attachment metadata has an invalid content address, MIME, or size"
                        .into(),
                ));
            }
            let size = max_size as u64;
            let path = staging.join("attachments").join(&sha);
            let metadata = std::fs::metadata(&path).map_err(|_| {
                LificError::BadRequest(format!(
                    "staged database references missing attachment {sha}"
                ))
            })?;
            if !metadata.is_file() || metadata.len() != size {
                return Err(LificError::BadRequest(format!(
                    "staged attachment {sha} does not match database metadata"
                )));
            }
            if hash_file(&path)? != sha {
                return Err(LificError::BadRequest(format!(
                    "staged attachment {sha} does not match its content address"
                )));
            }
        }

        let mut mime_stmt = conn
            .prepare("SELECT DISTINCT sha256, mime FROM attachments")
            .map_err(|e| LificError::BadRequest(format!("read staged attachment MIME: {e}")))?;
        let mime_rows = mime_stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| LificError::BadRequest(format!("read staged attachment MIME: {e}")))?;
        for row in mime_rows {
            let (sha, declared_mime) = row
                .map_err(|e| LificError::BadRequest(format!("read staged attachment MIME: {e}")))?;
            let path = staging.join("attachments").join(&sha);
            let bytes = std::fs::read(&path).map_err(|e| {
                LificError::BadRequest(format!("read staged attachment {sha}: {e}"))
            })?;
            let detected_mime = crate::storage::sniff_and_validate(&bytes, Some(&declared_mime))?;
            if detected_mime != declared_mime {
                return Err(LificError::BadRequest(format!(
                    "staged attachment {sha} MIME {declared_mime} does not match its content ({detected_mime})"
                )));
            }
        }
    }

    let attachments_dir = staging.join("attachments");
    for entry in std::fs::read_dir(&attachments_dir)
        .map_err(|e| LificError::BadRequest(format!("read staged attachments: {e}")))?
    {
        let entry = entry
            .map_err(|e| LificError::BadRequest(format!("read staged attachment: {e}")))?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|e| {
            LificError::BadRequest(format!("inspect staged attachment {}: {e}", path.display()))
        })?;
        if !metadata.is_file() {
            return Err(LificError::BadRequest(format!(
                "staged attachment is not a regular file: {}",
                path.display()
            )));
        }
        let name = entry.file_name().into_string().map_err(|_| {
            LificError::BadRequest(format!(
                "staged attachment has a non-UTF-8 name: {}",
                path.display()
            ))
        })?;
        if !crate::storage::valid_sha256(&name) {
            return Err(LificError::BadRequest(format!(
                "staged attachment has an invalid content address: {name}"
            )));
        }
        if metadata.len() > MAX_ATTACHMENT_BYTES {
            return Err(LificError::BadRequest(format!(
                "staged attachment exceeds restore limit: {name}"
            )));
        }
        if hash_file(&path)? != name {
            return Err(LificError::BadRequest(format!(
                "staged attachment {name} does not match its content address"
            )));
        }
    }
    Ok(())
}

/// Read and validate an archive's manifest + entry list without extracting.
/// Returns the parsed manifest. Rejects archives missing `manifest.json` or
/// `lific.db`, and any attachment entry that fails [`validate_attachment_entry`].
pub fn inspect_archive(archive: &Path) -> Result<Manifest, LificError> {
    let manifest = read_manifest(archive)?;
    validate_manifest_limits(&manifest)?;

    // Second pass: validate every entry name (traversal guard) and require the
    // DB member is present.
    let file = std::fs::File::open(archive)
        .map_err(|e| LificError::BadRequest(format!("open archive: {e}")))?;
    let mut tar = bounded_archive(file);
    let mut has_db = false;
    let mut db_bytes = None;
    let mut manifest_entries = 0u64;
    let mut attachment_names = std::collections::HashSet::new();
    let mut entry_count = 0u64;
    let mut attachment_count = 0u64;
    let mut attachment_bytes = 0u64;
    let mut payload_bytes = 0u64;
    for entry in tar
        .entries()
        .map_err(|e| LificError::BadRequest(format!("read archive entries: {e}")))?
        .raw(true)
    {
        let entry = entry.map_err(|e| LificError::BadRequest(format!("read entry: {e}")))?;
        validate_tar_entry_type(&entry)?;
        entry_count += 1;
        if entry_count > MAX_RESTORE_ENTRIES + 2 {
            return Err(LificError::BadRequest(
                "archive has too many entries".into(),
            ));
        }
        let size = entry.size();
        let path = entry
            .path()
            .map_err(|e| LificError::BadRequest(format!("entry path: {e}")))?;
        let name = path.to_string_lossy().replace('\\', "/");
        if name == ARCHIVE_MANIFEST_NAME {
            manifest_entries += 1;
            if manifest_entries > 1 {
                return Err(LificError::BadRequest(
                    "archive contains duplicate manifests".into(),
                ));
            }
            continue;
        } else if name == ARCHIVE_DB_NAME {
            if has_db {
                return Err(LificError::BadRequest(
                    "archive contains duplicate databases".into(),
                ));
            }
            if size > MAX_DB_BYTES {
                return Err(LificError::BadRequest(
                    "archive database exceeds restore limit".into(),
                ));
            }
            has_db = true;
            db_bytes = Some(size);
            payload_bytes = payload_bytes
                .checked_add(size)
                .ok_or_else(|| LificError::BadRequest("archive size overflow".into()))?;
        } else if name.starts_with(ARCHIVE_ATTACHMENTS_PREFIX) {
            validate_attachment_entry(&name)?;
            if !attachment_names.insert(name.clone()) {
                return Err(LificError::BadRequest(format!(
                    "archive contains duplicate attachment: {name}"
                )));
            }
            if size > MAX_ATTACHMENT_BYTES {
                return Err(LificError::BadRequest(
                    "archive attachment exceeds restore limit".into(),
                ));
            }
            attachment_count += 1;
            attachment_bytes = attachment_bytes
                .checked_add(size)
                .ok_or_else(|| LificError::BadRequest("archive attachment size overflow".into()))?;
            payload_bytes = payload_bytes
                .checked_add(size)
                .ok_or_else(|| LificError::BadRequest("archive size overflow".into()))?;
            if attachment_count > MAX_RESTORE_ENTRIES
                || attachment_bytes > manifest.attachment_bytes
                || payload_bytes > MAX_TOTAL_RESTORE_BYTES
            {
                return Err(LificError::BadRequest(
                    "archive attachment contents exceed manifest limits".into(),
                ));
            }
        } else {
            return Err(LificError::BadRequest(format!(
                "rejected unexpected archive entry: {name}"
            )));
        }
    }
    if !has_db {
        return Err(LificError::BadRequest("archive is missing lific.db".into()));
    }
    if db_bytes != Some(manifest.db_size_bytes) {
        return Err(LificError::BadRequest(
            "archive database size does not match manifest".into(),
        ));
    }
    if attachment_count != manifest.attachment_count
        || attachment_bytes != manifest.attachment_bytes
    {
        return Err(LificError::BadRequest(
            "archive attachment entries do not match manifest".into(),
        ));
    }
    Ok(manifest)
}

/// Read just the manifest from the archive (first matching entry).
fn read_manifest(archive: &Path) -> Result<Manifest, LificError> {
    let file = std::fs::File::open(archive)
        .map_err(|e| LificError::BadRequest(format!("open archive: {e}")))?;
    let mut tar = bounded_archive(file);
    let mut entry_count = 0u64;
    for entry in tar
        .entries()
        .map_err(|e| LificError::BadRequest(format!("read archive entries: {e}")))?
        .raw(true)
    {
        let mut entry = entry.map_err(|e| LificError::BadRequest(format!("read entry: {e}")))?;
        validate_tar_entry_type(&entry)?;
        entry_count += 1;
        if entry_count > MAX_RESTORE_ENTRIES + 2 {
            return Err(LificError::BadRequest(
                "archive has too many entries before its manifest".into(),
            ));
        }
        let path = entry
            .path()
            .map_err(|e| LificError::BadRequest(format!("entry path: {e}")))?;
        if path.to_string_lossy() == ARCHIVE_MANIFEST_NAME {
            let bytes = read_entry_bounded(&mut entry, MAX_MANIFEST_BYTES, "manifest")?;
            let buf = String::from_utf8(bytes)
                .map_err(|e| LificError::BadRequest(format!("manifest is not UTF-8: {e}")))?;
            return serde_json::from_str(&buf)
                .map_err(|e| LificError::BadRequest(format!("parse manifest.json: {e}")));
        }
    }
    Err(LificError::BadRequest(
        "archive is missing manifest.json".into(),
    ))
}

/// Whether a hot WAL sidecar is present next to `db_path`, hinting the server
/// may still be running (best-effort — see command help).
fn wal_is_hot(db_path: &Path) -> bool {
    let wal = PathBuf::from(format!("{}-wal", db_path.display()));
    std::fs::metadata(&wal)
        .map(|m| m.len() > 0)
        .unwrap_or(false)
}

/// Run `lific restore`: validate the archive, then stage-extract it into the
/// data dir at `db_path`. Refuses to clobber an existing DB unless `force`;
/// with `force`, moves the existing DB + `-wal`/`-shm` aside. Refuses archives
/// created by a newer Lific (higher schema_version than this binary).
pub fn run_restore(
    archive: &Path,
    db_path: &Path,
    force: bool,
) -> Result<RestoreResult, LificError> {
    let manifest = inspect_archive(archive)?;

    // Schema compatibility gate.
    let latest = crate::db::migrate::latest_version();
    if manifest.schema_version > latest {
        return Err(LificError::BadRequest(format!(
            "archive was created by a newer Lific (schema v{} > this binary's v{}); \
             upgrade Lific before restoring",
            manifest.schema_version, latest
        )));
    }

    let data_dir = db_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| LificError::Internal(format!("create data dir: {e}")))?;

    if db_path.exists() && !force {
        return Err(LificError::Conflict(format!(
            "{} already exists; pass --force to restore over it (stop the server first)",
            db_path.display()
        )));
    }

    // Stage and validate the complete restore before moving any live state.
    let staging = data_dir.join(format!(".lific-restore-{}", archive_timestamp()));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .map_err(|e| LificError::Internal(format!("clear staging dir: {e}")))?;
    }
    std::fs::create_dir_all(staging.join("attachments"))
        .map_err(|e| LificError::Internal(format!("create staging dir: {e}")))?;

    let extract = (|| -> Result<u64, LificError> {
        let file = std::fs::File::open(archive)
            .map_err(|e| LificError::BadRequest(format!("open archive: {e}")))?;
        let mut tar = bounded_archive(file);
        let mut attachment_count = 0u64;
        let mut total_bytes = 0u64;
        for entry in tar
            .entries()
            .map_err(|e| LificError::BadRequest(format!("read archive entries: {e}")))?
            .raw(true)
        {
            let mut entry =
                entry.map_err(|e| LificError::BadRequest(format!("read entry: {e}")))?;
            validate_tar_entry_type(&entry)?;
            let epath = entry
                .path()
                .map_err(|e| LificError::BadRequest(format!("entry path: {e}")))?;
            let name = epath.to_string_lossy().replace('\\', "/");
            if name == ARCHIVE_MANIFEST_NAME {
                // Persist the manifest alongside the restored DB for provenance.
                let buf = read_entry_bounded(&mut entry, MAX_MANIFEST_BYTES, "manifest")?;
                std::fs::write(staging.join(ARCHIVE_MANIFEST_NAME), &buf)
                    .map_err(|e| LificError::Internal(format!("write manifest: {e}")))?;
            } else if name == ARCHIVE_DB_NAME {
                let db = staging.join(ARCHIVE_DB_NAME);
                let mut output = std::fs::File::create(&db)
                    .map_err(|e| LificError::Internal(format!("create staged db: {e}")))?;
                copy_entry_bounded(
                    &mut entry,
                    &mut output,
                    MAX_DB_BYTES,
                    &mut total_bytes,
                    "database",
                )?;
                set_owner_only(&db)
                    .map_err(|e| LificError::Internal(format!("chmod staged db: {e}")))?;
            } else if name.starts_with(ARCHIVE_ATTACHMENTS_PREFIX) {
                let bare = validate_attachment_entry(&name)?;
                let path = staging.join("attachments").join(&bare);
                let mut output = std::fs::File::create(&path)
                    .map_err(|e| LificError::Internal(format!("create staged attachment: {e}")))?;
                copy_entry_bounded(
                    &mut entry,
                    &mut output,
                    MAX_ATTACHMENT_BYTES,
                    &mut total_bytes,
                    "attachment",
                )?;
                set_owner_only(&path)
                    .map_err(|e| LificError::Internal(format!("chmod staged attachment: {e}")))?;
                attachment_count += 1;
            } else {
                return Err(LificError::BadRequest(format!(
                    "rejected unexpected archive entry: {name}"
                )));
            }
        }
        if !staging.join(ARCHIVE_DB_NAME).exists() {
            return Err(LificError::BadRequest("archive is missing lific.db".into()));
        }
        validate_staged_database(&staging, &manifest)?;
        Ok(attachment_count)
    })();

    let attachment_count = match extract {
        Ok(n) => n,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
    };

    // Recheck after staging in case another process created the destination
    // while the archive was being validated.
    if db_path.exists() && !force {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(LificError::Conflict(format!(
            "{} already exists; pass --force to restore over it (stop the server first)",
            db_path.display()
        )));
    }

    let mut moved_existing_to = None;
    if db_path.exists() {
        if let Err(error) = checkpoint_db_file(db_path) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error);
        }
        let suffix = format!("pre-restore-{}", archive_timestamp());
        let dest = PathBuf::from(format!("{}.{suffix}", db_path.display()));
        if let Err(error) = std::fs::rename(db_path, &dest) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(LificError::Internal(format!(
                "move existing db aside: {error}"
            )));
        }
        for ext in ["-wal", "-shm"] {
            let side = PathBuf::from(format!("{}{ext}", db_path.display()));
            if let Err(error) = remove_file_if_present(&side) {
                let _ = std::fs::remove_dir_all(&staging);
                let cause = LificError::Internal(format!("remove old {ext}: {error}"));
                return Err(rollback_moved_db(&dest, db_path, cause));
            }
        }
        moved_existing_to = Some(dest);
    }

    // Move restored files into place as one recoverable transaction. Keep the
    // old attachment directory until both the DB and new directory are live so
    // a filesystem failure cannot leave mismatched metadata and blobs.
    let attachments_dest = attachments_dir_for(db_path);
    let attachments_backup = PathBuf::from(format!(
        "{}.pre-restore-{}",
        attachments_dest.display(),
        archive_timestamp()
    ));
    let had_attachments = attachments_dest.exists();
    if had_attachments && let Err(e) = std::fs::rename(&attachments_dest, &attachments_backup) {
        let _ = std::fs::remove_dir_all(&staging);
        let cause = LificError::Internal(format!("move existing attachments aside: {e}"));
        return Err(match &moved_existing_to {
            Some(moved) => rollback_moved_db(moved, db_path, cause),
            None => cause,
        });
    }

    let install_result = (|| -> Result<(), LificError> {
        std::fs::rename(staging.join(ARCHIVE_DB_NAME), db_path)
            .map_err(|e| LificError::Internal(format!("install restored db: {e}")))?;
        set_owner_only(db_path)
            .map_err(|e| LificError::Internal(format!("chmod restored db: {e}")))?;
        std::fs::rename(staging.join("attachments"), &attachments_dest)
            .map_err(|e| LificError::Internal(format!("install restored attachments: {e}")))?;
        Ok(())
    })();

    if let Err(error) = install_result {
        return Err(rollback_install(
            error,
            &staging,
            db_path,
            &attachments_dest,
            &attachments_backup,
            had_attachments,
            moved_existing_to.as_deref(),
        ));
    }

    if had_attachments {
        let _ = std::fs::remove_dir_all(&attachments_backup);
    }

    let _ = std::fs::remove_dir_all(&staging);

    Ok(RestoreResult {
        manifest,
        attachment_count,
        db_path: db_path.to_path_buf(),
        moved_existing_to,
    })
}

fn remove_file_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn remove_dir_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn rollback_install(
    cause: LificError,
    staging: &Path,
    db_path: &Path,
    attachments_dest: &Path,
    attachments_backup: &Path,
    had_attachments: bool,
    moved_existing_to: Option<&Path>,
) -> LificError {
    let mut failures = Vec::new();
    if let Err(error) = remove_file_if_present(db_path) {
        failures.push(format!("remove installed database: {error}"));
    }
    if let Err(error) = remove_dir_if_present(attachments_dest) {
        failures.push(format!("remove installed attachments: {error}"));
    }
    if had_attachments && let Err(error) = std::fs::rename(attachments_backup, attachments_dest) {
        failures.push(format!("restore previous attachments: {error}"));
    }
    if let Some(moved) = moved_existing_to
        && let Err(error) = std::fs::rename(moved, db_path)
    {
        failures.push(format!(
            "restore previous database from {}: {error}",
            moved.display()
        ));
    }
    if let Err(error) = remove_dir_if_present(staging) {
        failures.push(format!("remove staging directory: {error}"));
    }

    if failures.is_empty() {
        cause
    } else {
        LificError::Internal(format!(
            "restore failed ({cause}); rollback also failed: {}",
            failures.join("; ")
        ))
    }
}

/// Put the database that `--force` moved aside back at `db_path` after a failed
/// restore, and decide which error the caller should surface.
///
/// On success the original failure (`cause`) is returned unchanged. If the
/// rollback rename itself fails, swallowing it would tell the user only that
/// the restore failed while their database sits at a path they were never
/// shown, reading exactly like data loss. So that case returns a combined
/// error carrying both failures and the exact path the original
/// database still occupies, plus where to move it back to.
fn rollback_moved_db(moved: &Path, db_path: &Path, cause: LificError) -> LificError {
    match std::fs::rename(moved, db_path) {
        Ok(()) => cause,
        Err(rollback_err) => LificError::Internal(format!(
            "restore failed ({cause}), and rolling the previous database back to {} failed too \
             ({rollback_err}). Your original database was NOT deleted: it is still at {}. \
             Move it back to {} by hand to recover it.",
            db_path.display(),
            moved.display(),
            db_path.display()
        )),
    }
}

/// Checkpoint a database file's WAL into the main file, so the `.db` is
/// self-contained (used before moving an existing db aside under `--force`).
/// Failing closed is important: deleting the sidecars after a failed
/// checkpoint could discard committed WAL-backed data.
fn checkpoint_db_file(db_path: &Path) -> Result<(), LificError> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| LificError::Internal(format!("open database for WAL checkpoint: {e}")))?;
    let busy: i64 = conn
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
        .map_err(|e| LificError::Internal(format!("checkpoint database WAL: {e}")))?;
    if busy != 0 {
        return Err(LificError::Conflict(
            "database WAL is busy; stop the server before forcing a restore".into(),
        ));
    }
    Ok(())
}

/// True when a hot WAL warns the server may be running. Exposed so the CLI can
/// print the best-effort warning documented in the command help.
pub fn server_maybe_running(db_path: &Path) -> bool {
    wal_is_hot(db_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Scratch directory for one test. The guard removes it on Drop, which
    /// also runs while a failed assertion unwinds, so nothing is left for a
    /// later run to trip over.
    fn temp_dir(tag: &str) -> TempDir {
        tempfile::Builder::new()
            .prefix(&format!("lific_dump_{tag}_"))
            .tempdir()
            .unwrap()
    }

    /// Build a real on-disk DB with a seeded project, plus an attachments dir
    /// containing one real blob and one `.tmp` stray. Returns (dir guard,
    /// db_path).
    fn seed_data_dir(tag: &str) -> (TempDir, PathBuf) {
        let tmp = temp_dir(tag);
        let dir = tmp.path();
        let db_path = dir.join("lific.db");
        {
            let pool = crate::db::open(&db_path).unwrap();
            let conn = pool.write().unwrap();
            crate::db::queries::create_project(
                &conn,
                &crate::db::models::CreateProject {
                    name: "DumpTest".into(),
                    identifier: "DMP".into(),
                    description: String::new(),
                    emoji: None,
                    lead_user_id: None,
                },
            )
            .unwrap();
        }
        let att = dir.join("attachments");
        fs::create_dir_all(&att).unwrap();
        let first_sha = crate::storage::AttachmentStore::hash_bytes(b"blob one");
        let second_sha = crate::storage::AttachmentStore::hash_bytes(b"second blob bytes");
        fs::write(
            att.join(&first_sha),
            b"blob one",
        )
        .unwrap();
        fs::write(
            att.join(&second_sha),
            b"second blob bytes",
        )
        .unwrap();
        fs::write(
            att.join(format!("{second_sha}.tmp")),
            b"partial write",
        )
        .unwrap();
        (tmp, db_path)
    }

    /// List the entry names inside an archive.
    fn archive_entries(archive: &Path) -> Vec<String> {
        let file = fs::File::open(archive).unwrap();
        let dec = flate2::read::GzDecoder::new(file);
        let mut tar = tar::Archive::new(dec);
        tar.entries()
            .unwrap()
            .map(|e| {
                e.unwrap()
                    .path()
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn dump_archive_contains_db_manifest_and_blobs_excluding_tmp() {
        let (dir_tmp, db_path) = seed_data_dir("contents");
        let dir = dir_tmp.path();
        let out = dir.join("out.tar.gz");
        let manifest = write_dump(&crate::db::open(&db_path).unwrap(), &db_path, &out).unwrap();

        let entries = archive_entries(&out);
        assert!(entries.contains(&ARCHIVE_DB_NAME.to_string()));
        assert!(entries.contains(&ARCHIVE_MANIFEST_NAME.to_string()));
        assert!(entries.contains(&format!(
            "{ARCHIVE_ATTACHMENTS_PREFIX}{}",
            crate::storage::AttachmentStore::hash_bytes(b"blob one")
        )));
        assert!(entries.contains(&format!(
            "{ARCHIVE_ATTACHMENTS_PREFIX}{}",
            crate::storage::AttachmentStore::hash_bytes(b"second blob bytes")
        )));
        assert!(
            !entries.iter().any(|e| e.ends_with(".tmp")),
            "in-progress .tmp writes must be excluded: {entries:?}"
        );

        assert_eq!(manifest.attachment_count, 2);
        assert_eq!(
            manifest.attachment_bytes,
            (b"blob one".len() + b"second blob bytes".len()) as u64
        );
        assert_eq!(manifest.lific_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(
            manifest.schema_version,
            crate::db::migrate::latest_version()
        );
        assert!(manifest.db_size_bytes > 0);
    }

    #[cfg(unix)]
    #[test]
    fn dump_archive_is_owner_only_0600() {
        use std::os::unix::fs::PermissionsExt;
        let (dir_tmp, db_path) = seed_data_dir("perms");
        let dir = dir_tmp.path();
        let out = dir.join("out.tar.gz");
        write_dump(&crate::db::open(&db_path).unwrap(), &db_path, &out).unwrap();
        let mode = fs::metadata(&out).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "archive must be chmod 0600");
    }

    #[test]
    fn failed_dump_cleans_up_its_staging_files() {
        // LIF-329: force a failure *after* the staging archive exists by
        // squatting the final path with a directory (rename onto a directory
        // fails on every platform). Neither staging file may survive.
        let (dir_tmp, db_path) = seed_data_dir("errclean");
        let dir = dir_tmp.path();
        let out = dir.join("blocked.tar.gz");
        fs::create_dir_all(&out).unwrap();

        let result = write_dump(&crate::db::open(&db_path).unwrap(), &db_path, &out);
        assert!(result.is_err(), "rename onto a directory must fail");

        assert!(
            !out.with_extension("archive.tmp").exists(),
            "partial archive staging file must be cleaned on error"
        );
        assert!(
            !out.with_extension("dbsnapshot.tmp").exists(),
            "db snapshot staging file must be cleaned on error"
        );
    }

    #[test]
    fn dumped_db_is_openable_sqlite_with_seeded_data() {
        let (dir_tmp, db_path) = seed_data_dir("snapshot");
        let dir = dir_tmp.path();
        let out = dir.join("snap.tar.gz");
        write_dump(&crate::db::open(&db_path).unwrap(), &db_path, &out).unwrap();

        // Extract the db member and open it.
        let extract_dir = dir.join("extracted");
        fs::create_dir_all(&extract_dir).unwrap();
        let file = fs::File::open(&out).unwrap();
        let dec = flate2::read::GzDecoder::new(file);
        let mut tar = tar::Archive::new(dec);
        for entry in tar.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.path().unwrap().to_string_lossy() == ARCHIVE_DB_NAME {
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();
                fs::write(extract_dir.join("lific.db"), &buf).unwrap();
            }
        }
        let conn = rusqlite::Connection::open(extract_dir.join("lific.db")).unwrap();
        let name: String = conn
            .query_row(
                "SELECT name FROM projects WHERE identifier = 'DMP'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "DumpTest");
    }

    #[test]
    fn run_dump_into_directory_uses_default_filename() {
        let (dir_tmp, db_path) = seed_data_dir("outdir");
        let dir = dir_tmp.path();
        let target = dir.join("dumps");
        fs::create_dir_all(&target).unwrap();
        let res = run_dump(&db_path, Some(&target)).unwrap();
        assert_eq!(res.archive_path.parent().unwrap(), target);
        let fname = res.archive_path.file_name().unwrap().to_string_lossy();
        assert!(fname.starts_with("lific_"));
        assert!(fname.ends_with(".tar.gz"));
        assert!(res.archive_path.exists());
    }

    #[test]
    fn restore_round_trip_matches_entities_and_blob_bytes() {
        let (src_dir_tmp, src_db) = seed_data_dir("rt_src");
        let src_dir = src_dir_tmp.path();
        let out = src_dir.join("backup.tar.gz");
        write_dump(&crate::db::open(&src_db).unwrap(), &src_db, &out).unwrap();

        // Fresh, empty destination dir.
        let dst_dir_tmp = temp_dir("rt_dst");
        let dst_dir = dst_dir_tmp.path();
        let dst_db = dst_dir.join("lific.db");
        let res = run_restore(&out, &dst_db, false).unwrap();
        assert_eq!(res.attachment_count, 2);

        // Entities: the seeded project is present.
        let pool = crate::db::open(&dst_db).unwrap();
        let conn = pool.read().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM projects WHERE identifier = 'DMP'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Blob bytes identical.
        assert_eq!(
            fs::read(dst_dir.join("attachments").join(
                crate::storage::AttachmentStore::hash_bytes(b"blob one"),
            ))
            .unwrap(),
            b"blob one"
        );
        assert_eq!(
            fs::read(dst_dir.join("attachments").join(
                crate::storage::AttachmentStore::hash_bytes(b"second blob bytes"),
            ))
            .unwrap(),
            b"second blob bytes"
        );
    }

    #[test]
    fn restore_refuses_existing_db_without_force() {
        let (src_dir_tmp, src_db) = seed_data_dir("guard_src");
        let src_dir = src_dir_tmp.path();
        let out = src_dir.join("b.tar.gz");
        write_dump(&crate::db::open(&src_db).unwrap(), &src_db, &out).unwrap();

        // Destination already has a db.
        let dst_dir_tmp = temp_dir("guard_dst");
        let dst_dir = dst_dir_tmp.path();
        let dst_db = dst_dir.join("lific.db");
        let _ = crate::db::open(&dst_db).unwrap();

        let err = run_restore(&out, &dst_db, false).unwrap_err();
        assert!(matches!(err, LificError::Conflict(_)), "got {err:?}");
    }

    #[cfg(unix)]
    #[test]
    fn restore_force_moves_existing_db_aside() {
        let (src_dir_tmp, src_db) = seed_data_dir("force_src");
        let src_dir = src_dir_tmp.path();
        let out = src_dir.join("b.tar.gz");
        write_dump(&crate::db::open(&src_db).unwrap(), &src_db, &out).unwrap();

        let dst_dir_tmp = temp_dir("force_dst");
        let dst_dir = dst_dir_tmp.path();
        let dst_db = dst_dir.join("lific.db");
        // Seed a DIFFERENT project so we can tell the old db apart.
        {
            let pool = crate::db::open(&dst_db).unwrap();
            let conn = pool.write().unwrap();
            crate::db::queries::create_project(
                &conn,
                &crate::db::models::CreateProject {
                    name: "OldData".into(),
                    identifier: "OLD".into(),
                    description: String::new(),
                    emoji: None,
                    lead_user_id: None,
                },
            )
            .unwrap();
        }

        let res = run_restore(&out, &dst_db, true).unwrap();
        let moved = res
            .moved_existing_to
            .expect("existing db should be moved aside");
        assert!(moved.exists(), "moved-aside db must still exist");
        assert!(
            moved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("pre-restore-"),
            "moved db name must include pre-restore-: {}",
            moved.display()
        );
        // The moved-aside db still has the OLD project.
        let conn = rusqlite::Connection::open(&moved).unwrap();
        let old: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM projects WHERE identifier='OLD'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(old, 1);
        // The live db now has the restored project.
        let pool = crate::db::open(&dst_db).unwrap();
        let conn = pool.read().unwrap();
        let dmp: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM projects WHERE identifier='DMP'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(dmp, 1);
    }

    #[test]
    fn force_restore_refuses_an_uncheckpointable_database() {
        let (src_dir_tmp, src_db) = seed_data_dir("checkpoint_src");
        let archive = src_dir_tmp.path().join("backup.tar.gz");
        write_dump(&crate::db::open(&src_db).unwrap(), &src_db, &archive).unwrap();

        let destination = temp_dir("checkpoint_dst");
        let db_path = destination.path().join(ARCHIVE_DB_NAME);
        fs::create_dir(&db_path).unwrap();
        let sentinel = db_path.join("must-survive");
        fs::write(&sentinel, b"original state").unwrap();

        let error = run_restore(&archive, &db_path, true).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("open database for WAL checkpoint")
        );
        assert!(db_path.is_dir());
        assert_eq!(fs::read(sentinel).unwrap(), b"original state");
    }

    #[test]
    fn restore_refuses_newer_schema_version() {
        let (src_dir_tmp, src_db) = seed_data_dir("newer_src");
        let src_dir = src_dir_tmp.path();
        let out = src_dir.join("b.tar.gz");
        write_dump(&crate::db::open(&src_db).unwrap(), &src_db, &out).unwrap();

        // Rewrite the archive with a bumped schema_version to simulate a dump
        // from a newer Lific.
        let bumped = src_dir.join("bumped.tar.gz");
        rewrite_archive_with_schema(&out, &bumped, crate::db::migrate::latest_version() + 5);

        let dst_dir_tmp = temp_dir("newer_dst");
        let dst_dir = dst_dir_tmp.path();
        let dst_db = dst_dir.join("lific.db");
        let err = run_restore(&bumped, &dst_db, false).unwrap_err();
        assert!(matches!(err, LificError::BadRequest(_)), "got {err:?}");
        assert!(
            !dst_db.exists(),
            "nothing should be restored on schema refusal"
        );
    }

    #[test]
    fn restore_accepts_pre_attachment_schema_without_attachment_table() {
        let dir_tmp = temp_dir("legacy_schema");
        let dir = dir_tmp.path();
        let legacy_db = dir.join("legacy.db");
        {
            let conn = rusqlite::Connection::open(&legacy_db).unwrap();
            conn.execute_batch(
                "CREATE TABLE _migrations (version INTEGER PRIMARY KEY);
                 INSERT INTO _migrations (version) VALUES (30);",
            )
            .unwrap();
        }

        let archive = dir.join("legacy.tar.gz");
        let manifest = Manifest {
            lific_version: "old".into(),
            schema_version: 30,
            created_at: "now".into(),
            db_size_bytes: fs::metadata(&legacy_db).unwrap().len(),
            attachment_count: 0,
            attachment_bytes: 0,
        };
        let file = fs::File::create(&archive).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        append_bytes(
            &mut builder,
            ARCHIVE_MANIFEST_NAME,
            &serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        builder
            .append_path_with_name(&legacy_db, ARCHIVE_DB_NAME)
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();

        let destination = temp_dir("legacy_schema_dst");
        let db_path = destination.path().join(ARCHIVE_DB_NAME);
        run_restore(&archive, &db_path, false).expect("older empty backups remain restorable");
        assert!(db_path.exists());
        assert!(
            !destination
                .path()
                .join("attachments")
                .join("unexpected")
                .exists()
        );
    }

    #[test]
    fn restore_rejects_path_traversal_entry() {
        let dir_tmp = temp_dir("traversal");
        let dir = dir_tmp.path();
        let archive = dir.join("evil.tar.gz");
        // Craft an archive with a manifest, a db, and a malicious attachment
        // entry that tries to escape the attachments dir.
        {
            let file = fs::File::create(&archive).unwrap();
            let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut tar = tar::Builder::new(enc);
            let manifest = Manifest {
                lific_version: "x".into(),
                schema_version: 1,
                created_at: "now".into(),
                db_size_bytes: 1,
                attachment_count: 1,
                attachment_bytes: 1,
            };
            let mj = serde_json::to_vec(&manifest).unwrap();
            append_bytes(&mut tar, ARCHIVE_MANIFEST_NAME, &mj).unwrap();
            append_bytes(&mut tar, ARCHIVE_DB_NAME, b"not a real db").unwrap();
            // A nested path under attachments/ — tar permits writing it, but a
            // blob name must be a bare hash, so the validator must reject it
            // (this is the class of entry a path-traversal payload uses).
            append_bytes(&mut tar, "attachments/sub/escape", b"pwned").unwrap();
            let enc = tar.into_inner().unwrap();
            enc.finish().unwrap();
        }

        // inspect_archive must reject it.
        let err = inspect_archive(&archive).unwrap_err();
        assert!(matches!(err, LificError::BadRequest(_)), "got {err:?}");

        // And a full restore attempt must also refuse, leaving no db behind.
        let dst_dir_tmp = temp_dir("traversal_dst");
        let dst_dir = dst_dir_tmp.path();
        let dst_db = dst_dir.join("lific.db");
        assert!(run_restore(&archive, &dst_db, false).is_err());
        assert!(!dst_db.exists());
        assert!(
            !dst_dir.join("attachments").join("sub").exists(),
            "traversal entry must not write nested dirs"
        );
    }

    #[cfg(unix)]
    #[test]
    fn restore_mid_extract_failure_leaves_original_untouched() {
        // A corrupt archive (valid manifest+db header claim, truncated body)
        // must fail extraction WITHOUT clobbering the pre-existing db when
        // --force moved it aside — the rollback restores it.
        let (src_dir_tmp, src_db) = seed_data_dir("midfail_src");
        let src_dir = src_dir_tmp.path();
        let good = src_dir.join("good.tar.gz");
        write_dump(&crate::db::open(&src_db).unwrap(), &src_db, &good).unwrap();

        // Truncate the good archive to corrupt it mid-stream.
        let corrupt = src_dir.join("corrupt.tar.gz");
        let bytes = fs::read(&good).unwrap();
        fs::write(&corrupt, &bytes[..bytes.len() / 2]).unwrap();

        let dst_dir_tmp = temp_dir("midfail_dst");
        let dst_dir = dst_dir_tmp.path();
        let dst_db = dst_dir.join("lific.db");
        {
            let pool = crate::db::open(&dst_db).unwrap();
            let conn = pool.write().unwrap();
            crate::db::queries::create_project(
                &conn,
                &crate::db::models::CreateProject {
                    name: "Original".into(),
                    identifier: "ORG".into(),
                    description: String::new(),
                    emoji: None,
                    lead_user_id: None,
                },
            )
            .unwrap();
        }
        // Force restore of the corrupt archive should error but roll back.
        let result = run_restore(&corrupt, &dst_db, true);
        assert!(result.is_err(), "corrupt archive must fail");
        // Original db must still be present and openable with its project.
        assert!(dst_db.exists(), "original db must be restored on rollback");
        let pool = crate::db::open(&dst_db).unwrap();
        let conn = pool.read().unwrap();
        let org: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM projects WHERE identifier='ORG'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(org, 1, "original data must survive a failed restore");
    }

    #[test]
    fn failed_rollback_error_names_both_failures_and_the_surviving_db_path() {
        // LIF-371: when the moved-aside db cannot be renamed back, the user
        // must be told the restore failed AND where their database still is.
        let dir_tmp = temp_dir("rollback_fail");
        let dir = dir_tmp.path();
        let db_path = dir.join("lific.db");
        // A source that does not exist makes the rollback rename fail.
        let moved = dir.join("lific.db.pre-restore-gone");

        let err = rollback_moved_db(
            &moved,
            &db_path,
            LificError::BadRequest("read db from archive: unexpected eof".into()),
        );
        let msg = err.to_string();

        assert!(matches!(err, LificError::Internal(_)), "got {err:?}");
        assert!(
            msg.contains("read db from archive: unexpected eof"),
            "must carry the original failure: {msg}"
        );
        assert!(
            msg.contains(&moved.display().to_string()),
            "must name the path the original db still lives at: {msg}"
        );
        assert!(
            msg.contains(&db_path.display().to_string()),
            "must name where to move it back to: {msg}"
        );
    }

    #[test]
    fn successful_rollback_surfaces_the_original_error_unchanged() {
        let dir_tmp = temp_dir("rollback_ok");
        let dir = dir_tmp.path();
        let db_path = dir.join("lific.db");
        let moved = dir.join("lific.db.pre-restore-1");
        fs::write(&moved, b"original db bytes").unwrap();

        let err = rollback_moved_db(
            &moved,
            &db_path,
            LificError::BadRequest("archive is missing lific.db".into()),
        );

        assert!(matches!(err, LificError::BadRequest(ref m) if m == "archive is missing lific.db"));
        assert_eq!(fs::read(&db_path).unwrap(), b"original db bytes");
        assert!(!moved.exists());
    }

    #[test]
    fn validate_attachment_entry_accepts_bare_hash_rejects_traversal() {
        assert!(
            validate_attachment_entry(
                "attachments/0000000000000000000000000000000000000000000000000000000000000001"
            )
            .is_ok()
        );
        assert!(validate_attachment_entry("attachments/../etc/passwd").is_err());
        assert!(validate_attachment_entry("attachments/sub/dir").is_err());
        assert!(validate_attachment_entry("attachments/").is_err());
        assert!(validate_attachment_entry("notattachments/x").is_err());
        assert!(validate_attachment_entry("attachments/.hidden").is_err());
    }

    #[test]
    fn inspect_rejects_manifest_size_bomb() {
        let (dir_tmp, db_path) = seed_data_dir("manifest_limit");
        let dir = dir_tmp.path();
        let out = dir.join("source.tar.gz");
        write_dump(&crate::db::open(&db_path).unwrap(), &db_path, &out).unwrap();
        let mut manifest = read_manifest(&out).unwrap();
        manifest.attachment_bytes = MAX_TOTAL_RESTORE_BYTES;
        let rewritten = dir.join("oversized.tar.gz");
        rewrite_archive_manifest(&out, &rewritten, &manifest);
        let error = inspect_archive(&rewritten).unwrap_err();
        assert!(error.to_string().contains("total restore size"));
    }

    #[test]
    fn compressed_archive_expansion_is_bounded() {
        const TEST_LIMIT: u64 = 1024 * 1024;
        let dir_tmp = temp_dir("compressed_expansion");
        let archive = dir_tmp.path().join("bomb.tar.gz");
        let expanded_bytes = vec![b'x'; (TEST_LIMIT * 2) as usize];
        let file = fs::File::create(&archive).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::best());
        let mut builder = tar::Builder::new(encoder);
        append_bytes(&mut builder, "payload", &expanded_bytes).unwrap();
        builder.into_inner().unwrap().finish().unwrap();

        assert!(
            fs::metadata(&archive).unwrap().len() < expanded_bytes.len() as u64 / 100,
            "the fixture must be much smaller while expanding past the reader limit"
        );
        let file = fs::File::open(&archive).unwrap();
        let mut archive = bounded_archive_with_limit(file, TEST_LIMIT);
        let mut entries = archive.entries().unwrap().raw(true);
        let mut entry = entries.next().unwrap().unwrap();
        let mut total_bytes = 0;
        let error = copy_entry_bounded(
            &mut entry,
            &mut std::io::sink(),
            MAX_ATTACHMENT_BYTES,
            &mut total_bytes,
            "payload",
        )
        .unwrap_err();
        assert!(error.to_string().contains("expected 2097152 bytes"));
    }

    #[test]
    fn inspect_rejects_tar_extension_entries_before_reading_their_body() {
        let dir_tmp = temp_dir("tar_extension");
        let archive = dir_tmp.path().join("extension.tar.gz");
        let file = fs::File::create(&archive).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::best());
        let mut builder = tar::Builder::new(encoder);
        let extension = vec![b'x'; MAX_MANIFEST_BYTES as usize + 1];
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::XHeader);
        header.set_size(extension.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        builder
            .append_data(&mut header, "pax", extension.as_slice())
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();

        let error = inspect_archive(&archive).unwrap_err();
        assert!(error.to_string().contains("unsupported tar entry type"));
    }

    #[test]
    fn manifest_scan_rejects_excessive_entries_before_manifest() {
        let dir_tmp = temp_dir("manifest_entries");
        let archive = dir_tmp.path().join("too-many.tar.gz");
        let file = fs::File::create(&archive).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(enc);
        for index in 0..(MAX_RESTORE_ENTRIES + 3) {
            append_bytes(&mut builder, &format!("ignored-{index}"), &[0]).unwrap();
        }
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();
        let error = inspect_archive(&archive).unwrap_err();
        assert!(error.to_string().contains("before its manifest"));
    }

    #[test]
    fn inspect_rejects_duplicate_control_entries() {
        let dir_tmp = temp_dir("duplicate_entries");
        let archive = dir_tmp.path().join("duplicate.tar.gz");
        let file = fs::File::create(&archive).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(enc);
        let manifest = Manifest {
            lific_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: 1,
            created_at: String::new(),
            db_size_bytes: 1,
            attachment_count: 0,
            attachment_bytes: 0,
        };
        let bytes = serde_json::to_vec(&manifest).unwrap();
        append_bytes(&mut builder, ARCHIVE_MANIFEST_NAME, &bytes).unwrap();
        append_bytes(&mut builder, ARCHIVE_MANIFEST_NAME, &bytes).unwrap();
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();
        let error = inspect_archive(&archive).unwrap_err();
        assert!(error.to_string().contains("duplicate manifest"));
    }

    #[test]
    fn restore_rejects_manifest_schema_that_does_not_match_database() {
        let (dir_tmp, db_path) = seed_data_dir("schema_mismatch");
        let archive = dir_tmp.path().join("source.tar.gz");
        write_dump(&crate::db::open(&db_path).unwrap(), &db_path, &archive).unwrap();
        let rewritten = dir_tmp.path().join("mismatch.tar.gz");
        rewrite_archive_with_schema(
            &archive,
            &rewritten,
            crate::db::migrate::latest_version() - 1,
        );

        let destination = temp_dir("schema_mismatch_dst");
        let error =
            run_restore(&rewritten, &destination.path().join(ARCHIVE_DB_NAME), false).unwrap_err();
        assert!(error.to_string().contains("does not match manifest"));
    }

    #[test]
    fn staged_database_rejects_unconstrained_attachment_schema() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE attachments (
                sha256 TEXT NOT NULL,
                filename TEXT NOT NULL,
                mime TEXT NOT NULL /* mime TEXT NOT NULL CHECK (mime IN ( */,
                size_bytes INTEGER NOT NULL /* size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0) */
                /* CHECK (length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*') */
            )",
        )
        .unwrap();

        let error = validate_attachment_schema(&conn).unwrap_err();
        assert!(error.to_string().contains("integrity constraints"));
    }

    #[test]
    fn staged_database_rejects_blob_content_hash_mismatch() {
        let (dir_tmp, db_path) = seed_data_dir("content_hash");
        let dir = dir_tmp.path();
        let pool = crate::db::open(&db_path).unwrap();
        let sha = crate::storage::AttachmentStore::hash_bytes(b"good");
        {
            let conn = pool.write().unwrap();
            crate::db::queries::attachments::create_attachment(
                &conn,
                &sha,
                "note.txt",
                "text/plain",
                4,
                None,
            )
            .unwrap();
        }
        checkpoint_db_file(&db_path).unwrap();
        let staging = dir.join("staging");
        fs::create_dir_all(staging.join("attachments")).unwrap();
        fs::copy(&db_path, staging.join(ARCHIVE_DB_NAME)).unwrap();
        fs::write(staging.join("attachments").join(&sha), b"evil").unwrap();
        let manifest = Manifest {
            lific_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: crate::db::migrate::latest_version(),
            created_at: String::new(),
            db_size_bytes: fs::metadata(&db_path).unwrap().len(),
            attachment_count: 1,
            attachment_bytes: 4,
        };

        let error = validate_staged_database(&staging, &manifest).unwrap_err();
        assert!(error.to_string().contains("content address"));
    }

    #[test]
    fn staged_database_rejects_blob_content_mime_mismatch() {
        let (dir_tmp, db_path) = seed_data_dir("content_mime");
        let dir = dir_tmp.path();
        let pool = crate::db::open(&db_path).unwrap();
        let sha = crate::storage::AttachmentStore::hash_bytes(b"good");
        {
            let conn = pool.write().unwrap();
            crate::db::queries::attachments::create_attachment(
                &conn,
                &sha,
                "image.png",
                "image/png",
                4,
                None,
            )
            .unwrap();
        }
        checkpoint_db_file(&db_path).unwrap();
        let staging = dir.join("staging");
        fs::create_dir_all(staging.join("attachments")).unwrap();
        fs::copy(&db_path, staging.join(ARCHIVE_DB_NAME)).unwrap();
        fs::write(staging.join("attachments").join(&sha), b"good").unwrap();
        let manifest = Manifest {
            lific_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: crate::db::migrate::latest_version(),
            created_at: String::new(),
            db_size_bytes: fs::metadata(&db_path).unwrap().len(),
            attachment_count: 1,
            attachment_bytes: 4,
        };

        let error = validate_staged_database(&staging, &manifest).unwrap_err();
        assert!(error.to_string().contains("MIME image/png"));
    }

    #[test]
    fn staged_database_rejects_duplicate_hashes_with_inconsistent_sizes() {
        let (dir_tmp, db_path) = seed_data_dir("duplicate_sizes");
        let dir = dir_tmp.path();
        let pool = crate::db::open(&db_path).unwrap();
        let sha = crate::storage::AttachmentStore::hash_bytes(b"good");
        {
            let conn = pool.write().unwrap();
            for (filename, size) in [("one.txt", 4), ("two.txt", 5)] {
                crate::db::queries::attachments::create_attachment(
                    &conn,
                    &sha,
                    filename,
                    "text/plain",
                    size,
                    None,
                )
                .unwrap();
            }
        }
        checkpoint_db_file(&db_path).unwrap();
        let staging = dir.join("staging");
        fs::create_dir_all(staging.join("attachments")).unwrap();
        fs::copy(&db_path, staging.join(ARCHIVE_DB_NAME)).unwrap();
        fs::write(staging.join("attachments").join(&sha), b"good").unwrap();
        let manifest = Manifest {
            lific_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: crate::db::migrate::latest_version(),
            created_at: String::new(),
            db_size_bytes: fs::metadata(&db_path).unwrap().len(),
            attachment_count: 1,
            attachment_bytes: 4,
        };

        let error = validate_staged_database(&staging, &manifest).unwrap_err();
        assert!(error
            .to_string()
            .contains("invalid content address, MIME, or size"));
    }

    #[test]
    fn staged_database_rejects_unreferenced_blob_content_hash_mismatch() {
        let (dir_tmp, db_path) = seed_data_dir("unreferenced_content_hash");
        let dir = dir_tmp.path();
        let pool = crate::db::open(&db_path).unwrap();
        checkpoint_db_file(&db_path).unwrap();
        drop(pool);

        let staging = dir.join("staging");
        fs::create_dir_all(staging.join("attachments")).unwrap();
        fs::copy(&db_path, staging.join(ARCHIVE_DB_NAME)).unwrap();
        let sha = crate::storage::AttachmentStore::hash_bytes(b"good");
        fs::write(staging.join("attachments").join(&sha), b"evil").unwrap();
        let manifest = Manifest {
            lific_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: crate::db::migrate::latest_version(),
            created_at: String::new(),
            db_size_bytes: fs::metadata(&db_path).unwrap().len(),
            attachment_count: 1,
            attachment_bytes: 4,
        };

        let error = validate_staged_database(&staging, &manifest).unwrap_err();
        assert!(error.to_string().contains("content address"));
    }

    #[test]
    fn staged_database_requires_attachment_table() {
        let (dir_tmp, db_path) = seed_data_dir("missing_attachments_table");
        let dir = dir_tmp.path();
        checkpoint_db_file(&db_path).unwrap();
        let staging = dir.join("staging");
        fs::create_dir_all(staging.join("attachments")).unwrap();
        fs::copy(&db_path, staging.join(ARCHIVE_DB_NAME)).unwrap();
        {
            let conn = rusqlite::Connection::open(staging.join(ARCHIVE_DB_NAME)).unwrap();
            conn.execute_batch(
                "DROP TRIGGER IF EXISTS attachments_fts_ai;
                 DROP TRIGGER IF EXISTS attachments_fts_au;
                 DROP TRIGGER IF EXISTS attachments_fts_ad;
                 DROP TABLE attachments;",
            )
            .unwrap();
        }
        let manifest = Manifest {
            lific_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: crate::db::migrate::latest_version(),
            created_at: String::new(),
            db_size_bytes: 0,
            attachment_count: 0,
            attachment_bytes: 0,
        };

        let error = validate_staged_database(&staging, &manifest).unwrap_err();
        assert!(error.to_string().contains("attachments table"));
    }

    #[test]
    fn staged_database_rejects_missing_or_mismatched_attachment_metadata() {
        let (dir_tmp, db_path) = seed_data_dir("staged_metadata");
        let dir = dir_tmp.path();
        let pool = crate::db::open(&db_path).unwrap();
        let sha = "a".repeat(64);
        {
            let conn = pool.write().unwrap();
            crate::db::queries::attachments::create_attachment(
                &conn,
                &sha,
                "note.txt",
                "text/plain",
                4,
                None,
            )
            .unwrap();
        }
        checkpoint_db_file(&db_path).unwrap();
        let staging = dir.join("staging");
        fs::create_dir_all(staging.join("attachments")).unwrap();
        fs::copy(&db_path, staging.join(ARCHIVE_DB_NAME)).unwrap();
        let manifest = Manifest {
            lific_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: crate::db::migrate::latest_version(),
            created_at: String::new(),
            db_size_bytes: fs::metadata(&db_path).unwrap().len(),
            attachment_count: 1,
            attachment_bytes: 4,
        };
        let error = validate_staged_database(&staging, &manifest).unwrap_err();
        assert!(error.to_string().contains("missing attachment"));
    }

    // Test helper: re-pack an archive but overwrite the manifest's
    // schema_version, to simulate an archive from a newer binary.
    fn rewrite_archive_with_schema(src: &Path, dst: &Path, schema_version: i64) {
        let mut manifest = read_manifest(src).unwrap();
        manifest.schema_version = schema_version;
        rewrite_archive_manifest(src, dst, &manifest);
    }

    fn rewrite_archive_manifest(src: &Path, dst: &Path, manifest: &Manifest) {
        let mj = serde_json::to_vec_pretty(&manifest).unwrap();

        let out = fs::File::create(dst).unwrap();
        let enc = flate2::write::GzEncoder::new(out, flate2::Compression::default());
        let mut builder = tar::Builder::new(enc);
        append_bytes(&mut builder, ARCHIVE_MANIFEST_NAME, &mj).unwrap();

        // Copy the db + attachments through unchanged.
        let file = fs::File::open(src).unwrap();
        let dec = flate2::read::GzDecoder::new(file);
        let mut tar = tar::Archive::new(dec);
        for entry in tar.entries().unwrap() {
            let mut entry = entry.unwrap();
            let name = entry.path().unwrap().to_string_lossy().to_string();
            if name == ARCHIVE_MANIFEST_NAME {
                continue;
            }
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();
            append_bytes(&mut builder, &name, &buf).unwrap();
        }
        let enc = builder.into_inner().unwrap();
        enc.finish().unwrap();
    }
}
