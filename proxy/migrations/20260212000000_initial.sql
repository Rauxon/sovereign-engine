-- OIDC providers configured by admin
CREATE TABLE IF NOT EXISTS idp_configs (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    issuer TEXT NOT NULL,
    client_id TEXT NOT NULL,
    client_secret_enc TEXT NOT NULL,
    scopes TEXT NOT NULL DEFAULT 'openid email profile',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Model categories (thinking, coding, general, fast, etc.)
CREATE TABLE IF NOT EXISTS model_categories (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    preferred_model_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Concrete models (downloaded/available)
CREATE TABLE IF NOT EXISTS models (
    id TEXT PRIMARY KEY NOT NULL,
    hf_repo TEXT NOT NULL,
    filename TEXT,
    size_bytes INTEGER,
    category_id TEXT REFERENCES model_categories(id),
    loaded INTEGER NOT NULL DEFAULT 0,
    backend_port INTEGER,
    backend_type TEXT NOT NULL DEFAULT 'vllm',
    last_used_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Users (populated on first OIDC login)
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY NOT NULL,
    idp_id TEXT NOT NULL REFERENCES idp_configs(id),
    subject TEXT NOT NULL,
    email TEXT,
    display_name TEXT,
    is_admin INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(idp_id, subject)
);

-- API tokens
CREATE TABLE IF NOT EXISTS tokens (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id),
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    category_id TEXT REFERENCES model_categories(id),
    specific_model_id TEXT REFERENCES models(id),
    expires_at TEXT,
    revoked INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Usage log (every request)
CREATE TABLE IF NOT EXISTS usage_log (
    id TEXT PRIMARY KEY NOT NULL,
    token_id TEXT REFERENCES tokens(id),
    user_id TEXT NOT NULL REFERENCES users(id),
    model_id TEXT NOT NULL,
    category_id TEXT,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    latency_ms INTEGER NOT NULL DEFAULT 0,
    queued_ms INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Mapping: IdP group/attribute → model category access
CREATE TABLE IF NOT EXISTS idp_model_access (
    id TEXT PRIMARY KEY NOT NULL,
    idp_id TEXT NOT NULL REFERENCES idp_configs(id),
    group_claim TEXT NOT NULL,
    group_value TEXT NOT NULL,
    category_id TEXT NOT NULL REFERENCES model_categories(id)
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_tokens_hash ON tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_tokens_user ON tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_usage_log_user ON usage_log(user_id);
CREATE INDEX IF NOT EXISTS idx_usage_log_created ON usage_log(created_at);
CREATE INDEX IF NOT EXISTS idx_usage_log_token ON usage_log(token_id);
CREATE INDEX IF NOT EXISTS idx_users_idp_subject ON users(idp_id, subject);
CREATE INDEX IF NOT EXISTS idx_models_category ON models(category_id);

-- Session management for OIDC-authenticated portal users
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id),
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_sessions_hash ON sessions(token_hash);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);

-- Temporary OIDC authorization state (CSRF + nonce)
CREATE TABLE IF NOT EXISTS oidc_auth_state (
    csrf_token TEXT PRIMARY KEY NOT NULL,
    nonce TEXT NOT NULL,
    idp_id TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_oidc_auth_state_expires ON oidc_auth_state(expires_at);
