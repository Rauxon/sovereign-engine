-- Soft-delete support for user-facing tokens.
-- deleted_at is set on delete; the token row is preserved for usage_log integrity.
ALTER TABLE tokens ADD COLUMN deleted_at TEXT;
