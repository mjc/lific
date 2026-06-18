-- LIF-215: single-user web auto-login.
--
-- When enabled, the web UI silently authenticates as the first admin account
-- so a solo operator never sees a sign-in screen. Scoped to the browser only —
-- REST and MCP still require real bearer tokens.
--
-- DANGEROUS on a publicly-reachable instance: anyone who can load the page
-- becomes admin. Off by default; admin-only to enable.
ALTER TABLE instance_settings ADD COLUMN web_auto_login INTEGER NOT NULL DEFAULT 0;
