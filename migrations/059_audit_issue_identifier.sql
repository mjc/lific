-- Structured relation history resolves immutable issue identifiers even after
-- the target is purged or its project is renamed. These lookups run before
-- activity pagination and must not scan every issue audit for each reference.
CREATE INDEX IF NOT EXISTS idx_audit_issue_label
ON audit_log(entity_label COLLATE NOCASE, project_id)
WHERE entity_type = 'issue';
