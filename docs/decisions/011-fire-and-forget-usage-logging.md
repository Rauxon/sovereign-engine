# ADR 011: Fire-and-forget usage logging

**Status:** Accepted
**Date:** 2026-02-17

## Context

After proxying each inference request to a llama.cpp backend, usage must be logged (token_id, user_id, model_id, latency). Awaiting the database write would add latency to the user-facing response.

## Decision

Spawn usage logging as a detached `tokio::spawn` task. The HTTP response is returned to the client before the database write completes. If the write fails, the error is logged but does not affect the client.

## Consequences

- **Positive:** Zero additional latency on inference responses
- **Negative:** Usage logs are best-effort — a crash between response and write loses that entry. Acceptable trade-off for a monitoring/analytics feature rather than a billing-critical path
