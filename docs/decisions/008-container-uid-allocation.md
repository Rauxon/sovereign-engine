# ADR 008: Random UID allocation with collision avoidance

**Status:** Accepted
**Date:** 2026-02-13

## Context

Each backend container runs as a non-root user to provide process isolation. Two containers running as the same UID could potentially interfere with each other's processes.

The original implementation derived UIDs deterministically from a hash of the model_id: `UID = 10000 + (hash(model_id) % 50000)`. This had a collision risk — different model_ids could hash to the same UID, especially as the number of models grew.

## Decision

Allocate UIDs randomly in the range 10000–65000, with collision avoidance:

1. Query all running managed containers to collect their current UIDs
2. Generate a random UID in the range
3. If it collides with an existing container's UID, retry
4. The allocated UID is stored in `container_secrets` alongside the container's API key

Range rationale: above 10000 (avoids system UIDs), below 65000 (below `nobody` on most distributions).

## Consequences

- **Positive:** No deterministic collisions. UID uniqueness is guaranteed against running containers at allocation time. Simple implementation.
- **Negative:** Not deterministic — restarting the same model gets a different UID each time. This is fine because UIDs are only needed for process isolation, not for persistent identity. The UID is persisted in `container_secrets` for the lifetime of the container.
