use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tracing::{error, info, warn};

use crate::config::BackupConfig;
use crate::db::DbPool;
use crate::dump;

/// Start the background backup task. Returns the JoinHandle.
pub fn start_backup_task(
    pool: Arc<DbPool>,
    db_path: PathBuf,
    config: BackupConfig,
) -> tokio::task::JoinHandle<()> {
    let backup_dir = if config.dir.is_absolute() {
        config.dir.clone()
    } else if let Some(parent) = db_path.parent() {
        parent.join(&config.dir)
    } else {
        config.dir.clone()
    };

    let interval = Duration::from_secs(config.interval_minutes * 60);
    let retain = config.retain;

    tokio::spawn(async move {
        if let Err(e) = std::fs::create_dir_all(&backup_dir) {
            error!(dir = %backup_dir.display(), error = %e, "failed to create backup directory");
            return;
        }

        info!(
            dir = %backup_dir.display(),
            interval_min = config.interval_minutes,
            retain = retain,
            "backup task started"
        );

        // Run initial backup after a short delay (let the server finish starting)
        tokio::time::sleep(Duration::from_secs(5)).await;
        run_backup(&pool, &db_path, &backup_dir, retain);

        // Then run on interval
        let mut interval_timer = tokio::time::interval(interval);
        interval_timer.tick().await; // skip first immediate tick
        loop {
            interval_timer.tick().await;
            run_backup(&pool, &db_path, &backup_dir, retain);
        }
    })
}

/// Whether we've already logged the one-time hint about the legacy mirrored
/// `attachments/` dir left behind by the pre-LIF-266 backup scheme.
static LEGACY_MIRROR_HINTED: AtomicBool = AtomicBool::new(false);

/// Perform a single backup: write one self-contained `.tar.gz` archive via the
/// shared dump code path (same artifact `lific dump` produces), then rotate.
///
/// LIF-266: this replaces the old bare-`.db` snapshot plus additive
/// attachments-mirror scheme. The mirror grew forever (blobs were never GC'd);
/// self-contained archives sidestep that (at the cost of duplicating blobs per
/// archive — acceptable at current scale).
fn run_backup(pool: &DbPool, db_path: &Path, backup_dir: &Path, retain: usize) {
    let db_stem = db_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("lific");
    let filename = dump::archive_filename(db_stem, &dump::archive_timestamp());
    let backup_path = backup_dir.join(&filename);

    match dump::write_dump(pool, db_path, &backup_path) {
        Ok(manifest) => {
            let size = std::fs::metadata(&backup_path)
                .map(|m| m.len())
                .unwrap_or(0);
            info!(
                path = %backup_path.display(),
                size_kb = size / 1024,
                attachments = manifest.attachment_count,
                "backup archive written"
            );
        }
        Err(e) => {
            error!(error = %e, "backup archive failed");
            let _ = std::fs::remove_file(&backup_path);
            return;
        }
    }

    // One-time hint about the legacy mirrored attachments dir (old scheme). It
    // is no longer written to or read from; the operator can delete it.
    let legacy_mirror = backup_dir.join("attachments");
    if legacy_mirror.is_dir() && !LEGACY_MIRROR_HINTED.swap(true, Ordering::Relaxed) {
        info!(
            dir = %legacy_mirror.display(),
            "legacy mirrored attachments dir from the pre-archive backup scheme is no longer \
             used and can be deleted"
        );
    }

    rotate_backups(backup_dir, db_stem, retain);
}

/// Keep only the N most recent backup archives, delete the rest.
///
/// LIF-266: rotation candidates are the new `.tar.gz` archives AND legacy
/// bare-`.db` snapshots from the old scheme (both share the `{stem}_` prefix
/// and a sortable timestamp), so old snapshots age out naturally alongside new
/// archives. The legacy mirrored `attachments/` dir is left alone (it isn't a
/// per-run artifact); a one-time hint in `run_backup` notes it can be deleted.
fn rotate_backups(backup_dir: &Path, db_stem: &str, retain: usize) {
    let prefix = format!("{db_stem}_");
    let mut backups: Vec<PathBuf> = match std::fs::read_dir(backup_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                let name = match p.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n,
                    None => return false,
                };
                if !name.starts_with(&prefix) {
                    return false;
                }
                // New archives (`.tar.gz`) or legacy snapshots (`.db`).
                name.ends_with(".tar.gz")
                    || p.extension().and_then(|e| e.to_str()) == Some("db")
            })
            .collect(),
        Err(e) => {
            warn!(error = %e, "failed to read backup directory for rotation");
            return;
        }
    };

    // Sort by filename (timestamps sort lexicographically)
    backups.sort();

    // Remove oldest backups beyond retention
    if backups.len() > retain {
        let to_remove = backups.len() - retain;
        for path in backups.iter().take(to_remove) {
            match std::fs::remove_file(path) {
                Ok(()) => info!(path = %path.display(), "removed old backup"),
                Err(e) => warn!(path = %path.display(), error = %e, "failed to remove old backup"),
            }
        }
    }
}

