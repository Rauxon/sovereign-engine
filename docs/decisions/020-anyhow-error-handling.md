# ADR 020: Anyhow for error handling, no custom error types

**Status:** Accepted
**Date:** 2026-02-17

## Context

Rust offers two main approaches to error handling: `anyhow::Result<T>` for application-level code (opaque errors with context strings) and `thiserror::Error` for library-level code (typed error enums that callers can pattern-match on). Sovereign Engine is an application, not a library — callers of its internal functions do not need to branch on specific error variants.

## Decision

Use `anyhow::Result<T>` throughout the proxy codebase. Add context via `.context()` for debugging. Do not define custom error enums. API handlers convert errors to appropriate HTTP status codes and generic error messages at the boundary (see `api/error.rs`).

## Consequences

- **Positive:** Less boilerplate — no error enum maintenance as new failure modes are added. Context strings provide clear debugging information in logs. Faster iteration when adding new features.
- **Negative:** No compile-time verification that all error cases are handled. Error responses to clients must be crafted manually rather than derived from error types. Acceptable because the proxy is an application with a small team, not a library with external consumers.
