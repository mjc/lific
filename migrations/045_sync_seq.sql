-- LIF-436: instance-scoped monotonic sequence for issues, pages and comments.
--
-- Every mutation of a syncable row gets a strictly increasing `seq` drawn
-- from one instance-wide counter (`sync_seq`). A replica can then ask for
-- "everything above N" and get a total order over the three tables without
-- reconciling wall clocks.
--
-- ── Why triggers ──────────────────────────────────────────────────────
-- Writes reach these tables from REST, MCP, the CLI, the importers, the
-- insights refresh and the activity/audit triggers themselves. Stamping in
-- the query layer would leak the moment someone adds a new write path, so
-- the counter is driven from SQL, where every path has to pass.
--
-- ── Recursion, and what `recursive_triggers = OFF` actually buys ──────
-- The stamp is itself an `UPDATE` on the table being stamped, so this
-- migration lives or dies on SQLite's re-entry rules. Measured, not
-- assumed:
--
--   * `recursive_triggers` is OFF (SQLite's default; nothing in
--     `db::mod::apply_pragmas` or `open_memory` turns it on). That stops a
--     trigger from firing ITSELF, directly or indirectly.
--   * It does NOT stop a trigger body from firing the OTHER triggers on the
--     same table. An `UPDATE t` inside an AFTER INSERT trigger on `t` runs
--     every AFTER UPDATE trigger on `t`.
--
-- So the stamping UPDATE is visible to every other AFTER UPDATE trigger on
-- these tables, and left unguarded it does real damage: 001's `issues_au`
-- would re-index a row `issues_ai` had just indexed (duplicate FTS hits),
-- and 001's `issues_updated` would rewrite `updated_at` on plain inserts.
--
-- The fix is one guard, applied consistently: a seq-only write changes
-- `seq`, so `WHEN NEW.seq IS OLD.seq` identifies "this is not the stamp"
-- and every AFTER UPDATE trigger on these three tables carries it. That
-- also makes the stamp triggers safe if `recursive_triggers` is ever turned
-- on. The triggers that need it and predate this migration (001's FTS and
-- updated_at bumps, 034's comment FTS, 017's activity bumps) are dropped
-- and recreated below, otherwise verbatim.
--
-- The 018 audit triggers deliberately keep NO guard: they are already
-- per-field WHEN-guarded, a seq-only update matches none of those fields,
-- and so a stamp produces zero audit_log rows. Verified by test.
--
-- ── Trigger-originated activity ───────────────────────────────────────
-- Migration 017 bumps the parent issue's `updated_at` when a comment or a
-- label changes. Its UPDATE carries `seq` now too, so an issue that gains a
-- comment advances in the sync stream instead of quietly changing
-- `updated_at` behind a stale seq.

CREATE TABLE IF NOT EXISTS sync_seq (
    id    INTEGER PRIMARY KEY CHECK (id = 1),
    value INTEGER NOT NULL
);
INSERT OR IGNORE INTO sync_seq (id, value) VALUES (1, 0);

ALTER TABLE issues   ADD COLUMN seq INTEGER;
ALTER TABLE pages    ADD COLUMN seq INTEGER;
ALTER TABLE comments ADD COLUMN seq INTEGER;

-- ── Clear the way ─────────────────────────────────────────────────────
-- Dropped before the backfill for two reasons: the backfill is a plain
-- UPDATE that would otherwise have `issues_updated` / `pages_updated` /
-- `comments_bump_issue_au` rewrite every `updated_at` to now, and the FTS
-- bumps would churn the whole index for a change that touches no indexed
-- text. All of them come back below.

DROP TRIGGER IF EXISTS issues_au;
DROP TRIGGER IF EXISTS issues_updated;
DROP TRIGGER IF EXISTS pages_au;
DROP TRIGGER IF EXISTS pages_updated;
DROP TRIGGER IF EXISTS comments_search_au;
DROP TRIGGER IF EXISTS comments_bump_issue_ai;
DROP TRIGGER IF EXISTS comments_bump_issue_au;
DROP TRIGGER IF EXISTS comments_bump_issue_ad;
DROP TRIGGER IF EXISTS issue_labels_bump_ai;
DROP TRIGGER IF EXISTS issue_labels_bump_ad;

-- ── Backfill ──────────────────────────────────────────────────────────
-- Ascending by updated_at within each table, issues then pages then
-- comments. Any deterministic order satisfies monotonicity; this one at
-- least puts a replica's first full sync in a plausible chronology.

WITH ordered AS (
    SELECT id AS eid, ROW_NUMBER() OVER (ORDER BY updated_at, id) AS rn FROM issues
)
UPDATE issues SET seq = ordered.rn FROM ordered WHERE issues.id = ordered.eid;

WITH ordered AS (
    SELECT id AS eid, ROW_NUMBER() OVER (ORDER BY updated_at, id) AS rn FROM pages
)
UPDATE pages
   SET seq = (SELECT COUNT(*) FROM issues) + ordered.rn
  FROM ordered WHERE pages.id = ordered.eid;

WITH ordered AS (
    SELECT id AS eid, ROW_NUMBER() OVER (ORDER BY updated_at, id) AS rn FROM comments
)
UPDATE comments
   SET seq = (SELECT COUNT(*) FROM issues) + (SELECT COUNT(*) FROM pages) + ordered.rn
  FROM ordered WHERE comments.id = ordered.eid;

UPDATE sync_seq
   SET value = (SELECT COUNT(*) FROM issues)
             + (SELECT COUNT(*) FROM pages)
             + (SELECT COUNT(*) FROM comments)
 WHERE id = 1;

