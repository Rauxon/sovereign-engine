# ADR 025: Soft-delete for API tokens

**Status:** Accepted
**Date:** 2026-02-18

## Context

Users need to remove revoked or unwanted tokens from their token list. However, the `usage_log` table references `tokens.id` via a foreign key, so hard-deleting a token row would either violate the FK constraint or orphan usage records, making historical reporting inaccurate.

## Decision

Add a `deleted_at TEXT` column to the `tokens` table. The `DELETE /api/user/tokens/:id` endpoint sets `deleted_at = datetime('now')` and also sets `revoked = 1` (ensuring the token is immediately unusable). The token list query (`GET /api/user/tokens`) filters out rows where `deleted_at IS NOT NULL`.

Internal and meta tokens are excluded from user deletion.

## Consequences

- Token rows are preserved, so `usage_log` joins remain valid and historical usage reports are unaffected.
- The user-facing token list stays clean — deleted tokens no longer appear.
- Deletion is irreversible from the UI (no "undo"), but the row still exists in the database for audit/support purposes.
- The `deleted_at` column can be used in future admin tooling to surface deletion history if needed.
