# ADR 002: React SPA at /portal subpath

**Status:** Superseded by [ADR 026 — Subdomain routing](026-subdomain-routing.md)
**Date:** 2026-02-13

## Context

Sovereign Engine serves two web UIs:
1. The built-in React SPA (admin dashboard, token management, reservations)
2. Open WebUI (chat interface, proxied from a separate container)

Open WebUI assumes it owns the root path (`/`). It uses hardcoded paths for assets, API calls, and routing that cannot be configured to run on a subpath.

The React SPA, being our own code, can be configured to run on any base path via Vite's `base` option.

## Decision

Serve the React SPA at `/portal/*` with Vite `base: '/portal/'`. Use the root path (`/*`) as a fallback that reverse-proxies to Open WebUI.

Route priority in the axum router:
1. `/auth/*` — OIDC auth routes
2. `/api/*` — Admin/user API
3. `/v1/*` — OpenAI-compatible API
4. `/portal/*` — React SPA (static file serving with SPA fallback)
5. `/*` (fallback) — Open WebUI reverse proxy

## Consequences

- **Positive:** Both UIs work without conflicts. Open WebUI gets its expected root path. The React SPA is explicitly namespaced.
- **Negative:** Users must navigate to `/portal` for the admin dashboard (not `/`). The root URL shows Open WebUI, which may confuse first-time users expecting the admin panel.
- **Mitigation:** Login page at `/auth/providers` and session redirect logic guide users to the right place.
