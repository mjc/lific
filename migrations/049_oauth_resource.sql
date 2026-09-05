-- Bind OAuth grants and access tokens to the MCP protected resource (RFC 8707).
-- Nullable keeps existing pre-resource rows readable during migration; all new
-- authorization and device flows require and persist the canonical resource.
ALTER TABLE oauth_codes ADD COLUMN resource TEXT;
ALTER TABLE oauth_device_codes ADD COLUMN resource TEXT;
ALTER TABLE oauth_tokens ADD COLUMN resource TEXT;

CREATE INDEX IF NOT EXISTS idx_oauth_tokens_resource
    ON oauth_tokens(resource);
