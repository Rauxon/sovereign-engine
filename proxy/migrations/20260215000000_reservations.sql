CREATE TABLE reservations (
    id           TEXT PRIMARY KEY NOT NULL,
    user_id      TEXT NOT NULL REFERENCES users(id),
    status       TEXT NOT NULL DEFAULT 'pending'
                 CHECK(status IN ('pending','approved','active','completed','rejected','cancelled')),
    start_time   TEXT NOT NULL,  -- ISO 8601, 30-min boundary
    end_time     TEXT NOT NULL,  -- ISO 8601, 30-min boundary
    reason       TEXT NOT NULL DEFAULT '',
    admin_note   TEXT NOT NULL DEFAULT '',
    approved_by  TEXT REFERENCES users(id),
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_reservations_user   ON reservations(user_id);
CREATE INDEX idx_reservations_status ON reservations(status);
CREATE INDEX idx_reservations_time   ON reservations(start_time, end_time);
CREATE INDEX idx_reservations_active ON reservations(status, start_time, end_time)
    WHERE status IN ('approved', 'active');
