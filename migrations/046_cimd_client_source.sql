-- Keep Client ID Metadata Documents revalidatable (LIF-47).
-- A CIMD row is a cache of untrusted metadata, not a pre-registered client.
ALTER TABLE oauth_clients ADD COLUMN registration_source TEXT NOT NULL DEFAULT 'dcr';
