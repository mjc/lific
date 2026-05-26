-- Backfill: any project with no lead gets the first admin as lead.
-- Fixes LIF-102: projects created before fix #1 (default-creator-as-lead) are
-- stuck in an unreachable state where no non-admin can edit them because
-- require_project_lead compares Some(user.id) to None and always rejects.
-- Setting an admin as lead keeps them recoverable; new projects default via
-- the API/MCP create paths.
UPDATE projects
SET lead_user_id = (SELECT id FROM users WHERE is_admin = 1 ORDER BY id ASC LIMIT 1)
WHERE lead_user_id IS NULL
  AND EXISTS (SELECT 1 FROM users WHERE is_admin = 1);
