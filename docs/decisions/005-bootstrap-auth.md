# ADR 005: Break-glass bootstrap authentication

**Status:** Accepted
**Date:** 2026-02-12

## Context

Sovereign Engine uses OIDC for authentication. But on first deployment (or after losing all IdP configurations), there is no way to log in to configure an IdP — a chicken-and-egg problem.

## Decision

Provide a "break-glass" bootstrap authentication mechanism:

- Enabled via `BREAK_GLASS=true` environment variable
- Uses `BOOTSTRAP_USER` and `BOOTSTRAP_PASSWORD` env vars for credentials
- Accepts HTTP Basic Auth on `/api/*` and `/auth/me` routes
- On `/auth/me`, silently creates a session cookie so the React SPA can function normally (the SPA uses session cookies, not Basic Auth)
- The bootstrap user is always treated as admin

## Consequences

- **Positive:** Zero-dependency initial setup. Emergency access recovery if all IdPs are misconfigured.
- **Negative:** Credentials passed as environment variables (visible in Docker inspect, process environment). Mitigated by: (1) intended only for initial setup, (2) should be disabled (`BREAK_GLASS=false`) once OIDC is configured, (3) session created silently means the password isn't sent on every request after initial login.
