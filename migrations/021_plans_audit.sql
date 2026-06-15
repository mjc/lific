-- LIF-176: audit-log coverage for DIRECT plan + plan_step mutations (the
-- cascade-driven changes are audited inline in 020). Mirrors the 018 pattern:
-- one row per lifecycle event, one row per changed field on update. Cascade
-- writes from 020's trigger bodies do NOT reach these triggers (recursive
-- triggers OFF), so there's no double-counting.
--
-- A plan's entity_label is PROJ-PLAN-n; a step borrows its plan's label.

-- ════════════════════════════════════════════════════════════════════
-- Plans
-- ════════════════════════════════════════════════════════════════════

CREATE TRIGGER IF NOT EXISTS audit_plans_insert AFTER INSERT ON plans BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'plan', NEW.id,
        (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-PLAN-' || NEW.sequence,
        NEW.project_id, NEW.issue_id, 'create', NEW.title
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_plans_delete AFTER DELETE ON plans BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'plan', OLD.id,
        (SELECT identifier FROM projects WHERE id = OLD.project_id) || '-PLAN-' || OLD.sequence,
        OLD.project_id, OLD.issue_id, 'delete', OLD.title
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_plans_update AFTER UPDATE ON plans BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'plan', NEW.id,
           (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-PLAN-' || NEW.sequence,
           NEW.project_id, 'update', 'title', OLD.title, NEW.title
    WHERE OLD.title IS NOT NEW.title;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'plan', NEW.id,
           (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-PLAN-' || NEW.sequence,
           NEW.project_id, 'update', 'status', OLD.status, NEW.status
    WHERE OLD.status IS NOT NEW.status;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'plan', NEW.id,
           (SELECT identifier FROM projects WHERE id = NEW.project_id) || '-PLAN-' || NEW.sequence,
           NEW.project_id, NEW.issue_id, 'update', 'anchor_issue',
           (SELECT pr.identifier || '-' || i.sequence FROM issues i JOIN projects pr ON pr.id = i.project_id WHERE i.id = OLD.issue_id),
           (SELECT pr.identifier || '-' || i.sequence FROM issues i JOIN projects pr ON pr.id = i.project_id WHERE i.id = NEW.issue_id)
    WHERE OLD.issue_id IS NOT NEW.issue_id;
END;

-- ════════════════════════════════════════════════════════════════════
-- Plan steps (entity_label = the parent plan's PROJ-PLAN-n)
-- ════════════════════════════════════════════════════════════════════

CREATE TRIGGER IF NOT EXISTS audit_plan_steps_insert AFTER INSERT ON plan_steps BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, new_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'plan_step', NEW.id,
        (SELECT pr.identifier || '-PLAN-' || pl.sequence FROM plans pl JOIN projects pr ON pr.id = pl.project_id WHERE pl.id = NEW.plan_id),
        (SELECT project_id FROM plans WHERE id = NEW.plan_id),
        NEW.issue_id, 'create', NEW.title
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_plan_steps_delete AFTER DELETE ON plan_steps BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, old_value)
    VALUES (
        (SELECT user_id FROM _actor_state WHERE id = 1),
        COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
        'plan_step', OLD.id,
        (SELECT pr.identifier || '-PLAN-' || pl.sequence FROM plans pl JOIN projects pr ON pr.id = pl.project_id WHERE pl.id = OLD.plan_id),
        (SELECT project_id FROM plans WHERE id = OLD.plan_id),
        OLD.issue_id, 'delete', OLD.title
    );
END;

CREATE TRIGGER IF NOT EXISTS audit_plan_steps_update AFTER UPDATE ON plan_steps BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'plan_step', NEW.id,
           (SELECT pr.identifier || '-PLAN-' || pl.sequence FROM plans pl JOIN projects pr ON pr.id = pl.project_id WHERE pl.id = NEW.plan_id),
           (SELECT project_id FROM plans WHERE id = NEW.plan_id),
           'update', 'title', OLD.title, NEW.title
    WHERE OLD.title IS NOT NEW.title;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'plan_step', NEW.id,
           (SELECT pr.identifier || '-PLAN-' || pl.sequence FROM plans pl JOIN projects pr ON pr.id = pl.project_id WHERE pl.id = NEW.plan_id),
           (SELECT project_id FROM plans WHERE id = NEW.plan_id),
           'update', 'description', OLD.description, NEW.description
    WHERE OLD.description IS NOT NEW.description;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'plan_step', NEW.id,
           (SELECT pr.identifier || '-PLAN-' || pl.sequence FROM plans pl JOIN projects pr ON pr.id = pl.project_id WHERE pl.id = NEW.plan_id),
           (SELECT project_id FROM plans WHERE id = NEW.plan_id),
           'update', 'done', CAST(OLD.done AS TEXT), CAST(NEW.done AS TEXT)
    WHERE OLD.done IS NOT NEW.done;

    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'plan_step', NEW.id,
           (SELECT pr.identifier || '-PLAN-' || pl.sequence FROM plans pl JOIN projects pr ON pr.id = pl.project_id WHERE pl.id = NEW.plan_id),
           (SELECT project_id FROM plans WHERE id = NEW.plan_id),
           NEW.issue_id, 'update', 'issue',
           (SELECT pr.identifier || '-' || i.sequence FROM issues i JOIN projects pr ON pr.id = i.project_id WHERE i.id = OLD.issue_id),
           (SELECT pr.identifier || '-' || i.sequence FROM issues i JOIN projects pr ON pr.id = i.project_id WHERE i.id = NEW.issue_id)
    WHERE OLD.issue_id IS NOT NEW.issue_id;
END;
