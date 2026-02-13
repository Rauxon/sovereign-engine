# ADR 003: MODEL_HOST_PATH for Docker bind mounts

**Status:** Accepted
**Date:** 2026-02-13

## Context

The proxy manages backend containers (llama.cpp) and creates Docker bind mounts for model files. The proxy itself runs in a Docker container where models are mounted at `MODEL_PATH` (e.g. `/models`).

When the proxy creates a child container via the Docker API, it specifies a bind mount source path. This source path must be a **host filesystem path**, not a path inside the proxy container. If the host mounts `./models:/models`, then the child container bind mount source must be `./models` (the host path), not `/models` (the proxy container path).

When both paths happen to be the same (e.g. running the proxy directly on the host), there's no issue. But in Dockerized deployments, they often differ.

## Decision

Add a `MODEL_HOST_PATH` environment variable that specifies the host-side path for model bind mounts into child containers. It defaults to `MODEL_PATH` (correct when the proxy runs directly on the host).

- `MODEL_PATH` — where the proxy process reads model files (inside its own container)
- `MODEL_HOST_PATH` — what gets passed as the bind mount source when creating child containers

## Consequences

- **Positive:** Dockerized deployments work correctly without workarounds
- **Negative:** An extra env var to document and configure. Users who run everything on bare metal don't need it (the default handles their case).
