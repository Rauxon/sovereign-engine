-- Per-container secrets: UID allocation + API key for backend auth.
-- Inserted on container start, deleted on container stop.
-- Allows the proxy to recover credentials after restart.
CREATE TABLE IF NOT EXISTS container_secrets (
    model_id TEXT PRIMARY KEY NOT NULL REFERENCES models(id),
    container_uid INTEGER NOT NULL,
    api_key TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
