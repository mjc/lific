-- LIF-81/LIF-158: never let unauthenticated requests scan every pre-010
-- Argon2 key. Those rows have no indexed key_id and must be rotated by their
-- owners; revocation makes the policy explicit before the public scan is cut.
UPDATE api_keys SET revoked = 1 WHERE key_id IS NULL AND revoked = 0;
