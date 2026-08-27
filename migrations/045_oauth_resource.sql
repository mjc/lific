-- RFC 8707 resource indicators for MCP OAuth grants (LIF-47).
-- Keep the columns nullable so existing credentials remain readable by the
-- non-MCP compatibility resolver; newly issued MCP grants always populate it.
ALTER TABLE oauth_codes ADD COLUMN resource TEXT;
ALTER TABLE oauth_tokens ADD COLUMN resource TEXT;
ALTER TABLE oauth_device_codes ADD COLUMN resource TEXT;
