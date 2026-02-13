# ADR 007: CSP SHA-256 computed at startup from built index.html

**Status:** Accepted
**Date:** 2026-02-17

## Context

The React SPA (`/portal/*`) uses a Content Security Policy (CSP) header to prevent XSS. The CSP `script-src` directive must allowlist any inline `<script>` blocks in `index.html`.

Vite's build output may change the contents of inline scripts (e.g. the theme-detection script) across builds. A hardcoded SHA-256 hash in the Rust source would break whenever the frontend is rebuilt, requiring a coordinated proxy rebuild.

## Decision

At startup, the proxy:
1. Reads `{UI_PATH}/index.html`
2. Extracts all inline `<script>` blocks (no `src` attribute)
3. Computes SHA-256 of each block, base64-encodes it
4. Stores the full CSP header value in a `OnceLock<String>`
5. The `security_headers` middleware uses this computed value for `/portal/*` responses

If `index.html` is not found (dev mode without a built UI), the proxy falls back to the previously hardcoded hash with a warning log.

CSP is only applied to `/portal/*` routes. Proxied apps (Open WebUI) set their own CSP.

## Consequences

- **Positive:** Frontend can be rebuilt independently without updating the proxy. CSP always matches the actual inline scripts.
- **Negative:** CSP is computed once at startup. If `index.html` is replaced while the proxy is running (e.g. hot-deploy), a restart is needed. This is acceptable since the proxy and UI are co-deployed in the same Docker image.
