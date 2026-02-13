# ADR 015: Per-Container API Keys

**Status:** Accepted
**Date:** 2026-02-17

## Context
Each llama.cpp backend container serves an HTTP API on port 8080 within the `sovereign-internal` Docker network. Although the network is isolated (`internal: true`), defense-in-depth requires that backend containers authenticate incoming requests from the proxy.

## Decision
Provision each container with a unique UUID API key, stored in the `container_secrets` table. The key is passed as `--api-key` to the llama-server process. The proxy sends `Authorization: Bearer {api_key}` when forwarding requests. Keys are generated at container creation time and stored alongside container metadata (UID, model_id).

## Consequences
- **Positive:** If network isolation is breached, an attacker cannot directly query backends without the per-container key. Keys are scoped per container, so compromising one key does not expose others.
- **Negative:** Adds a database lookup per request to retrieve the container's API key. Mitigated by the key being fetched alongside model resolution, which already queries the database.
