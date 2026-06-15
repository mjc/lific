-- LIF-165: Plans — persisted, nestable step trees that survive across agent
-- sessions. Deliberately NOT sub-issues: issues stay flat, the hierarchy lives
-- on the plan. A step optionally references a flat issue; when it does, the
-- step mirrors that issue (cascade lives in migration 020).
--
-- Storage is an adjacency list (children reference parent via parent_step_id),
-- not a JSON blob: the issue<->step cascade is `WHERE issue_id = ?` against an
-- index, surgical edits are single-row UPDATEs, and concurrent writers don't
-- clobber a shared blob. Render shape (nested tree) is assembled in app code.

CREATE TABLE IF NOT EXISTS plans (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    sequence    INTEGER NOT NULL,                                 -- per-project, identifier PROJ-PLAN-n
    issue_id    INTEGER REFERENCES issues(id) ON DELETE SET NULL, -- anchor issue
    title       TEXT    NOT NULL,
    status      TEXT    NOT NULL DEFAULT 'active'
                        CHECK(status IN ('active','done','archived')),
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE(project_id, sequence)
);

CREATE TABLE IF NOT EXISTS plan_steps (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    plan_id               INTEGER NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
    parent_step_id        INTEGER REFERENCES plan_steps(id) ON DELETE CASCADE,
    position              INTEGER NOT NULL DEFAULT 0,
    title                 TEXT    NOT NULL,
    description           TEXT    NOT NULL DEFAULT '',
    issue_id              INTEGER REFERENCES issues(id) ON DELETE SET NULL,
    done                  INTEGER NOT NULL DEFAULT 0,
    -- set when an issue reopen auto-unchecked this step; cleared on next
    -- completion/edit. Lets UI/MCP say "reopened because LIF-42 reopened"
    -- instead of looking like a manual un-check (LIF-167).
    reopened_via_issue_at TEXT,
    created_at            TEXT    NOT NULL DEFAULT (datetime('now')),
    edited_at             TEXT
);

CREATE INDEX IF NOT EXISTS idx_plans_project      ON plans(project_id);
CREATE INDEX IF NOT EXISTS idx_plans_issue        ON plans(issue_id);
CREATE INDEX IF NOT EXISTS idx_plan_steps_plan    ON plan_steps(plan_id);
CREATE INDEX IF NOT EXISTS idx_plan_steps_parent  ON plan_steps(parent_step_id);
CREATE INDEX IF NOT EXISTS idx_plan_steps_issue   ON plan_steps(issue_id);  -- cascade index

-- Keep plans.updated_at fresh on direct edits (mirrors projects/issues/pages).
CREATE TRIGGER IF NOT EXISTS plans_updated AFTER UPDATE ON plans BEGIN
    UPDATE plans SET updated_at = datetime('now') WHERE id = NEW.id;
END;
