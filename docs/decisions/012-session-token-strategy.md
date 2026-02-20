# ADR 012: Session token strategy

**Status:** Accepted
**Date:** 2026-02-17

## Context

Session management for the React dashboard requires secure cookie-based authentication. Tokens stored in the database must not be useful if the database is compromised.

## Decision

Generate session tokens as random 32-byte hex strings. Hash with SHA-256 before storing in the `sessions` table. Set 24-hour TTL (`expires_at` column). Cookies are `HttpOnly`, `SameSite=Lax`, and `Secure` (configurable via `SECURE_COOKIES` for HTTP dev). When `COOKIE_DOMAIN` is set (e.g. `.example.com`), cookies include a `Domain` attribute for cross-subdomain sharing between the API and chat subdomains (see [ADR 026](026-subdomain-routing.md)). An hourly cleanup task deletes expired sessions. Cookie name: `se_session`.

## Consequences

- **Positive:** Database leak does not reveal session tokens (SHA-256 is one-way). 24h TTL limits exposure window. HttpOnly prevents XSS exfiltration
- **Negative:** SHA-256 is fast to brute-force compared to bcrypt, but session tokens have 128 bits of entropy (hex-encoded 32 bytes), making brute-force infeasible
