-- LIF-199: audit-log coverage for project membership changes (epic
-- LIF-194, design LIF-DOC-7 decision #14 — membership management is
-- REST-only, but its mutations flow through the same query layer as
-- everything else, so they're captured the same way: SQL triggers on the
-- table itself (migration 018 pattern), not manual inserts from the API
-- handler. Whoever's stamped on `_actor_state` (see src/actor.rs) at write
-- time is attributed automatically, same as every other entity.
--
-- `project_members` has no surrogate id (composite PK project_id+user_id),
-- so `entity_id` stores the affected user's id and `entity_label` snapshots
-- their username at write time — mirrors how `audit_projects_*` uses
-- `identifier` as the label for a table with a natural key. One row per
-- lifecycle event (create/delete); one row per changed field on update
-- (`role` is the only auditable column here).

CREATE TRIGGER IF NOT EXISTS audit_project_members_insert AFTER INSERT ON project_members BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'member', NEW.user_id,
        (SELECT username FROM users WHERE id = NEW.user_id),
        NEW.project_id, 'create', NEW.role
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_project_members_delete AFTER DELETE ON project_members BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'member', OLD.user_id,
        (SELECT username FROM users WHERE id = OLD.user_id),
        OLD.project_id, 'delete', OLD.role
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_project_members_update AFTER UPDATE ON project_members BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'member', NEW.user_id,
           (SELECT username FROM users WHERE id = NEW.user_id),
           NEW.project_id, 'update', 'role', OLD.role, NEW.role
    WHERE OLD.role IS NOT NEW.role;
END;
