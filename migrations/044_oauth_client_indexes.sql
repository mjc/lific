-- Index the client_id side of the OAuth code and token tables.
--
-- Both tables are keyed by their secret (the authorization code, the hashed
-- access token), so every access by *client* was a full table scan. That is
-- the direction the registration bounds probe: pruning abandoned anonymous
-- clients runs two correlated `NOT EXISTS` subqueries, one per table, for
-- each candidate client, which turns a cleanup sweep quadratic exactly when
-- the tables are largest. Issuing a token also reads the code row's client,
-- and revocation walks a client's tokens.
--
-- Non-unique on purpose: a client legitimately holds many codes and tokens.

CREATE INDEX IF NOT EXISTS idx_oauth_codes_client  ON oauth_codes(client_id);
CREATE INDEX IF NOT EXISTS idx_oauth_tokens_client ON oauth_tokens(client_id);
