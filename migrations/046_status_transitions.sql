-- LIF-437: a dedicated history of issue status changes.
--
-- The audit log already records status edits as one `update` row among
-- every other field change. That is the right shape for a feed and the
-- wrong shape for cycle-time questions ("how long does an issue sit in
-- in_progress"), which want a narrow, dense table they can scan per issue.
-- This is that table. Nothing reads it yet.
--
-- Captured by a trigger for the same reason the audit log is: status moves
-- through REST, MCP, the CLI, the kanban board's drag-and-drop, the plan
-- step effects and the importers, and a query-layer hook would miss the
-- next one somebody adds.
--
-- Actor attribution reuses the audit log's mechanism verbatim: the one-row
-- `_actor_state` table that `DbPool::write` stamps before handing out the
-- exclusive write connection (migration 018, `src/actor.rs`). Both halves
-- are recorded, exactly as audit_log records them: `actor_user_id` is the
-- users.id at write time (NULL for system writes, no FK on purpose, same
-- rationale as audit_log), and `transport` is the always-present door
-- string ('system' for migrations, startup and unstamped connections).
-- Cycle-time wants "how long", standup wants "who moved it"; transport
-- alone cannot answer the second.
--
-- `seq` places the transition on the instance-wide sequence from migration
-- 045, so a replica pulling "everything above my cursor" gets transitions
-- alongside the issues they belong to.
--
-- It draws its own tick rather than copying the issue's. Both this trigger
-- and 045's `stamp_issues_au` fire on the same UPDATE and SQLite does not
-- define their relative order, so simply reading `sync_seq` here can return
-- the value from *before* the issue was restamped. That number is a cursor
-- a replica has very likely already passed (it is the issue's previous
-- seq), and the transition would be skipped forever. One extra tick per
-- status change costs nothing and puts the row unambiguously ahead of every
-- cursor that predates it.

CREATE TABLE IF NOT EXISTS status_transitions (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    issue_id      INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    from_status   TEXT    NOT NULL,
    to_status     TEXT    NOT NULL,
    actor_user_id INTEGER,           -- users.id at write time; no FK on purpose
    transport     TEXT    NOT NULL DEFAULT 'system',
    seq           INTEGER,
    created_at    TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_status_transitions_issue ON status_transitions(issue_id);

CREATE TRIGGER IF NOT EXISTS status_transitions_capture
AFTER UPDATE OF status ON issues
WHEN NEW.status IS NOT OLD.status
BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    INSERT INTO status_transitions
        (issue_id, from_status, to_status, actor_user_id, transport, seq)
    VALUES (
        NEW.id,
        OLD.status,
        NEW.status,
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        (SELECT value FROM sync_seq WHERE id = 1)
    );
END;
