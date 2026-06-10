-- LIF-155: audit log — append-only history of every mutation, captured by
-- triggers so all write surfaces (REST, MCP, CLI, future code) are covered
-- without touching the query layer.
--
-- ── Actor attribution ──────────────────────────────────────────────────
-- Triggers can't read connection state or temp tables (SQLite resolves
-- trigger-body table refs against the trigger's own database), so actor
-- identity flows through `_actor_state`: a one-row table the application
-- stamps on the exclusive write connection before each request's writes
-- (see DbPool::write). Single-writer architecture makes this race-free.
-- An unstamped connection (migrations, startup) reads as 'system'.
--
-- ── Design rules ───────────────────────────────────────────────────────
-- * No foreign keys: history must outlive the entities (and users) it
--   describes. Joins at read time are LEFT and degrade gracefully.
-- * One row per changed field on UPDATE (action='update', field set),
--   one row per lifecycle event otherwise (create/delete/attach/detach/
--   link/unlink — field NULL except relations, where field carries the
--   relation type).
-- * `updated_at` / `sort_order` / `sequence` are never audited: pure
--   noise (and the 001/017 bump triggers would generate junk rows).
-- * `entity_label` snapshots a human identifier (LIF-42, page/module
--   name) so deleted entities still render in feeds.
-- * module_id / folder_id / lead_user_id changes store resolved NAMES,
--   not ids — the feed reads "module: Web UI → Core" even after the
--   module is gone.
-- * recursive_triggers is OFF (SQLite default): the updated_at bump
--   triggers (001, 017) run inside trigger bodies and therefore cannot
--   re-fire these audit triggers.

CREATE TABLE IF NOT EXISTS _actor_state (
    id        INTEGER PRIMARY KEY CHECK (id = 1),
    user_id   INTEGER,
    transport TEXT NOT NULL DEFAULT 'system'
);
INSERT OR IGNORE INTO _actor_state (id, user_id, transport) VALUES (1, NULL, 'system');

CREATE TABLE IF NOT EXISTS audit_log (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    ts            TEXT    NOT NULL DEFAULT (datetime('now')),
    actor_user_id INTEGER,           -- users.id at write time; no FK on purpose
    transport     TEXT    NOT NULL,  -- web | mcp | api | cli | system
    entity_type   TEXT    NOT NULL,  -- project|module|label|issue|page|comment|folder
    entity_id     INTEGER NOT NULL,
    entity_label  TEXT,              -- 'LIF-42', 'LIF-DOC-3', module/label/folder name
    project_id    INTEGER,           -- denormalized scope for project feeds
    issue_id      INTEGER,           -- parent issue (comments, labels, relations)
    page_id       INTEGER,           -- parent page  (comments, labels)
    action        TEXT    NOT NULL,  -- create|update|delete|attach|detach|link|unlink
    field         TEXT,
    old_value     TEXT,
    new_value     TEXT
);

CREATE INDEX IF NOT EXISTS idx_audit_entity  ON audit_log(entity_type, entity_id, id);
CREATE INDEX IF NOT EXISTS idx_audit_project ON audit_log(project_id, id);
CREATE INDEX IF NOT EXISTS idx_audit_issue   ON audit_log(issue_id, id);
CREATE INDEX IF NOT EXISTS idx_audit_page    ON audit_log(page_id, id);
CREATE INDEX IF NOT EXISTS idx_audit_ts      ON audit_log(ts);

-- ════════════════════════════════════════════════════════════════════
-- Issues
-- ════════════════════════════════════════════════════════════════════

