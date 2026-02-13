# ADR 018: Reservation State Machine

**Status:** Accepted
**Date:** 2026-02-17

## Context
Users need to reserve exclusive GPU access for specific time windows (e.g., a 2-hour training run). Reservations require admin approval to prevent abuse, and must activate/deactivate automatically based on time boundaries.

## Decision
Implement reservations as a state machine with states: `pending` -> `approved` -> `active` -> `completed`, plus `rejected` and `cancelled` terminal states. A background task (`tick_reservations`) runs every 30 seconds to: activate approved reservations whose `start_time <= now`, complete active reservations whose `end_time <= now`, and cancel stale pending reservations. The active reservation is cached in-memory (recovered from DB on startup) for fast access during request processing. State changes are broadcast via `ReservationBroadcaster` so SSE clients re-fetch.

## Consequences
- **Positive:** Automatic transitions ensure reservations activate/deactivate without manual intervention. 30s tick provides eventual consistency with minimal polling overhead. In-memory cache avoids per-request DB queries to check reservation status.
- **Negative:** Up to 30s delay between a reservation's scheduled time and actual activation. Acceptable for the use case (users schedule hours in advance).
