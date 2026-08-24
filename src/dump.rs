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

#[cfg(unix)]
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use fs2::FileExt;

use crate::db::DbPool;
use crate::error::LificError;

/// The DB filename inside every archive, independent of the on-disk name.
pub const ARCHIVE_DB_NAME: &str = "lific.db";
/// The manifest filename inside every archive.
pub const ARCHIVE_MANIFEST_NAME: &str = "manifest.json";
/// The prefix under which attachment blobs are stored inside the archive.
pub const ARCHIVE_ATTACHMENTS_PREFIX: &str = "attachments/";

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
/// The SQLite online backup API runs on a read connection, holds no long
/// writer lock, and snapshots into the already-reserved staging file — safe
/// while the server is running.
fn snapshot_db(pool: &DbPool, dest: &TempFile) -> Result<(), LificError> {
    let conn = pool.read()?;
    let mut destination = rusqlite::Connection::open_with_flags(
        dest.path(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
            | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|e| LificError::Internal(format!("open SQLite staging file: {e}")))?;
    let backup = rusqlite::backup::Backup::new(&conn, &mut destination)
        .map_err(|e| LificError::Internal(format!("create SQLite backup: {e}")))?;
    backup
        .run_to_completion(100, std::time::Duration::ZERO, None)
        .map_err(|e| LificError::Internal(format!("SQLite backup failed: {e}")))?;
    Ok(())
}

/// Set 0600 permissions on an open file (owner-only) on Unix. No-op elsewhere.
fn set_owner_only(file: &File) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        let _ = file;
    }
    Ok(())
}

struct TempFile {
    file: File,
    path: PathBuf,
}

struct StagingLock {
    _file: File,
}

impl StagingLock {
    fn create(path: &Path) -> Result<Self, LificError> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|error| LificError::Internal(format!("create dump lock: {error}")))?;
        file.try_lock_exclusive()
            .map_err(|error| LificError::Internal(format!("lock dump staging: {error}")))?;
        Ok(Self { _file: file })
    }
}

/// Return whether another process currently owns a dump staging lock.
pub(crate) fn staging_is_locked(path: &Path) -> bool {
    let lock = path.join("lock");
    let Ok(file) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock)
    else {
        return true;
    };
    match file.try_lock_exclusive() {
        Ok(()) => {
            false
        }
        Err(_) => true,
    }
}

fn refresh_activity(file: &File) -> std::io::Result<()> {
    file.set_modified(std::time::SystemTime::now())
}

