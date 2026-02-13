# Security Policy

## Reporting Vulnerabilities

If you discover a security vulnerability, please report it responsibly:

**Email:** graham@rostron-wood.org

Please include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

We will acknowledge receipt within 48 hours and aim to provide an initial assessment within 5 business days.

**Do not** open a public GitHub issue for security vulnerabilities.

## Scope

The following are in scope:
- Authentication and session management (OIDC, bootstrap auth, API tokens)
- Authorization bypasses (admin-only routes, reservation access controls)
- Secret handling (IdP client secrets, API token hashing, DB encryption)
- Container isolation (UID allocation, network isolation, Docker socket access)
- Input validation (API endpoints, SQL injection, path traversal)
- CSP and XSS prevention

The following are out of scope:
- Vulnerabilities in upstream dependencies (report to the dependency maintainer)
- Denial-of-service via resource exhaustion (this is a single-tenant system)
- Social engineering

## Security Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for details on:
- Network isolation (dual Docker network topology)
- Per-container UID isolation
- Auth flows (OIDC with PKCE, session management, bearer tokens)
- Secret encryption at rest (AES-256-GCM for IdP client secrets)

## Known Accepted Risks

- **Docker socket access:** The proxy requires `/var/run/docker.sock` to manage backend containers. This grants container-level privileges. Mitigated by running the proxy in a dedicated container on an isolated host.
- **Bootstrap credentials:** When `BREAK_GLASS=true`, the admin username/password are passed as environment variables. Intended only for initial setup or emergency access recovery.
- **SQLite:** Single-writer database. Acceptable for the single-tenant deployment model; not suitable for high-concurrency multi-tenant use.
