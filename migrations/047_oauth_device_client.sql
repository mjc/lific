-- Bind RFC 8628 device grants to the public client that requested them.
-- Existing short-lived grants remain NULL and fail closed at token exchange.
ALTER TABLE oauth_device_codes ADD COLUMN client_id TEXT;
