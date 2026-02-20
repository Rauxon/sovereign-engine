# ADR 019: OIDC Callback URL

**Status:** Accepted
**Date:** 2026-02-17

## Context
OIDC login requires a callback URL that the Identity Provider redirects to after authentication. This URL must match exactly what is registered with the IdP. The proxy may be accessed via different hostnames or ports depending on deployment (development vs production, behind a reverse proxy, etc.).

## Decision
Derive the OIDC callback URL at runtime from the `API_HOSTNAME` configuration: `https://api.<domain>/auth/callback` (see [ADR 026](026-subdomain-routing.md)). The scheme is derived from `SECURE_COOKIES` (true → https, false → http). CORS allows both the API and chat subdomain origins.

The callback URL is generated during the auth request flow and must match the IdP's registered redirect URI. After successful login, the user is redirected to the chat subdomain (`https://chat.<domain>/`).

## Consequences
- **Positive:** OIDC auth routes live on the API subdomain alongside the portal, keeping auth and admin on one origin.
- **Positive:** Post-login redirect sends users directly to the chat interface.
- **Negative:** Changing `API_HOSTNAME` requires updating the IdP's registered redirect URI. Documented in DEPLOYMENT.md.
