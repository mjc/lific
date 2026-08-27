//! Retention sweep for soft-deleted rows (LIF-438).
//!
//! A delete leaves a tombstone so delta sync can advertise it and so the user
//! can undo it. Both reasons expire. This module is the other end of that
//! bargain: past the configured window, the tombstone is physically removed
//! and the row's identity is gone for good.
//!
//! The physical DELETE is also the only thing that ever fires the foreign-key
//! cascades. Labels, relations and attachment links survive a soft delete on
//! purpose (a restore has to bring them back with the row), so this is where
//! they finally go.

use rusqlite::{Connection, params};

use crate::error::LificError;

/// What one sweep removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PurgeCounts {
    pub issues: usize,
    pub pages: usize,
    pub comments: usize,
}

impl PurgeCounts {
    pub fn total(&self) -> usize {
        self.issues + self.pages + self.comments
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

/// Physically delete every tombstone older than `trash_days`.
///
/// `0` means "never purge" and is honored as a no-op rather than as a zero-day
/// window, which would empty the trash the instant anything landed in it.
///
/// The cutoff is computed by SQLite (`datetime('now', '-N days')`) because
/// `deleted_at` is written by `datetime('now')` on the same clock in the same
/// format; comparing against a value formatted in Rust would be comparing two
/// different things that happen to look alike. Same shape as
/// `backup::prune_audit_log` and `attachments::find_orphans`.
///
/// Comments go first so that the comment count reflects rows removed as
/// tombstones in their own right, and every comment tombstoned alongside its
/// parent is already gone by the time the parent's DELETE cascades. Search
/// index rows were removed when the tombstone was created, so the AFTER DELETE
/// FTS triggers here match nothing — a no-op DELETE against an ordinary fts5
/// table, not a double removal. The audit triggers stay silent too: migration
/// 047 guards them on `OLD.deleted_at IS NULL`, so the purge does not restate
/// a deletion the log already recorded.
pub fn purge_tombstones(conn: &Connection, trash_days: u32) -> Result<PurgeCounts, LificError> {
    if trash_days == 0 {
        return Ok(PurgeCounts::default());
    }
    let cutoff = format!("-{trash_days} days");

    let comments = conn.execute(
        "DELETE FROM comments
          WHERE deleted_at IS NOT NULL AND deleted_at < datetime('now', ?1)",
        params![cutoff],
    )?;
    let issues = conn.execute(
        "DELETE FROM issues
          WHERE deleted_at IS NOT NULL AND deleted_at < datetime('now', ?1)",
        params![cutoff],
    )?;
    let pages = conn.execute(
        "DELETE FROM pages
          WHERE deleted_at IS NOT NULL AND deleted_at < datetime('now', ?1)",
        params![cutoff],
    )?;

    Ok(PurgeCounts {
        issues,
        pages,
        comments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::db::models::*;
    use crate::db::queries;

    fn seeded() -> (db::DbPool, i64, i64) {
        let pool = db::open_memory().expect("test db");
        let (project_id, user_id) = {
            let conn = pool.write().unwrap();
            let project = queries::create_project(
                &conn,
                &CreateProject {
                    name: "Trash".into(),
                    identifier: "TRA".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            conn.execute(
                "INSERT INTO users (username, email, password_hash, display_name, is_admin, is_bot)
                 VALUES ('ada', 'ada@test.local', 'x', 'Ada', 1, 0)",
                [],
            )
            .unwrap();
            (project.id, conn.last_insert_rowid())
        };
        (pool, project_id, user_id)
    }

    fn new_issue(project_id: i64, title: &str) -> CreateIssue {
        CreateIssue {
            project_id,
            title: title.into(),
            ..Default::default()
        }
    }

    /// Backdate a tombstone so the sweep sees it as old.
    fn age_tombstone(conn: &Connection, table: &str, id: i64, days: i64) {
        conn.execute(
            &format!(
                "UPDATE {table} SET deleted_at = datetime('now', '-{days} days') WHERE id = ?1"
            ),
            params![id],
        )
        .unwrap();
    }

    #[test]
    fn purge_removes_old_tombstones_and_their_children() {
        let (pool, pid, uid) = seeded();
        let conn = pool.write().unwrap();
        let issue = queries::create_issue(&conn, &new_issue(pid, "Ancient")).unwrap();
        let comment = queries::comments::create_comment(
            &conn,
            queries::comments::CommentParent::Issue(issue.id),
            uid,
            "goes with it",
        )
        .unwrap();
        queries::delete_issue(&conn, issue.id).unwrap();
        age_tombstone(&conn, "issues", issue.id, 60);
        age_tombstone(&conn, "comments", comment.id, 60);

        let counts = purge_tombstones(&conn, 30).unwrap();
        assert_eq!(counts.issues, 1);
        assert_eq!(counts.comments, 1);

        let issue_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM issues WHERE id = ?1",
                params![issue.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(issue_rows, 0, "the row itself is gone");
        let comment_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM comments WHERE id = ?1",
                params![comment.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(comment_rows, 0);
    }

    #[test]
    fn purge_spares_tombstones_inside_the_window() {
        let (pool, pid, _) = seeded();
        let conn = pool.write().unwrap();
        let issue = queries::create_issue(&conn, &new_issue(pid, "Recent")).unwrap();
        queries::delete_issue(&conn, issue.id).unwrap();

        assert_eq!(purge_tombstones(&conn, 30).unwrap(), PurgeCounts::default());
        assert!(queries::restore_issue(&conn, issue.id).is_ok());
    }

    #[test]
    fn purge_never_touches_live_rows() {
        let (pool, pid, _) = seeded();
        let conn = pool.write().unwrap();
        let live = queries::create_issue(&conn, &new_issue(pid, "Alive")).unwrap();
        // A window of one day with nothing deleted still must not sweep.
        assert!(purge_tombstones(&conn, 1).unwrap().is_empty());
        assert!(queries::get_issue(&conn, live.id).is_ok());
    }

    #[test]
    fn trash_days_zero_disables_the_sweep() {
        let (pool, pid, _) = seeded();
        let conn = pool.write().unwrap();
        let issue = queries::create_issue(&conn, &new_issue(pid, "Kept forever")).unwrap();
        queries::delete_issue(&conn, issue.id).unwrap();
        age_tombstone(&conn, "issues", issue.id, 4000);

        assert_eq!(purge_tombstones(&conn, 0).unwrap(), PurgeCounts::default());
        assert!(queries::restore_issue(&conn, issue.id).is_ok());
    }

    #[test]
    fn purge_leaves_the_search_index_clean() {
        let (pool, pid, _) = seeded();
        let conn = pool.write().unwrap();
        let issue = queries::create_issue(&conn, &new_issue(pid, "Findable widget")).unwrap();
        queries::delete_issue(&conn, issue.id).unwrap();
        age_tombstone(&conn, "issues", issue.id, 90);
        purge_tombstones(&conn, 30).unwrap();

        let indexed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM search_index WHERE entity_type = 'issue' AND entity_id = ?1",
                params![issue.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(indexed, 0);
    }

    #[test]
    fn purge_writes_no_second_audit_row() {
        let (pool, pid, _) = seeded();
        let conn = pool.write().unwrap();
        let issue = queries::create_issue(&conn, &new_issue(pid, "Audited")).unwrap();
        queries::delete_issue(&conn, issue.id).unwrap();
        age_tombstone(&conn, "issues", issue.id, 90);

        let before: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM audit_log WHERE entity_type = 'issue' AND entity_id = ?1",
                params![issue.id],
                |r| r.get(0),
            )
            .unwrap();
        purge_tombstones(&conn, 30).unwrap();
        let after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM audit_log WHERE entity_type = 'issue' AND entity_id = ?1",
                params![issue.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(before, after, "a purge is not a second deletion");
    }

    #[test]
    fn purge_collects_page_tombstones_too() {
        let (pool, pid, _) = seeded();
        let conn = pool.write().unwrap();
        let page = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(pid),
                title: "Doomed doc".into(),
                ..Default::default()
            },
        )
        .unwrap();
        queries::delete_page(&conn, page.id).unwrap();
        age_tombstone(&conn, "pages", page.id, 45);

        assert_eq!(purge_tombstones(&conn, 30).unwrap().pages, 1);
        let rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pages WHERE id = ?1",
                params![page.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rows, 0);
    }
}
