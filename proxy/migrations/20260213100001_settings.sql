-- Runtime-configurable settings (fairness tuning, queue timeouts, etc).
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Seed fairness defaults
INSERT OR IGNORE INTO settings (key, value) VALUES
    ('fairness_base_priority',    '100.0'),
    ('fairness_wait_weight',      '1.0'),
    ('fairness_usage_weight',     '10.0'),
    ('fairness_usage_scale',      '1000.0'),
    ('fairness_window_minutes',   '60'),
    ('queue_timeout_secs',        '30');
