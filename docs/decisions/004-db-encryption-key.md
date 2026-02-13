# ADR 004: Optional AES-256-GCM encryption for IdP secrets

**Status:** Accepted
**Date:** 2026-02-13

## Context

OIDC identity providers require a `client_secret` to be stored in the database for the token exchange step. Storing these secrets in plaintext in SQLite is a security concern — if the database file is compromised, all IdP client secrets are exposed.

Full database encryption (e.g. SQLCipher) adds significant complexity and dependency overhead for protecting a handful of secrets.

## Decision

Introduce an optional `DB_ENCRYPTION_KEY` environment variable. When set:
- IdP `client_secret` values are encrypted with AES-256-GCM before storage in the `client_secret_enc` column
- The encryption key is derived via HKDF-SHA256 with a fixed application-specific salt and info string
- A random 12-byte nonce is prepended to each ciphertext
- On startup, `migrate_plaintext_secrets()` handles three cases:
  1. Already encrypted with the current HKDF key — no action
  2. Encrypted with the legacy SHA-256 key — automatically re-encrypted with HKDF
  3. Plaintext — encrypted with the HKDF key

When `DB_ENCRYPTION_KEY` is not set:
- Secrets are stored in plaintext (with a warning log at startup)
- The system remains fully functional

`DB_ENCRYPTION_KEY` must be a high-entropy random value (e.g. 32+ hex characters from `openssl rand -hex 32`), not a human-chosen passphrase. HKDF extracts a uniform key but does not add stretching — a low-entropy passphrase would still be brute-forceable.

## Consequences

- **Positive:** Secrets at rest are protected without full-DB encryption. The feature is opt-in and zero-config for dev environments.
- **Negative:** Key management is the user's responsibility. No key rotation support — changing the key breaks decryption of existing secrets. This is acceptable for the small-scale deployment model (typically 1-3 IdPs).
