-- LIF-167: issue <-> step / plan <-> anchor cascade. Closing flows DOWN from
-- issues, never UP from plans.
--
-- These triggers fire on a DIRECT `UPDATE issues SET status=...`. With
-- recursive_triggers OFF (SQLite default, see 018), the UPDATE/INSERT
-- statements inside these bodies do NOT re-fire other triggers — so the
-- step/plan changes here will NOT trip the direct-mutation audit triggers in
-- 021. That's why each cascade audits ITSELF inline (the only place that can),
-- attributed to whoever closed/reopened the issue (the true cause), with a
-- distinct action so it reads as system-driven via the issue (LIF-176).
--
-- Freeze rule: cascades only touch ACTIVE plans. done/archived plans are frozen.

-- ── Issue marked done  →  complete referencing steps in active plans ────
CREATE TRIGGER IF NOT EXISTS plans_issue_done_cascade
AFTER UPDATE OF status ON issues
WHEN NEW.status = 'done' AND OLD.status <> 'done'
BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'plan_step', ps.id,
           (SELECT identifier FROM projects WHERE id = pl.project_id) || '-PLAN-' || pl.sequence,
           pl.project_id, NEW.id, 'auto-complete', 'done', '0', '1'
    FROM plan_steps ps JOIN plans pl ON pl.id = ps.plan_id
    WHERE ps.issue_id = NEW.id AND ps.done = 0 AND pl.status = 'active';

    UPDATE plan_steps
       SET done = 1, reopened_via_issue_at = NULL, edited_at = datetime('now')
     WHERE issue_id = NEW.id AND done = 0
       AND plan_id IN (SELECT id FROM plans WHERE status = 'active');
END;

-- ── Issue reopened (done → not done)  →  un-complete steps, stamp reason ─
CREATE TRIGGER IF NOT EXISTS plans_issue_reopen_cascade
AFTER UPDATE OF status ON issues
WHEN OLD.status = 'done' AND NEW.status <> 'done'
BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'plan_step', ps.id,
           (SELECT identifier FROM projects WHERE id = pl.project_id) || '-PLAN-' || pl.sequence,
           pl.project_id, NEW.id, 'auto-reopen', 'done', '1', '0'
    FROM plan_steps ps JOIN plans pl ON pl.id = ps.plan_id
    WHERE ps.issue_id = NEW.id AND ps.done = 1 AND pl.status = 'active';

    UPDATE plan_steps
       SET done = 0, reopened_via_issue_at = datetime('now'), edited_at = datetime('now')
     WHERE issue_id = NEW.id AND done = 1
       AND plan_id IN (SELECT id FROM plans WHERE status = 'active');
END;

-- ── Anchor issue marked done  →  auto-archive the plan (never the reverse) ─
CREATE TRIGGER IF NOT EXISTS plans_anchor_done_archive
AFTER UPDATE OF status ON issues
WHEN NEW.status = 'done' AND OLD.status <> 'done'
BEGIN
    INSERT INTO audit_log (actor_user_id, transport, entity_type, entity_id, entity_label,
                           project_id, issue_id, action, field, old_value, new_value)
    SELECT (SELECT user_id FROM _actor_state WHERE id = 1),
           COALESCE((SELECT transport FROM _actor_state WHERE id = 1), 'system'),
           'plan', pl.id,
           (SELECT identifier FROM projects WHERE id = pl.project_id) || '-PLAN-' || pl.sequence,
           pl.project_id, NEW.id, 'auto-archive', 'status', 'active', 'archived'
    FROM plans pl
    WHERE pl.issue_id = NEW.id AND pl.status = 'active';

    UPDATE plans
       SET status = 'archived', updated_at = datetime('now')
     WHERE issue_id = NEW.id AND status = 'active';
END;
