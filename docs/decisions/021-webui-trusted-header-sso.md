# ADR 021: Open WebUI trusted-header SSO injection

**Status:** Accepted
**Date:** 2026-02-17

## Context

Sovereign Engine reverse-proxies Open WebUI at `/*`. Users authenticate via OIDC through the Sovereign Engine proxy. Without SSO, users would need to log in to Open WebUI separately, creating a confusing dual-auth experience.

Open WebUI supports trusted-header authentication: if a reverse proxy injects identity headers (e.g., `X-Remote-User`, `X-Remote-Email`), Open WebUI trusts them and auto-creates/logs in the user.

## Decision

The Open WebUI reverse proxy handler (`proxy/webui.rs`) injects the authenticated user's identity into upstream requests via trusted headers. The proxy only injects headers when the user has a valid session (enforced by `session_auth_redirect_middleware` on the `/*` fallback route). Open WebUI is configured to trust these headers since it sits on the isolated `sovereign-internal` network where only the proxy can reach it.

## Consequences

- **Positive:** Seamless single sign-on — users authenticate once via OIDC and are automatically logged in to Open WebUI. No separate user management in Open WebUI.
- **Negative:** Security relies on network isolation. If an attacker can reach Open WebUI directly (bypassing the proxy), they could forge trusted headers. Mitigated by the `sovereign-internal` network being `internal: true` (no host access) and Open WebUI having no host port bindings.