-- ── Indexes ───────────────────────────────────────────────────────────
-- "what changed in this project since N" is the sync read. Comments carry
-- no project_id (they hang off an issue XOR a page), so theirs is a bare
-- seq index and scope is recovered by joining the parent.

CREATE INDEX IF NOT EXISTS idx_issues_project_seq ON issues(project_id, seq DESC);
CREATE INDEX IF NOT EXISTS idx_pages_project_seq  ON pages(project_id, seq DESC);
CREATE INDEX IF NOT EXISTS idx_comments_seq       ON comments(seq DESC);

-- ── 001's FTS and updated_at bumps, guarded ───────────────────────────

CREATE TRIGGER IF NOT EXISTS issues_au AFTER UPDATE ON issues
WHEN NEW.seq IS OLD.seq
BEGIN
    DELETE FROM search_index WHERE entity_type = 'issue' AND entity_id = OLD.id;
    INSERT INTO search_index(title, body, entity_type, entity_id, project_id)
    VALUES (NEW.title, NEW.description, 'issue', NEW.id, NEW.project_id);
END;

CREATE TRIGGER IF NOT EXISTS pages_au AFTER UPDATE ON pages
WHEN NEW.seq IS OLD.seq
BEGIN
    DELETE FROM search_index WHERE entity_type = 'page' AND entity_id = OLD.id;
    INSERT INTO search_index(title, body, entity_type, entity_id, project_id)
    VALUES (NEW.title, NEW.content, 'page', NEW.id, NEW.project_id);
END;

CREATE TRIGGER IF NOT EXISTS issues_updated AFTER UPDATE ON issues
WHEN NEW.seq IS OLD.seq
BEGIN
    UPDATE issues SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS pages_updated AFTER UPDATE ON pages
WHEN NEW.seq IS OLD.seq
BEGIN
    UPDATE pages SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- ── 034's comment FTS bump, guarded ───────────────────────────────────

CREATE TRIGGER IF NOT EXISTS comments_search_au AFTER UPDATE ON comments
WHEN NEW.seq IS OLD.seq
BEGIN
    DELETE FROM search_index WHERE entity_type = 'comment' AND entity_id = OLD.id;
    INSERT INTO search_index(title, body, entity_type, entity_id, project_id)
    SELECT '', NEW.content, 'comment', NEW.id,
           COALESCE(
               (SELECT project_id FROM issues WHERE id = NEW.issue_id),
               (SELECT project_id FROM pages  WHERE id = NEW.page_id)
           );
END;

-- ── Stamp triggers ────────────────────────────────────────────────────

CREATE TRIGGER IF NOT EXISTS stamp_issues_ai AFTER INSERT ON issues
BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE issues SET seq = (SELECT value FROM sync_seq WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS stamp_issues_au AFTER UPDATE ON issues
WHEN NEW.seq IS OLD.seq
BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE issues SET seq = (SELECT value FROM sync_seq WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS stamp_pages_ai AFTER INSERT ON pages
BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE pages SET seq = (SELECT value FROM sync_seq WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS stamp_pages_au AFTER UPDATE ON pages
WHEN NEW.seq IS OLD.seq
BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE pages SET seq = (SELECT value FROM sync_seq WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS stamp_comments_ai AFTER INSERT ON comments
BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE comments SET seq = (SELECT value FROM sync_seq WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS stamp_comments_au AFTER UPDATE ON comments
WHEN NEW.seq IS OLD.seq
BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE comments SET seq = (SELECT value FROM sync_seq WHERE id = 1) WHERE id = NEW.id;
END;

-- ── 017's activity bumps, now seq-aware ───────────────────────────────
-- Identical to migration 017 except that the issue's seq is allocated and
-- stamped in the same statement as the updated_at bump, and the UPDATE
-- variant skips the stamp's own write. Without the seq, an issue would
-- advertise activity (a new updated_at, a new comment) that a seq-based
-- sync could never see.

CREATE TRIGGER IF NOT EXISTS comments_bump_issue_ai
AFTER INSERT ON comments
WHEN NEW.issue_id IS NOT NULL
BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE issues
       SET updated_at = datetime('now'),
           seq = (SELECT value FROM sync_seq WHERE id = 1)
     WHERE id = NEW.issue_id;
END;

CREATE TRIGGER IF NOT EXISTS comments_bump_issue_au
AFTER UPDATE ON comments
WHEN NEW.issue_id IS NOT NULL AND NEW.seq IS OLD.seq
BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE issues
       SET updated_at = datetime('now'),
           seq = (SELECT value FROM sync_seq WHERE id = 1)
     WHERE id = NEW.issue_id;
END;

CREATE TRIGGER IF NOT EXISTS comments_bump_issue_ad
AFTER DELETE ON comments
WHEN OLD.issue_id IS NOT NULL
BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE issues
       SET updated_at = datetime('now'),
           seq = (SELECT value FROM sync_seq WHERE id = 1)
     WHERE id = OLD.issue_id;
END;

CREATE TRIGGER IF NOT EXISTS issue_labels_bump_ai
AFTER INSERT ON issue_labels
BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE issues
       SET updated_at = datetime('now'),
           seq = (SELECT value FROM sync_seq WHERE id = 1)
     WHERE id = NEW.issue_id;
END;

CREATE TRIGGER IF NOT EXISTS issue_labels_bump_ad
AFTER DELETE ON issue_labels
BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE issues
       SET updated_at = datetime('now'),
           seq = (SELECT value FROM sync_seq WHERE id = 1)
     WHERE id = OLD.issue_id;
END;