CREATE TRIGGER IF NOT EXISTS audit_issues_insert AFTER INSERT ON issues BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'issue', NEW.id,
        (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-' || NEW.sequence,
        NEW.project_id, NEW.id, 'create', NEW.title
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_issues_delete AFTER DELETE ON issues BEGIN
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

CREATE TRIGGER IF NOT EXISTS audit_issues_update AFTER UPDATE ON issues BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'issue', NEW.id,
           (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-' || NEW.sequence,
           NEW.project_id, NEW.id, 'update', 'title', OLD.title, NEW.title
    WHERE OLD.title IS NOT NEW.title;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'issue', NEW.id,
           (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-' || NEW.sequence,
           NEW.project_id, NEW.id, 'update', 'description', OLD.description, NEW.description
    WHERE OLD.description IS NOT NEW.description;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'issue', NEW.id,
           (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-' || NEW.sequence,
           NEW.project_id, NEW.id, 'update', 'status', OLD.status, NEW.status
    WHERE OLD.status IS NOT NEW.status;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'issue', NEW.id,
           (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-' || NEW.sequence,
           NEW.project_id, NEW.id, 'update', 'priority', OLD.priority, NEW.priority
    WHERE OLD.priority IS NOT NEW.priority;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'issue', NEW.id,
           (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-' || NEW.sequence,
           NEW.project_id, NEW.id, 'update', 'module',
           (SELECT name FROM modules WHERE id = OLD.module_id),
           (SELECT name FROM modules WHERE id = NEW.module_id)
    WHERE OLD.module_id IS NOT NEW.module_id;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'issue', NEW.id,
           (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-' || NEW.sequence,
           NEW.project_id, NEW.id, 'update', 'start_date', OLD.start_date, NEW.start_date
    WHERE OLD.start_date IS NOT NEW.start_date;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'issue', NEW.id,
           (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-' || NEW.sequence,
           NEW.project_id, NEW.id, 'update', 'target_date', OLD.target_date, NEW.target_date
    WHERE OLD.target_date IS NOT NEW.target_date;
END;

-- ════════════════════════════════════════════════════════════════════
-- Pages (identifier scheme: PROJ-DOC-n, or DOC-n for workspace pages)
-- ════════════════════════════════════════════════════════════════════

CREATE TRIGGER IF NOT EXISTS audit_pages_insert AFTER INSERT ON pages BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, page_id, action, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'page', NEW.id,
        CASE WHEN NEW.project_id IS NULL THEN 'DOC-' || NEW.sequence
             ELSE (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-DOC-' || NEW.sequence END,
        NEW.project_id, NEW.id, 'create', NEW.title
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_pages_delete AFTER DELETE ON pages BEGIN
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

CREATE TRIGGER IF NOT EXISTS audit_pages_update AFTER UPDATE ON pages BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, page_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'page', NEW.id,
           CASE WHEN NEW.project_id IS NULL THEN 'DOC-' || NEW.sequence
                ELSE (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-DOC-' || NEW.sequence END,
           NEW.project_id, NEW.id, 'update', 'title', OLD.title, NEW.title
    WHERE OLD.title IS NOT NEW.title;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, page_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'page', NEW.id,
           CASE WHEN NEW.project_id IS NULL THEN 'DOC-' || NEW.sequence
                ELSE (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-DOC-' || NEW.sequence END,
           NEW.project_id, NEW.id, 'update', 'content', OLD.content, NEW.content
    WHERE OLD.content IS NOT NEW.content;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, page_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'page', NEW.id,
           CASE WHEN NEW.project_id IS NULL THEN 'DOC-' || NEW.sequence
                ELSE (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-DOC-' || NEW.sequence END,
           NEW.project_id, NEW.id, 'update', 'status', OLD.status, NEW.status
    WHERE OLD.status IS NOT NEW.status;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, page_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'page', NEW.id,
           CASE WHEN NEW.project_id IS NULL THEN 'DOC-' || NEW.sequence
                ELSE (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-DOC-' || NEW.sequence END,
           NEW.project_id, NEW.id, 'update', 'folder',
           (SELECT name FROM folders WHERE id = OLD.folder_id),
           (SELECT name FROM folders WHERE id = NEW.folder_id)
    WHERE OLD.folder_id IS NOT NEW.folder_id;
END;

-- ════════════════════════════════════════════════════════════════════
-- Comments (issue_id XOR page_id; entity_label = the parent's identifier)
-- ════════════════════════════════════════════════════════════════════

CREATE TRIGGER IF NOT EXISTS audit_comments_insert AFTER INSERT ON comments BEGIN
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
        NEW.issue_id, NEW.page_id, 'create', NEW.content
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_comments_update AFTER UPDATE OF content ON comments
WHEN OLD.content IS NOT NEW.content
BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, page_id, action, field, old_value, new_value)
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
        NEW.issue_id, NEW.page_id, 'update', 'content', OLD.content, NEW.content
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_comments_delete AFTER DELETE ON comments BEGIN
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

-- ════════════════════════════════════════════════════════════════════
-- Label attach / detach (issues + pages). Feed rows live on the parent.
-- ════════════════════════════════════════════════════════════════════

CREATE TRIGGER IF NOT EXISTS audit_issue_labels_attach AFTER INSERT ON issue_labels BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'issue', NEW.issue_id,
        (SELECT p.identifier || '-' || i.sequence FROM issues i JOIN projects p ON p.id = i.project_id WHERE i.id = NEW.issue_id),
        (SELECT project_id FROM issues WHERE id = NEW.issue_id),
        NEW.issue_id, 'attach', 'label',
        (SELECT name FROM labels WHERE id = NEW.label_id)
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_issue_labels_detach AFTER DELETE ON issue_labels BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'issue', OLD.issue_id,
        (SELECT p.identifier || '-' || i.sequence FROM issues i JOIN projects p ON p.id = i.project_id WHERE i.id = OLD.issue_id),
        (SELECT project_id FROM issues WHERE id = OLD.issue_id),
        OLD.issue_id, 'detach', 'label',
        (SELECT name FROM labels WHERE id = OLD.label_id)
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_page_labels_attach AFTER INSERT ON page_labels BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, page_id, action, field, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'page', NEW.page_id,
        (SELECT CASE WHEN pg.project_id IS NULL THEN 'DOC-' || pg.sequence
                     ELSE pr.identifier || '-DOC-' || pg.sequence END
         FROM pages pg LEFT JOIN projects pr ON pr.id = pg.project_id WHERE pg.id = NEW.page_id),
        (SELECT project_id FROM pages WHERE id = NEW.page_id),
        NEW.page_id, 'attach', 'label',
        (SELECT name FROM labels WHERE id = NEW.label_id)
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_page_labels_detach AFTER DELETE ON page_labels BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, page_id, action, field, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'page', OLD.page_id,
        (SELECT CASE WHEN pg.project_id IS NULL THEN 'DOC-' || pg.sequence
                     ELSE pr.identifier || '-DOC-' || pg.sequence END
         FROM pages pg LEFT JOIN projects pr ON pr.id = pg.project_id WHERE pg.id = OLD.page_id),
        (SELECT project_id FROM pages WHERE id = OLD.page_id),
        OLD.page_id, 'detach', 'label',
        (SELECT name FROM labels WHERE id = OLD.label_id)
    );
END;

-- ════════════════════════════════════════════════════════════════════
-- Issue relations. Recorded against the SOURCE issue; field carries the
-- relation type, value carries the target identifier.
-- ════════════════════════════════════════════════════════════════════

CREATE TRIGGER IF NOT EXISTS audit_relations_link AFTER INSERT ON issue_relations BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'issue', NEW.source_id,
        (SELECT p.identifier || '-' || i.sequence FROM issues i JOIN projects p ON p.id = i.project_id WHERE i.id = NEW.source_id),
        (SELECT project_id FROM issues WHERE id = NEW.source_id),
        NEW.source_id, 'link', NEW.relation_type,
        (SELECT p.identifier || '-' || i.sequence FROM issues i JOIN projects p ON p.id = i.project_id WHERE i.id = NEW.target_id)
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_relations_unlink AFTER DELETE ON issue_relations BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'issue', OLD.source_id,
        (SELECT p.identifier || '-' || i.sequence FROM issues i JOIN projects p ON p.id = i.project_id WHERE i.id = OLD.source_id),
        (SELECT project_id FROM issues WHERE id = OLD.source_id),
        OLD.source_id, 'unlink', OLD.relation_type,
        (SELECT p.identifier || '-' || i.sequence FROM issues i JOIN projects p ON p.id = i.project_id WHERE i.id = OLD.target_id)
    );
END;

-- ════════════════════════════════════════════════════════════════════
-- Projects
-- ════════════════════════════════════════════════════════════════════

CREATE TRIGGER IF NOT EXISTS audit_projects_insert AFTER INSERT ON projects BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'project', NEW.id, NEW.identifier, NEW.id, 'create', NEW.name
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_projects_delete AFTER DELETE ON projects BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'project', OLD.id, OLD.identifier, OLD.id, 'delete', OLD.name
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_projects_update AFTER UPDATE ON projects BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'project', NEW.id, NEW.identifier, NEW.id, 'update', 'name', OLD.name, NEW.name
    WHERE OLD.name IS NOT NEW.name;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'project', NEW.id, NEW.identifier, NEW.id, 'update', 'identifier', OLD.identifier, NEW.identifier
    WHERE OLD.identifier IS NOT NEW.identifier;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'project', NEW.id, NEW.identifier, NEW.id, 'update', 'description', OLD.description, NEW.description
    WHERE OLD.description IS NOT NEW.description;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'project', NEW.id, NEW.identifier, NEW.id, 'update', 'emoji', OLD.emoji, NEW.emoji
    WHERE OLD.emoji IS NOT NEW.emoji;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'project', NEW.id, NEW.identifier, NEW.id, 'update', 'lead',
           (SELECT username FROM users WHERE id = OLD.lead_user_id),
           (SELECT username FROM users WHERE id = NEW.lead_user_id)
    WHERE OLD.lead_user_id IS NOT NEW.lead_user_id;
END;

-- ════════════════════════════════════════════════════════════════════
-- Modules
-- ════════════════════════════════════════════════════════════════════

CREATE TRIGGER IF NOT EXISTS audit_modules_insert AFTER INSERT ON modules BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'module', NEW.id, NEW.name, NEW.project_id, 'create', NEW.name
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_modules_delete AFTER DELETE ON modules BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'module', OLD.id, OLD.name, OLD.project_id, 'delete', OLD.name
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_modules_update AFTER UPDATE ON modules BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'module', NEW.id, NEW.name, NEW.project_id, 'update', 'name', OLD.name, NEW.name
    WHERE OLD.name IS NOT NEW.name;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'module', NEW.id, NEW.name, NEW.project_id, 'update', 'description', OLD.description, NEW.description
    WHERE OLD.description IS NOT NEW.description;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'module', NEW.id, NEW.name, NEW.project_id, 'update', 'status', OLD.status, NEW.status
    WHERE OLD.status IS NOT NEW.status;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'module', NEW.id, NEW.name, NEW.project_id, 'update', 'emoji', OLD.emoji, NEW.emoji
    WHERE OLD.emoji IS NOT NEW.emoji;
END;

-- ════════════════════════════════════════════════════════════════════
-- Labels (the definitions themselves) + folders
-- ════════════════════════════════════════════════════════════════════

CREATE TRIGGER IF NOT EXISTS audit_labels_insert AFTER INSERT ON labels BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'label', NEW.id, NEW.name, NEW.project_id, 'create', NEW.name
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_labels_delete AFTER DELETE ON labels BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'label', OLD.id, OLD.name, OLD.project_id, 'delete', OLD.name
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_folders_insert AFTER INSERT ON folders BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'folder', NEW.id, NEW.name, NEW.project_id, 'create', NEW.name
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_folders_delete AFTER DELETE ON folders BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'folder', OLD.id, OLD.name, OLD.project_id, 'delete', OLD.name
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_folders_update AFTER UPDATE OF name ON folders
WHEN OLD.name IS NOT NEW.name
BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'folder', NEW.id, NEW.name, NEW.project_id, 'update', 'name', OLD.name, NEW.name
    );
END;
