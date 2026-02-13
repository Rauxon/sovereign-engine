# ADR 019: OIDC Callback URL

**Status:** Accepted
**Date:** 2026-02-17

## Context
OIDC login requires a callback URL that the Identity Provider redirects to after authentication. This URL must match exactly what is registered with the IdP. The proxy may be accessed via different hostnames or ports depending on deployment (development vs production, behind a reverse proxy, etc.).

## Decision
Derive the OIDC callback URL at runtime from the `EXTERNAL_URL` environment variable: `{EXTERNAL_URL}/auth/callback`. This same `EXTERNAL_URL` is used to configure the CORS allowed origin. The callback URL is generated during the auth request flow and must match the IdP's registered redirect URI.

## Consequences
- **Positive:** Single configuration point for the proxy's public-facing URL. Works across all deployment scenarios (direct, behind reverse proxy, different ports). CORS and OIDC callback are always consistent.
- **Negative:** Misconfiguring `EXTERNAL_URL` causes both OIDC and CORS failures simultaneously, which can be confusing to debug. Documented in DEPLOYMENT.md.
