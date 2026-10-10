-- Bind new OAuth grants and tokens to their resource. NULL identifies grants
-- issued before resource binding; they retain compatibility until expiry.
ALTER TABLE oauth_codes ADD COLUMN resource TEXT;
ALTER TABLE oauth_device_codes ADD COLUMN resource TEXT;
ALTER TABLE oauth_tokens ADD COLUMN resource TEXT;
ALTER TABLE oauth_clients ADD COLUMN application_type TEXT NOT NULL DEFAULT 'web';