/// Checkpoint the WAL into the main database file.
/// Call this on clean shutdown so the .db file is fully self-contained.
pub fn checkpoint_wal(pool: &DbPool) {
    match pool.write() {
        Ok(conn) => match conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);") {
            Ok(()) => info!("WAL checkpointed on shutdown"),
            Err(e) => warn!(error = %e, "WAL checkpoint failed"),
        },
        Err(e) => warn!(error = %e, "could not acquire write connection for checkpoint"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn make_temp_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("lific_backup_test_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rotate_keeps_only_retain_count() {
        let dir = make_temp_dir();

        // Create 5 fake archive files with lexicographic timestamps
        for i in 1..=5 {
            fs::write(dir.join(format!("lific_2026010{i}_120000.tar.gz")), "fake").unwrap();
        }

        rotate_backups(&dir, "lific", 3);

        let remaining: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).collect();
        assert_eq!(remaining.len(), 3);

        // Oldest two (01, 02) should be gone, newest three (03, 04, 05) kept
        assert!(!dir.join("lific_20260101_120000.tar.gz").exists());
        assert!(!dir.join("lific_20260102_120000.tar.gz").exists());
        assert!(dir.join("lific_20260103_120000.tar.gz").exists());
        assert!(dir.join("lific_20260105_120000.tar.gz").exists());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rotate_does_nothing_under_retain() {
        let dir = make_temp_dir();

        fs::write(dir.join("lific_20260101_120000.tar.gz"), "fake").unwrap();
        fs::write(dir.join("lific_20260102_120000.tar.gz"), "fake").unwrap();

        rotate_backups(&dir, "lific", 5);

        let count = fs::read_dir(&dir).unwrap().count();
        assert_eq!(count, 2);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rotate_ignores_other_files() {
        let dir = make_temp_dir();

        // These should be ignored (wrong prefix / extension)
        fs::write(dir.join("other_20260101_120000.tar.gz"), "x").unwrap();
        fs::write(dir.join("lific_20260101_120000.txt"), "x").unwrap();
        // These are real archives
        fs::write(dir.join("lific_20260101_120000.tar.gz"), "x").unwrap();
        fs::write(dir.join("lific_20260102_120000.tar.gz"), "x").unwrap();

        rotate_backups(&dir, "lific", 1);

        // Only 1 backup kept, non-matching files untouched
        assert!(dir.join("other_20260101_120000.tar.gz").exists());
        assert!(dir.join("lific_20260101_120000.txt").exists());
        assert!(!dir.join("lific_20260101_120000.tar.gz").exists()); // oldest removed
        assert!(dir.join("lific_20260102_120000.tar.gz").exists()); // kept

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rotate_ages_out_legacy_db_snapshots_alongside_archives() {
        // LIF-266: pre-archive `.db` snapshots from the old scheme are
        // rotation candidates too, so they age out naturally instead of
        // accumulating forever next to the new `.tar.gz` archives.
        let dir = make_temp_dir();
        // Two legacy .db snapshots (older timestamps) + two new archives.
        fs::write(dir.join("lific_20260101_120000.db"), "old1").unwrap();
        fs::write(dir.join("lific_20260102_120000.db"), "old2").unwrap();
        fs::write(dir.join("lific_20260103_120000.tar.gz"), "new1").unwrap();
        fs::write(dir.join("lific_20260104_120000.tar.gz"), "new2").unwrap();

        rotate_backups(&dir, "lific", 2);

        // The two oldest (legacy .db) are gone; the two newest archives kept.
        assert!(!dir.join("lific_20260101_120000.db").exists());
        assert!(!dir.join("lific_20260102_120000.db").exists());
        assert!(dir.join("lific_20260103_120000.tar.gz").exists());
        assert!(dir.join("lific_20260104_120000.tar.gz").exists());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_backup_emits_tar_gz_archive_with_data_and_blobs() {
        // LIF-266: the interval task now emits a single self-contained
        // `.tar.gz` archive (same artifact as `lific dump`) carrying the DB
        // snapshot and every non-.tmp attachment blob.
        let dir = make_temp_dir();
        let db_path = dir.join("lific.db");
        let backup_dir = dir.join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        // Seed the DB and an attachments sidecar dir next to it.
        let pool = crate::db::open(&db_path).expect("open test db");
        {
            let conn = pool.write().unwrap();
            crate::db::queries::create_project(
                &conn,
                &crate::db::models::CreateProject {
                    name: "BackupTest".into(),
                    identifier: "BKP".into(),
                    description: String::new(),
                    emoji: None,
                    lead_user_id: None,
                },
            )
            .unwrap();
        }
        let att_dir = dir.join("attachments");
        fs::create_dir_all(&att_dir).unwrap();
        fs::write(att_dir.join("deadbeefsha"), b"blob contents").unwrap();
        fs::write(att_dir.join("deadbeefsha.tmp"), b"partial").unwrap();

        run_backup(&pool, &db_path, &backup_dir, 5);

        // Exactly one `.tar.gz` archive, no bare `.db` snapshot.
        let archives: Vec<_> = fs::read_dir(&backup_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".tar.gz"))
            .collect();
        assert_eq!(archives.len(), 1, "expected one archive, got {archives:?}");

        // Its contents: db + manifest + the blob, excluding the .tmp write.
        let archive_path = backup_dir.join(&archives[0]);
        let file = fs::File::open(&archive_path).unwrap();
        let dec = flate2::read::GzDecoder::new(file);
        let mut tar = tar::Archive::new(dec);
        let names: Vec<String> = tar
            .entries()
            .unwrap()
            .map(|e| e.unwrap().path().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.iter().any(|n| n == crate::dump::ARCHIVE_DB_NAME));
        assert!(names.iter().any(|n| n == crate::dump::ARCHIVE_MANIFEST_NAME));
        assert!(names.iter().any(|n| n == "attachments/deadbeefsha"));
        assert!(
            !names.iter().any(|n| n.ends_with(".tmp")),
            "in-progress .tmp writes must not be archived: {names:?}"
        );

        fs::remove_dir_all(&dir).ok();
    }
}
