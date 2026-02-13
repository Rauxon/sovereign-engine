# ADR 022: Internal and meta token flags

**Status:** Accepted
**Date:** 2026-02-17

## Context

The `tokens` table serves multiple purposes: user-created API tokens for programmatic access, auto-provisioned tokens for internal integrations (e.g., Open WebUI's key for proxying to `/v1`), and bookkeeping tokens for per-user usage attribution (e.g., when Open WebUI creates per-user sub-tokens). Users listing their tokens should not see internal plumbing.

## Decision

Add two boolean flags to the `tokens` table:

- `internal` — tokens auto-provisioned by the system (e.g., the `WEBUI_API_KEY` token). Hidden from user token lists, cannot be revoked via the user API.
- `meta` — bookkeeping tokens created for usage attribution. Hidden from user token lists but usage is tracked against them for per-user analytics.

User-facing queries filter with `WHERE internal = 0 AND meta = 0`. Admin queries can see all tokens.

## Consequences

- **Positive:** Clean separation between user-visible tokens and system plumbing. Users see only tokens they created. Usage attribution works transparently.
- **Negative:** Two additional columns and filter conditions on every token query. Minor complexity cost, but prevents user confusion about tokens they didn't create.
