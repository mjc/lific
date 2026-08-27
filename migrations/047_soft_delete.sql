-- LIF-438: soft delete + tombstones for issues, pages and comments.
--
-- Delta sync needs to learn about deletions. A hard DELETE leaves nothing
-- behind to learn from: a replica asking "what changed above seq N" sees the
-- row simply stop appearing, which is indistinguishable from "it was never in
-- my scope". So a delete now sets `deleted_at` instead, the row keeps its
-- identity, and migration 045's stamp triggers give the tombstone its own
-- place in the sync stream.
--
-- ── Why the stamp still fires ─────────────────────────────────────────
-- Every AFTER UPDATE trigger on these three tables carries 045's
-- `WHEN NEW.seq IS OLD.seq` guard, which means "this update is not the stamp
-- writing seq back". A soft delete changes `deleted_at` and leaves `seq`
-- alone, so the guard passes and `stamp_issues_au` / `stamp_pages_au` /
-- `stamp_comments_au` allocate a fresh seq. The tombstone is a normal
-- mutation as far as sync is concerned.
--
-- ── Cascade, in the same transaction ──────────────────────────────────
-- Deleting an issue or a page tombstones its live comments by copying the
-- parent's *exact* `deleted_at` value. That shared timestamp is what makes
-- restore precise: restoring the parent revives only the children that went
-- down with it, and a comment deleted on its own beforehand carries a
-- different timestamp and stays deleted. Each cascaded comment passes
-- through the comment stamp trigger, so it gets its own seq too.
--
-- ── FTS ───────────────────────────────────────────────────────────────
-- 001's `issues_au` / `pages_au` and 034's `comments_search_au` are the only
-- places a live row is (re)indexed on update, so they are the right place to
-- express "a tombstone is not searchable": the DELETE half runs
-- unconditionally, the INSERT half is now conditional on
-- `NEW.deleted_at IS NULL`. One guard covers three cases — soft delete drops
-- the row out of the index, restore puts it back, and an edit to an already
-- deleted row does not resurrect it. The AFTER DELETE FTS triggers stay as
-- they are: a purge deleting an index row that is already gone is a no-op on
-- an ordinary (non-external-content) fts5 table.
--
-- ── audit_log ─────────────────────────────────────────────────────────
-- Soft delete and restore each write one row, mirroring 018's row shape and
-- its `_actor_state` attribution. 018's AFTER DELETE triggers are recreated
-- with a `WHEN OLD.deleted_at IS NULL` guard so that the eventual physical
-- purge of a tombstone does not write a second delete row years after the
-- user's action. A row hard-deleted without ever being tombstoned (a project
-- delete cascading through the foreign keys) still records its 'delete'.

ALTER TABLE issues   ADD COLUMN deleted_at TEXT;
ALTER TABLE pages    ADD COLUMN deleted_at TEXT;
ALTER TABLE comments ADD COLUMN deleted_at TEXT;

-- ── Indexes ───────────────────────────────────────────────────────────
-- Partial on `deleted_at IS NOT NULL`: the tombstone set is small and the
-- purge sweep ("everything deleted before this cutoff") is the only query
-- that scans it. Indexing the whole column would index a NULL for every live
-- row to answer a question only tombstones can be part of.

