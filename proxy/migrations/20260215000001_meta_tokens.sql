-- Meta tokens: bookkeeping tokens for per-user Open WebUI usage attribution.
-- A meta token is never used for authentication — it exists solely so that
-- usage_log entries can reference a per-user token_id instead of the shared
-- internal (Open WebUI) token.

ALTER TABLE tokens ADD COLUMN meta INTEGER NOT NULL DEFAULT 0;

-- Email index needed for per-request user lookup in resolve_meta_user().
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
