# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.1.0] - 2026-02-20

### Added
- Subdomain-based routing: `chat.<domain>` for Open WebUI, `api.<domain>` for portal and APIs (ADR 026)
- Host-based request dispatch via `Host` header with 421 Misdirected Request for unknown hosts
- Cross-subdomain cookie sharing via `Domain` attribute (`COOKIE_DOMAIN` env var)
- Multi-domain ACME certificate provisioning (SAN cert for both subdomains)
- DB encryption key rotation support via `DB_ENCRYPTION_KEY_OLD` env var
- Empty-key bug recovery: automatic migration from HKDF("") encrypted secrets
- 7 new migration tests covering all encryption key scenarios

### Changed
- OIDC callback redirects to `/portal/` instead of chat subdomain after login
- Session cookie validation now tries all matching cookies (handles stale duplicate cookies)
- Logout deletes all matching session cookies
- `EXTERNAL_URL` replaced by `API_HOSTNAME`, `CHAT_HOSTNAME`, `COOKIE_DOMAIN`
- `ACME_DOMAIN` removed; domains now derived from hostnames when `ACME_CONTACT` is set
- CORS allows both subdomain origins
- Dev mode preserved: when both hostnames are `localhost`, combined single-host router is used

### Fixed
- DB encryption: empty `DB_ENCRYPTION_KEY` was silently treated as a valid key, causing double-encryption when a real key was later set
- Token management: display bug, expiry support, soft delete
- API subdomain root (`/`) now redirects to `/portal/` instead of returning 404

### Security
- DB encryption key rotation without downtime (set old key, deploy new key, remove old key)
- Config now filters empty `DB_ENCRYPTION_KEY` to prevent accidental encryption with derived-from-nothing key

## [1.0.2] - 2026-02-18

### Added
- Comprehensive unit test suite: 92 Rust tests across 11 modules, 20 React tests
- Test coverage for security-critical paths: token hashing, encryption roundtrips, credential validation, CSP hash extraction
- Vitest + Testing Library setup for React UI

### Changed
- Reduced code duplication from 10.5% to 6.6%: extracted shared table/form styles, `TokenMintForm` component, and `proxy/src/api/common.rs` shared handlers
- Extracted `start_container_core()` and `post_stop_cleanup()` shared helpers to consolidate container lifecycle logic

## [1.0.1] - 2026-02-18

### Changed
- Converted all modal dialogs to native `<dialog>` element with `showModal()` for proper focus trapping, Escape key handling, and accessibility semantics
- Reduced Rust cognitive complexity: decomposed `download_single_file()` by extracting `hf_http_error_hint()` and `stream_response_to_file()` helpers
- Extracted shared `try_bootstrap_auth()` helper to eliminate duplicated Basic auth logic
- Replaced deprecated React 19 `FormEvent` usage with `SubmitEvent`
- Improved accessibility: proper label associations, keyboard support on interactive calendar elements, semantic HTML throughout
- Flattened deeply nested control flow in `fetch_tokenizer_config()` and `run_download()`
- Extracted `parse_drm_fdinfo_vram()` as a pure testable function

### Fixed
- Resolved 147 SonarQube code quality issues (code smells and bugs) across React UI and Rust proxy
- Fixed nested ternary expressions across multiple components
- Fixed missing `key` props using semantic identifiers instead of array indices
- Fixed non-interactive elements incorrectly receiving event handlers

## [1.0.0] - 2026-02-18

### Added
- GPU reservation system with admin approval workflow and automatic state transitions
- Per-container UID allocation and API key authentication for defence-in-depth
- AES-256-GCM encryption for IdP client secrets at rest
- Real-time metrics via SSE (GPU memory, CPU, disk, queue stats)
- Content Security Policy with SHA-256 inline script hashing
- Logarithmic fair-use scheduler with runtime-tunable parameters
- Concurrency gate with RAII slot management
- Meta tokens for Open WebUI per-user usage attribution
- Threat model documentation (docs/THREAT_MODEL.md)
- Architecture Decision Records (ADRs 001–024)
- CODE_OF_CONDUCT.md (Contributor Covenant)

### Changed
- Removed vLLM backend support; llama.cpp is now the sole backend (see ADR 001)
- Removed CUDA and ROCm backend support; Vulkan is now the sole GPU backend
- Updated security contact email in SECURITY.md
- Expanded CONTRIBUTING.md with GitHub fork/branch/PR workflow
- Expanded DEVELOPMENT.md with first-time contributor setup guide
- Added ADR index to ARCHITECTURE.md

### Security
- Fixed token scope bypass: category-scoped tokens no longer fall through to unrestricted model resolution
- Fixed HuggingFace download path traversal: file paths with `..` or leading `/` are rejected
- Fixed `hf_repo` directory traversal: format validation rejects `..` and non-standard characters
- Added constant-time comparison for bootstrap credentials (prevents timing side-channel)
- Added 10 MB request body size limit
- Added `Referrer-Policy` and `Permissions-Policy` security headers
- Migrated `DB_ENCRYPTION_KEY` derivation from bare SHA-256 to HKDF-SHA256 (automatic data migration on startup)
- Changed `BREAK_GLASS` default to `false` in docker-compose.yml; startup warns on default credentials
- Dockerfile now runs as non-root user (`sovereign`)

### Removed
- `docker-compose.nvidia.yml` and `docker-compose.rocm.yml` overlay files

## [0.9.0] - 2026-02-13

Initial public release preparation.

### Added
- Rust reverse proxy (axum) with OpenAI-compatible API passthrough
- Backend container management via Docker API (bollard) — llama.cpp with NVIDIA CUDA, AMD ROCm, or CPU-only
- OIDC authentication with PKCE and configurable identity providers
- Bootstrap credential authentication (break-glass mode)
- API token management (SHA-256 hashed, scoped per user, configurable expiry)
- Fair-use request scheduler with per-user queuing
- React dashboard with model management, user admin, and usage metrics
- Multi-model support with GPU memory-aware loading
- HuggingFace model search and background download with progress tracking
- TLS support: manual certs or automatic via Let's Encrypt (ACME TLS-ALPN-01)
- Open WebUI integration with trusted-header SSO
- Dual Docker network architecture (public + isolated internal)
- SQLite database with WAL mode and compile-time migration support