impl TempFile {
    fn create(path: PathBuf) -> Result<Self, LificError> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(&path).map_err(|error| {
            LificError::Internal(format!("create secure staging file: {error}"))
        })?;
        Ok(Self { file, path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
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
    let parent = out_path.parent().unwrap_or_else(|| Path::new("."));
    validate_dump_destination(out_path)?;
    let staging = tempfile::Builder::new()
        .prefix(".lific-dump-")
        .tempdir_in(parent)
        .map_err(|e| LificError::Internal(format!("create dump staging directory: {e}")))?;
    let _lock = StagingLock::create(&staging.path().join("lock"))?;
    let activity = TempFile::create(staging.path().join("activity"))?;
    refresh_activity(&activity.file)
        .map_err(|e| LificError::Internal(format!("mark dump staging active: {e}")))?;
    // The private staging directory prevents another user of a shared output
    // directory from swapping either pathname while its open handle is live.
    let tmp_db = TempFile::create(staging.path().join("dbsnapshot"))?;
    snapshot_db(pool, &tmp_db)?;
    refresh_activity(&activity.file)
        .map_err(|e| LificError::Internal(format!("refresh dump staging activity: {e}")))?;

    let tmp_archive = TempFile::create(staging.path().join("archive"))?;

    (|| {
        let db_size_bytes = std::fs::metadata(tmp_db.path())
            .map(|m| m.len())
            .unwrap_or(0);

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
                if !entry
                    .file_type()
                    .map_err(|e| LificError::Internal(format!("inspect attachment entry: {e}")))?
                    .is_file()
                {
                    continue;
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if entry
                        .metadata()
                        .map_err(|e| {
                            LificError::Internal(format!("inspect attachment metadata: {e}"))
                        })?
                        .nlink()
                        != 1
                    {
                        return Err(LificError::Internal(format!(
                            "attachment entry is hard-linked: {}",
                            path.display()
                        )));
                    }
                }
                if path.extension().and_then(|e| e.to_str()) == Some("tmp") {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                validate_attachment_entry(&format!("{ARCHIVE_ATTACHMENTS_PREFIX}{name}"))
                    .map_err(|_| {
                        LificError::Internal(format!(
                            "invalid attachment filename in store: {name}"
                        ))
                    })?;
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
            let file = tmp_archive
                .file
                .try_clone()
                .map_err(|e| LificError::Internal(format!("open archive staging handle: {e}")))?;
            let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut tar = tar::Builder::new(enc);

            // manifest.json
            append_bytes(&mut tar, ARCHIVE_MANIFEST_NAME, &manifest_json)?;
            // lific.db (from the snapshot file)
            tar.append_path_with_name(tmp_db.path(), ARCHIVE_DB_NAME)
                .map_err(|e| LificError::Internal(format!("append db to archive: {e}")))?;
            // attachments/<sha256>
            for (name, path, _size) in &blobs {
                refresh_activity(&activity.file).map_err(|e| {
                    LificError::Internal(format!("refresh dump staging activity: {e}"))
                })?;
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

        tmp_archive
            .file
            .sync_all()
            .map_err(|e| LificError::Internal(format!("sync archive: {e}")))?;
        set_owner_only(&tmp_archive.file)
            .map_err(|e| LificError::Internal(format!("chmod archive: {e}")))?;
        std::fs::rename(tmp_archive.path(), out_path)
            .map_err(|e| LificError::Internal(format!("finalize archive: {e}")))?;

        Ok(manifest)
    })()
}

fn validate_dump_destination(out_path: &Path) -> Result<(), LificError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let parent = out_path.parent().unwrap_or_else(|| Path::new("."));
        let parent_metadata = std::fs::symlink_metadata(parent)
            .map_err(|e| LificError::Internal(format!("inspect dump destination: {e}")))?;
        if parent_metadata.file_type().is_symlink() {
            return Err(LificError::Internal("dump destination parent is a symlink".into()));
        }
        let mode = parent_metadata.mode();
        if mode & 0o1000 == 0 && mode & 0o022 != 0 {
            return Err(LificError::Internal(
                "dump destination parent is group/world-writable without sticky protection".into(),
            ));
        }
        if let Ok(metadata) = std::fs::symlink_metadata(out_path)
            && (metadata.file_type().is_symlink() || metadata.nlink() > 1)
        {
            return Err(LificError::Internal(
                "dump destination must not be a symlink or hard link".into(),
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = out_path;
    Ok(())
}

fn write_owner_only(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path)?;
    set_owner_only(&file)?;
    file.write_all(bytes)?;
    Ok(())
}

fn set_owner_only_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
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
    if rest.len() != 64
        || rest.contains('/')
        || rest.contains('\\')
        || rest.contains("..")
        || rest.starts_with('.')
        || !rest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(LificError::BadRequest(format!(
            "rejected attachment entry (path traversal or invalid name): {name}"
        )));
    }
    Ok(rest.to_string())
}

/// Read and validate an archive's manifest + entry list without extracting.
/// Returns the parsed manifest. Rejects archives missing `manifest.json` or
/// `lific.db`, and any attachment entry that fails [`validate_attachment_entry`].
pub fn inspect_archive(archive: &Path) -> Result<Manifest, LificError> {
    let manifest = read_manifest(archive)?;

    // Second pass: validate every entry name (traversal guard) and require the
    // DB member is present.
    let file = std::fs::File::open(archive)
        .map_err(|e| LificError::BadRequest(format!("open archive: {e}")))?;
    let dec = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(dec);
    let mut has_db = false;
    for entry in tar
        .entries()
        .map_err(|e| LificError::BadRequest(format!("read archive entries: {e}")))?
    {
        let entry = entry.map_err(|e| LificError::BadRequest(format!("read entry: {e}")))?;
        let path = entry
            .path()
            .map_err(|e| LificError::BadRequest(format!("entry path: {e}")))?;
        let name = path.to_string_lossy().replace('\\', "/");
        if name == ARCHIVE_MANIFEST_NAME {
            continue;
        } else if name == ARCHIVE_DB_NAME {
            has_db = true;
        } else if name.starts_with(ARCHIVE_ATTACHMENTS_PREFIX) {
            validate_attachment_entry(&name)?;
        } else {
            return Err(LificError::BadRequest(format!(
                "rejected unexpected archive entry: {name}"
            )));
        }
    }
    if !has_db {
        return Err(LificError::BadRequest("archive is missing lific.db".into()));
    }
    Ok(manifest)
}

/// Read just the manifest from the archive (first matching entry).
fn read_manifest(archive: &Path) -> Result<Manifest, LificError> {
    let file = std::fs::File::open(archive)
        .map_err(|e| LificError::BadRequest(format!("open archive: {e}")))?;
    let dec = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(dec);
    for entry in tar
        .entries()
        .map_err(|e| LificError::BadRequest(format!("read archive entries: {e}")))?
    {
        let mut entry = entry.map_err(|e| LificError::BadRequest(format!("read entry: {e}")))?;
        let path = entry
            .path()
            .map_err(|e| LificError::BadRequest(format!("entry path: {e}")))?;
        if path.to_string_lossy() == ARCHIVE_MANIFEST_NAME {
            let mut buf = String::new();
            entry
                .read_to_string(&mut buf)
                .map_err(|e| LificError::BadRequest(format!("read manifest: {e}")))?;
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
    #[cfg(unix)]
    let data_dir_existed = data_dir.exists();
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| LificError::Internal(format!("create data dir: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let mode = fs::metadata(&data_dir)
            .map_err(|e| LificError::Internal(format!("inspect data dir: {e}")))?
            .mode()
            & 0o777;
        if data_dir_existed && mode & 0o022 != 0 {
            return Err(LificError::Internal(format!(
                "data directory {} is writable by group/others; remove group/other write permissions before restoring",
                data_dir.display()
            )));
        }
        if !data_dir_existed {
            fs::set_permissions(&data_dir, fs::Permissions::from_mode(0o700))
                .map_err(|e| LificError::Internal(format!("secure data dir: {e}")))?;
        }
    }

    // Staged extraction: unpack into a temp dir next to the data dir, then move
    // into place. A failure mid-extract leaves the original data dir untouched.
    let staging = tempfile::Builder::new()
        .prefix(".lific-restore-")
        .tempdir_in(&data_dir)
        .map_err(|e| LificError::Internal(format!("create staging dir: {e}")))?;
    std::fs::create_dir_all(staging.path().join("attachments"))
        .map_err(|e| LificError::Internal(format!("create staging dir: {e}")))?;
    set_owner_only_dir(staging.path())
        .map_err(|e| LificError::Internal(format!("secure restore staging dir: {e}")))?;
    set_owner_only_dir(&staging.path().join("attachments"))
        .map_err(|e| LificError::Internal(format!("secure restore attachments dir: {e}")))?;

    let mut moved_existing_to: Option<PathBuf> = None;
    let extract = (|| -> Result<u64, LificError> {
        let file = std::fs::File::open(archive)
            .map_err(|e| LificError::BadRequest(format!("open archive: {e}")))?;
        let dec = flate2::read::GzDecoder::new(file);
        let mut tar = tar::Archive::new(dec);
        let mut attachment_count = 0u64;
        for entry in tar
            .entries()
            .map_err(|e| LificError::BadRequest(format!("read archive entries: {e}")))?
        {
            let mut entry =
                entry.map_err(|e| LificError::BadRequest(format!("read entry: {e}")))?;
            let epath = entry
                .path()
                .map_err(|e| LificError::BadRequest(format!("entry path: {e}")))?;
            let name = epath.to_string_lossy().replace('\\', "/");
            if name == ARCHIVE_MANIFEST_NAME {
                // Persist the manifest alongside the restored DB for provenance.
                let mut buf = Vec::new();
                entry
                    .read_to_end(&mut buf)
                    .map_err(|e| LificError::BadRequest(format!("read manifest: {e}")))?;
                write_owner_only(&staging.path().join(ARCHIVE_MANIFEST_NAME), &buf)
                    .map_err(|e| LificError::Internal(format!("write manifest: {e}")))?;
            } else if name == ARCHIVE_DB_NAME {
                let mut buf = Vec::new();
                entry
                    .read_to_end(&mut buf)
                    .map_err(|e| LificError::BadRequest(format!("read db from archive: {e}")))?;
                write_owner_only(&staging.path().join(ARCHIVE_DB_NAME), &buf)
                    .map_err(|e| LificError::Internal(format!("write db: {e}")))?;
            } else if name.starts_with(ARCHIVE_ATTACHMENTS_PREFIX) {
                let bare = validate_attachment_entry(&name)?;
                let mut buf = Vec::new();
                entry
                    .read_to_end(&mut buf)
                    .map_err(|e| LificError::BadRequest(format!("read attachment: {e}")))?;
                write_owner_only(&staging.path().join("attachments").join(&bare), &buf)
                    .map_err(|e| LificError::Internal(format!("write attachment: {e}")))?;
                attachment_count += 1;
            } else {
                return Err(LificError::BadRequest(format!(
                    "rejected unexpected archive entry: {name}"
                )));
            }
        }
        if !staging.path().join(ARCHIVE_DB_NAME).exists() {
            return Err(LificError::BadRequest("archive is missing lific.db".into()));
        }
        Ok(attachment_count)
    })();

    let attachment_count = match extract {
        Ok(n) => n,
        Err(e) => {
            // Roll back: discard staging; restore the moved-aside DB if any.
            // The moved db is self-contained (WAL was checkpointed before the
            // move), so a bare rename back is enough.
            return Err(match &moved_existing_to {
                Some(moved) => rollback_moved_db(moved, db_path, e),
                None => e,
            });
        }
    };

    // Existing-DB guard. Prepare and validate the complete staging tree before
    // moving the live database, so a staging failure cannot leave db_path
    // absent.
    if db_path.exists() {
        if !force {
            return Err(LificError::Conflict(format!(
                "{} already exists; pass --force to restore over it (stop the server first)",
                db_path.display()
            )));
        }
        checkpoint_db_file(db_path);
        let suffix = format!("pre-restore-{}", archive_timestamp());
        let dest = PathBuf::from(format!("{}.{suffix}", db_path.display()));
        std::fs::rename(db_path, &dest)
            .map_err(|e| LificError::Internal(format!("move existing db aside: {e}")))?;
        for ext in ["-wal", "-shm"] {
            let side = PathBuf::from(format!("{}{ext}", db_path.display()));
            let _ = std::fs::remove_file(&side);
        }
        moved_existing_to = Some(dest);
    }

    // Move restored files into place. Keep the old attachments directory until
    // the new one is installed so a later failure can restore both data sets.
    let attachments_dest = attachments_dir_for(db_path);
    let attachments_backup = if attachments_dest.exists() {
        let backup = data_dir.join(format!(".lific-restore-attachments-{}", archive_timestamp()));
        if let Err(error) = std::fs::rename(&attachments_dest, &backup) {
            let cause = LificError::Internal(format!("stage existing attachments: {error}"));
            return Err(match &moved_existing_to {
                Some(moved) => rollback_moved_db(moved, db_path, cause),
                None => cause,
            });
        }
        Some(backup)
    } else {
        None
    };
    let manifest_dest = db_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join(ARCHIVE_MANIFEST_NAME);
    let manifest_backup = if manifest_dest.exists() {
        let backup = data_dir.join(format!(".lific-restore-manifest-{}", archive_timestamp()));
        if let Err(error) = std::fs::rename(&manifest_dest, &backup) {
            let cause = LificError::Internal(format!("stage existing manifest: {error}"));
            if let Some(backup) = &attachments_backup {
                let _ = std::fs::rename(backup, &attachments_dest);
            }
            return Err(match &moved_existing_to {
                Some(moved) => rollback_moved_db(moved, db_path, cause),
                None => cause,
            });
        }
        Some(backup)
    } else {
        None
    };
    let install = (|| -> Result<(), LificError> {
        std::fs::rename(staging.path().join(ARCHIVE_DB_NAME), db_path)
            .map_err(|e| LificError::Internal(format!("install restored db: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(db_path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| LificError::Internal(format!("chmod restored db: {e}")))?;
        }
        std::fs::rename(staging.path().join("attachments"), &attachments_dest)
            .map_err(|e| LificError::Internal(format!("install restored attachments: {e}")))?;
        std::fs::rename(
            staging.path().join(ARCHIVE_MANIFEST_NAME),
            &manifest_dest,
        )
        .map_err(|e| LificError::Internal(format!("install restored manifest: {e}")))?;
        Ok(())
    })();
    if let Err(error) = install {
        if db_path.exists() {
            let _ = std::fs::remove_file(db_path);
        }
        if let Some(backup) = &attachments_backup {
            let _ = std::fs::remove_dir_all(&attachments_dest);
            let _ = std::fs::rename(backup, &attachments_dest);
        }
        if let Some(backup) = &manifest_backup {
            let _ = std::fs::remove_file(&manifest_dest);
            let _ = std::fs::rename(backup, &manifest_dest);
        }
        return Err(match &moved_existing_to {
            Some(moved) => rollback_moved_db(moved, db_path, error),
            None => error,
        });
    }
    if let Some(backup) = attachments_backup {
        let _ = std::fs::remove_dir_all(backup);
    }
    if let Some(backup) = manifest_backup {
        let _ = std::fs::remove_file(backup);
    }

    Ok(RestoreResult {
        manifest,
        attachment_count,
        db_path: db_path.to_path_buf(),
        moved_existing_to,
    })
}

/// Put the database that `--force` moved aside back at `db_path` after a failed
/// restore, and decide which error the caller should surface.
///
/// On success the original failure (`cause`) is returned unchanged. If the
/// rollback rename itself fails, swallowing it would tell the user only that
/// the restore failed while their database sits at a path they were never
/// shown, reading exactly like data loss (LIF-371). So that case returns a
/// combined error carrying both failures and the exact path the original
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
/// Best-effort: opening or checkpointing failure is ignored (nothing on disk
/// changes for the worse).
fn checkpoint_db_file(db_path: &Path) {
    if let Ok(conn) = rusqlite::Connection::open(db_path) {
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }
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
        let dir = tempfile::Builder::new()
            .prefix(&format!("lific_dump_{tag}_"))
            .tempdir()
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        }
        dir
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
        fs::write(
            att.join("b9b3a769cea97e51493b4e72ee90ba48282936fadabbb8149bf8fa6d54b873c8"),
            b"blob one",
        )
        .unwrap();
        fs::write(
            att.join("ea2566faf1d1b369882675745c14fdca057e281ec2e198da480c8c3e2d95dcf0"),
            b"second blob bytes",
        )
        .unwrap();
        fs::write(
            att.join("b9b3a769cea97e51493b4e72ee90ba48282936fadabbb8149bf8fa6d54b873c8.tmp"),
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
        assert!(
            entries.contains(
                &"attachments/b9b3a769cea97e51493b4e72ee90ba48282936fadabbb8149bf8fa6d54b873c8"
                    .to_string()
            )
        );
        assert!(
            entries.contains(
                &"attachments/ea2566faf1d1b369882675745c14fdca057e281ec2e198da480c8c3e2d95dcf0"
                    .to_string()
            )
        );
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

    #[cfg(unix)]
    #[test]
    fn dump_rejects_unsafe_destinations() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let (dir_tmp, db_path) = seed_data_dir("unsafe-destinations");
        let dir = dir_tmp.path();
        let target = dir.join("target.tar.gz");
        fs::write(&target, b"existing").unwrap();
        let link = dir.join("link.tar.gz");
        symlink(&target, &link).unwrap();

        assert!(write_dump(&crate::db::open(&db_path).unwrap(), &db_path, &link).is_err());

        let hardlink = dir.join("hardlink.tar.gz");
        fs::hard_link(&target, &hardlink).unwrap();
        assert!(write_dump(&crate::db::open(&db_path).unwrap(), &db_path, &hardlink).is_err());

        let safe_parent = dir.join("safe-parent");
        fs::create_dir(&safe_parent).unwrap();
        fs::set_permissions(&safe_parent, fs::Permissions::from_mode(0o755)).unwrap();
        let safe_output = safe_parent.join("out.tar.gz");
        assert!(write_dump(&crate::db::open(&db_path).unwrap(), &db_path, &safe_output).is_ok());

        let real_parent = dir.join("real-parent");
        fs::create_dir(&real_parent).unwrap();
        let parent_link = dir.join("parent-link");
        symlink(&real_parent, &parent_link).unwrap();
        let nested = parent_link.join("nested.tar.gz");
        assert!(write_dump(&crate::db::open(&db_path).unwrap(), &db_path, &nested).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn dump_rejects_symlink_and_hard_linked_attachments() {
        use std::os::unix::fs::symlink;

        let (dir_tmp, db_path) = seed_data_dir("unsafe-attachments");
        let dir = dir_tmp.path();
        let attachments = dir.join("attachments");
        let name = "c".repeat(64);
        let entry = attachments.join(&name);
        let outside = dir.join("outside");
        fs::write(&outside, b"outside").unwrap();
        symlink(&outside, &entry).unwrap();

        let out = dir.join("symlink.tar.gz");
        write_dump(&crate::db::open(&db_path).unwrap(), &db_path, &out).unwrap();
        assert!(!archive_entries(&out).contains(&format!("attachments/{name}")));

        fs::remove_file(&entry).unwrap();
        fs::hard_link(&outside, &entry).unwrap();
        let out = dir.join("hardlink.tar.gz");
        assert!(write_dump(&crate::db::open(&db_path).unwrap(), &db_path, &out).is_err());
    }

    #[test]
    fn failed_dump_cleans_up_its_staging_files() {
        // Force a failure after the staging archive exists by squatting the
        // final path with a directory. The private staging directory must not
        // survive the failed rename.
        let (dir_tmp, db_path) = seed_data_dir("errclean");
        let dir = dir_tmp.path();
        let out = dir.join("blocked.tar.gz");
        fs::create_dir_all(&out).unwrap();

        let result = write_dump(&crate::db::open(&db_path).unwrap(), &db_path, &out);
        assert!(result.is_err(), "rename onto a directory must fail");

        assert!(!fs::read_dir(dir)
            .unwrap()
            .any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with(".lific-dump-")));
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
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
        }
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
        assert!(dst_dir.join(ARCHIVE_MANIFEST_NAME).is_file());

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
            fs::read(
                dst_dir
                    .join("attachments")
                    .join("b9b3a769cea97e51493b4e72ee90ba48282936fadabbb8149bf8fa6d54b873c8")
            )
            .unwrap(),
            b"blob one"
        );
        assert_eq!(
            fs::read(
                dst_dir
                    .join("attachments")
                    .join("ea2566faf1d1b369882675745c14fdca057e281ec2e198da480c8c3e2d95dcf0")
            )
            .unwrap(),
            b"second blob bytes"
        );
    }

    #[cfg(unix)]
    #[test]
    fn restore_protects_data_and_attachment_paths() {
        use std::os::unix::fs::PermissionsExt;

        let (src_dir_tmp, src_db) = seed_data_dir("perms_src");
        let out = src_dir_tmp.path().join("backup.tar.gz");
        write_dump(&crate::db::open(&src_db).unwrap(), &src_db, &out).unwrap();

        let dst_dir_tmp = temp_dir("perms_dst");
        let dst_dir = dst_dir_tmp.path();
        let dst_db = dst_dir.join("lific.db");
        run_restore(&out, &dst_db, false).unwrap();

        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(dst_dir), 0o700);
        assert_eq!(mode(&dst_dir.join("attachments")), 0o700);
        assert_eq!(mode(&dst_db), 0o600);
        assert_eq!(
            mode(&dst_dir.join(
                "attachments/b9b3a769cea97e51493b4e72ee90ba48282936fadabbb8149bf8fa6d54b873c8"
            )),
            0o600
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
                "attachments/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            )
            .is_ok()
        );
        assert!(validate_attachment_entry("attachments/abc123def").is_err());
        assert!(validate_attachment_entry("attachments/../etc/passwd").is_err());
        assert!(validate_attachment_entry("attachments/sub/dir").is_err());
        assert!(validate_attachment_entry("attachments/").is_err());
        assert!(validate_attachment_entry("notattachments/x").is_err());
        assert!(validate_attachment_entry("attachments/.hidden").is_err());
    }

    // Test helper: re-pack an archive but overwrite the manifest's
    // schema_version, to simulate an archive from a newer binary.
    fn rewrite_archive_with_schema(src: &Path, dst: &Path, schema_version: i64) {
        let mut manifest = read_manifest(src).unwrap();
        manifest.schema_version = schema_version;
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
