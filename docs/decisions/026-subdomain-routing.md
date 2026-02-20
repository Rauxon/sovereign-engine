# ADR 026: Subdomain-based routing

**Status:** Accepted
**Date:** 2026-02-20
**Supersedes:** [ADR 002 — Portal subpath](002-portal-subpath.md) (subpath routing replaced by subdomain routing)

## Context

Sovereign Engine serves two distinct web UIs plus API routes, all on a single host:port. Open WebUI (chat interface) lives at `/*` and the React portal at `/portal/*`. Mixing these on a single hostname creates several problems:

- **Cookie scope** — session cookies set for the root path are visible to both UIs, but Open WebUI and the portal have different security contexts.
- **CORS** — a single origin means both UIs share the same cross-origin policy.
- **Separation of concerns** — operators want to point users at a chat URL (e.g. `chat.example.com`) without exposing the admin portal.

## Decision

Split routing into two subdomains, both terminating at the same Rust proxy process:

- **`chat.<domain>`** — Open WebUI reverse proxy. Session-authenticated with redirect to portal for login.
- **`api.<domain>`** — Portal UI (`/portal/*`), OIDC auth routes (`/auth/*`), admin/user APIs (`/api/*`), OpenAI-compatible API (`/v1/*`).

### Configuration

Three new env vars replace `EXTERNAL_URL` and `ACME_DOMAIN`:

| Variable | Example | Purpose |
|---|---|---|
| `API_HOSTNAME` | `api.example.com` | Hostname for API/portal subdomain |
| `CHAT_HOSTNAME` | `chat.example.com` | Hostname for Open WebUI subdomain |
| `COOKIE_DOMAIN` | `.example.com` | Shared cookie domain for cross-subdomain sessions |

When both hostnames default to `localhost` (unconfigured), the proxy falls back to the pre-subdomain combined router for backwards-compatible dev mode.

### Host dispatch

A middleware layer reads the `Host` header (stripping port) and routes:
- `chat_hostname` match → chat router (Open WebUI proxy)
- `api_hostname` match → API router
- No match → 421 Misdirected Request

### Cookie sharing

The `Domain` attribute is appended to session cookies when `COOKIE_DOMAIN` is set (e.g. `Domain=.example.com`). This allows a session created by the OIDC callback on `api.<domain>` to be valid on `chat.<domain>`.

### ACME

ACME now provisions a multi-SAN certificate covering both hostnames. The `ACME_DOMAIN` env var is removed; domains are derived from `API_HOSTNAME` + `CHAT_HOSTNAME` when `ACME_CONTACT` is set.

### OIDC

- Callback URL: `https://api.<domain>/auth/callback` (OIDC routes live on the API subdomain)
- Post-login redirect: `https://chat.<domain>/` (users land in Open WebUI after login)

## Consequences

- **Positive:** Clean separation of chat and admin concerns. Operators can restrict portal access at the DNS/network level. Cookie domain scoping is explicit.
- **Positive:** Dev mode unchanged — both hostnames default to `localhost`, combined router preserved.
- **Negative:** Production deployments require DNS records for both subdomains pointing to the same IP. Wildcard DNS or two A records.
- **Negative:** OIDC provider redirect URI must be updated to `https://api.<domain>/auth/callback`.