CREATE INDEX IF NOT EXISTS idx_issues_deleted_at
    ON issues(deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_pages_deleted_at
    ON pages(deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_comments_deleted_at
    ON comments(deleted_at) WHERE deleted_at IS NOT NULL;

-- The cascade triggers below find a parent's live children by parent id, and
-- the comment list does the same on every issue/page read.
CREATE INDEX IF NOT EXISTS idx_comments_issue_live
    ON comments(issue_id) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_comments_page_live
    ON comments(page_id) WHERE deleted_at IS NULL;

-- ── Import provenance, narrowed to live rows ──────────────────────────
-- 033's UNIQUE(source) is what makes a re-import idempotent: the second
-- attempt to insert the same external issue collides and the importer skips
-- it. A tombstone must not participate in that. Someone who deletes an
-- imported issue and re-runs the import is asking for it back, and an index
-- entry held by a deleted row would answer "already imported" forever, leaving
-- them with nothing and no way to say otherwise. Narrowing the index to live
-- rows keeps idempotency exactly where it belongs and lets the tombstone sit
-- in the trash until the retention sweep collects it.

DROP INDEX IF EXISTS idx_issues_source;
CREATE UNIQUE INDEX idx_issues_source
    ON issues(source) WHERE source IS NOT NULL AND deleted_at IS NULL;

-- ── FTS update triggers, tombstone-aware ──────────────────────────────
-- Recreated verbatim from 045 except that the re-index is conditional.

DROP TRIGGER IF EXISTS issues_au;
CREATE TRIGGER issues_au AFTER UPDATE ON issues
WHEN NEW.seq IS OLD.seq
BEGIN
    DELETE FROM search_index WHERE entity_type = 'issue' AND entity_id = OLD.id;
    INSERT INTO search_index(title, body, entity_type, entity_id, project_id)
    SELECT NEW.title, NEW.description, 'issue', NEW.id, NEW.project_id
    WHERE NEW.deleted_at IS NULL;
END;

DROP TRIGGER IF EXISTS pages_au;
CREATE TRIGGER pages_au AFTER UPDATE ON pages
WHEN NEW.seq IS OLD.seq
BEGIN
    DELETE FROM search_index WHERE entity_type = 'page' AND entity_id = OLD.id;
    INSERT INTO search_index(title, body, entity_type, entity_id, project_id)
    SELECT NEW.title, NEW.content, 'page', NEW.id, NEW.project_id
    WHERE NEW.deleted_at IS NULL;
END;

DROP TRIGGER IF EXISTS comments_search_au;
CREATE TRIGGER comments_search_au AFTER UPDATE ON comments
WHEN NEW.seq IS OLD.seq
BEGIN
    DELETE FROM search_index WHERE entity_type = 'comment' AND entity_id = OLD.id;
    INSERT INTO search_index(title, body, entity_type, entity_id, project_id)
    SELECT '', NEW.content, 'comment', NEW.id,
           COALESCE(
               (SELECT project_id FROM issues WHERE id = NEW.issue_id),
               (SELECT project_id FROM pages  WHERE id = NEW.page_id)
           )
    WHERE NEW.deleted_at IS NULL;
END;

-- ── Cascade: parent tombstone reaches its comments ────────────────────

CREATE TRIGGER IF NOT EXISTS issues_soft_delete_cascade
AFTER UPDATE OF deleted_at ON issues
WHEN OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL
BEGIN
    UPDATE comments SET deleted_at = NEW.deleted_at
     WHERE issue_id = NEW.id AND deleted_at IS NULL;
END;

CREATE TRIGGER IF NOT EXISTS pages_soft_delete_cascade
AFTER UPDATE OF deleted_at ON pages
WHEN OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL
BEGIN
    UPDATE comments SET deleted_at = NEW.deleted_at
     WHERE page_id = NEW.id AND deleted_at IS NULL;
END;

-- Restore revives exactly the children that share the parent's old timestamp.
-- `deleted_at = OLD.deleted_at` is the whole mechanism: a comment deleted
-- independently before the parent carries a different value and is left alone.

CREATE TRIGGER IF NOT EXISTS issues_restore_cascade
AFTER UPDATE OF deleted_at ON issues
WHEN OLD.deleted_at IS NOT NULL AND NEW.deleted_at IS NULL
BEGIN
    UPDATE comments SET deleted_at = NULL
     WHERE issue_id = NEW.id AND deleted_at = OLD.deleted_at;
END;

CREATE TRIGGER IF NOT EXISTS pages_restore_cascade
AFTER UPDATE OF deleted_at ON pages
WHEN OLD.deleted_at IS NOT NULL AND NEW.deleted_at IS NULL
BEGIN
    UPDATE comments SET deleted_at = NULL
     WHERE page_id = NEW.id AND deleted_at = OLD.deleted_at;
END;

-- ── audit_log: 'delete' / 'restore' ─────────────────────────────────

CREATE TRIGGER IF NOT EXISTS audit_issues_soft_delete
AFTER UPDATE OF deleted_at ON issues
WHEN OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL
BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'issue', NEW.id,
        (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-' || NEW.sequence,
        NEW.project_id, NEW.id, 'delete', NEW.title
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_issues_restore
AFTER UPDATE OF deleted_at ON issues
WHEN OLD.deleted_at IS NOT NULL AND NEW.deleted_at IS NULL
BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'issue', NEW.id,
        (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-' || NEW.sequence,
        NEW.project_id, NEW.id, 'restore', NEW.title
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_pages_soft_delete
AFTER UPDATE OF deleted_at ON pages
WHEN OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL
BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, page_id, action, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'page', NEW.id,
        CASE WHEN NEW.project_id IS NULL THEN 'DOC-' || NEW.sequence
             ELSE (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-DOC-' || NEW.sequence END,
        NEW.project_id, NEW.id, 'delete', NEW.title
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_pages_restore
AFTER UPDATE OF deleted_at ON pages
WHEN OLD.deleted_at IS NOT NULL AND NEW.deleted_at IS NULL
BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, page_id, action, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'page', NEW.id,
        CASE WHEN NEW.project_id IS NULL THEN 'DOC-' || NEW.sequence
             ELSE (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-DOC-' || NEW.sequence END,
        NEW.project_id, NEW.id, 'restore', NEW.title
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_comments_soft_delete
AFTER UPDATE OF deleted_at ON comments
WHEN OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL
BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, page_id, action, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'comment', NEW.id,
        COALESCE(
            (SELECT p.identifier || '-' || i.sequence FROM issues i JOIN projects p ON p.id = i.project_id WHERE i.id = NEW.issue_id),
            (SELECT CASE WHEN pg.project_id IS NULL THEN 'DOC-' || pg.sequence
                         ELSE pr.identifier || '-DOC-' || pg.sequence END
             FROM pages pg LEFT JOIN projects pr ON pr.id = pg.project_id WHERE pg.id = NEW.page_id)
        ),
        COALESCE((SELECT project_id FROM issues WHERE id = NEW.issue_id),
                 (SELECT project_id FROM pages WHERE id = NEW.page_id)),
        NEW.issue_id, NEW.page_id, 'delete', NEW.content
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_comments_restore
AFTER UPDATE OF deleted_at ON comments
WHEN OLD.deleted_at IS NOT NULL AND NEW.deleted_at IS NULL
BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, page_id, action, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'comment', NEW.id,
        COALESCE(
            (SELECT p.identifier || '-' || i.sequence FROM issues i JOIN projects p ON p.id = i.project_id WHERE i.id = NEW.issue_id),
            (SELECT CASE WHEN pg.project_id IS NULL THEN 'DOC-' || pg.sequence
                         ELSE pr.identifier || '-DOC-' || pg.sequence END
             FROM pages pg LEFT JOIN projects pr ON pr.id = pg.project_id WHERE pg.id = NEW.page_id)
        ),
        COALESCE((SELECT project_id FROM issues WHERE id = NEW.issue_id),
                 (SELECT project_id FROM pages WHERE id = NEW.page_id)),
        NEW.issue_id, NEW.page_id, 'restore', NEW.content
    );
END;

-- ── 018's physical-delete audit rows, guarded ─────────────────────────
-- Otherwise the retention purge would write a second, misdated delete row for
-- every tombstone it collects.

DROP TRIGGER IF EXISTS audit_issues_delete;
CREATE TRIGGER audit_issues_delete AFTER DELETE ON issues
WHEN OLD.deleted_at IS NULL
BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'issue', OLD.id,
        (SELECT identifier FROM projects WHERE id = OLD.project_id) || '-' || OLD.sequence,
        OLD.project_id, OLD.id, 'delete', OLD.title
    );
END;

DROP TRIGGER IF EXISTS audit_pages_delete;
CREATE TRIGGER audit_pages_delete AFTER DELETE ON pages
WHEN OLD.deleted_at IS NULL
BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, page_id, action, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'page', OLD.id,
        CASE WHEN OLD.project_id IS NULL THEN 'DOC-' || OLD.sequence
             ELSE (SELECT identifier FROM projects WHERE id = OLD.project_id) || '-DOC-' || OLD.sequence END,
        OLD.project_id, OLD.id, 'delete', OLD.title
    );
END;

DROP TRIGGER IF EXISTS audit_comments_delete;
CREATE TRIGGER audit_comments_delete AFTER DELETE ON comments
WHEN OLD.deleted_at IS NULL
BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, page_id, action, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'comment', OLD.id,
        COALESCE(
            (SELECT p.identifier || '-' || i.sequence FROM issues i JOIN projects p ON p.id = i.project_id WHERE i.id = OLD.issue_id),
            (SELECT CASE WHEN pg.project_id IS NULL THEN 'DOC-' || pg.sequence
                         ELSE pr.identifier || '-DOC-' || pg.sequence END
             FROM pages pg LEFT JOIN projects pr ON pr.id = pg.project_id WHERE pg.id = OLD.page_id)
        ),
        COALESCE((SELECT project_id FROM issues WHERE id = OLD.issue_id),
                 (SELECT project_id FROM pages WHERE id = OLD.page_id)),
        OLD.issue_id, OLD.page_id, 'delete', OLD.content
    );
END;
