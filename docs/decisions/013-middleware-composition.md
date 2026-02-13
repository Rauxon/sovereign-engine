# ADR 013: Middleware composition per route group

**Status:** Accepted
**Date:** 2026-02-17

## Context

Different route groups require different authentication strategies: public OIDC routes need no auth, the dashboard API needs session cookies, the OpenAI-compatible API needs bearer tokens, and admin routes need additional authorization checks.

## Decision

Compose middleware per route group using axum's nested router structure:

- `/auth/*` — no middleware (public OIDC login/callback)
- `/api/*` — `session_auth_middleware` (cookie or Basic auth)
- `/api/admin/*` — `session_auth_middleware` + `admin_only_middleware`
- `/v1/*` — `bearer_auth_middleware` (API tokens)
- `/portal/*` — no auth (static files)
- `/*` (fallback) — `session_auth_redirect_middleware` (redirects browsers to login, returns 401 for API clients)

Global layers applied bottom-up: `security_headers`, `TraceLayer`, `CompressionLayer`, `CorsLayer`.

## Consequences

- **Positive:** Each route group has exactly the auth it needs, no over- or under-protection. Axum's type system prevents accidental middleware omission
- **Negative:** Adding a new route group requires understanding the composition order. Documented in DEVELOPMENT.md middleware table
