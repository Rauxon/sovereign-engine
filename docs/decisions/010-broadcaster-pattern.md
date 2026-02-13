# ADR 010: Broadcaster pattern for fan-out

**Status:** Accepted
**Date:** 2026-02-17

## Context

Multiple async tasks produce state changes (metrics collector, reservation tick) and multiple SSE connections consume them. Need to decouple producers from consumers without shared locks blocking async tasks.

## Decision

Use `tokio::broadcast` channels. `MetricsBroadcaster` and `ReservationBroadcaster` wrap `Arc<broadcast::Sender<T>>`. Producers publish snapshots; each SSE handler subscribes via `.subscribe()` and receives a dedicated `Receiver`. The metrics collector runs on a 2s interval; the reservation tick runs on a 30s interval.

## Consequences

- **Positive:** Lock-free fan-out, each subscriber gets independent stream, producers don't block on slow consumers (lagged messages are dropped)
- **Negative:** Broadcast channels allocate per-message; memory usage scales with channel capacity × message size. Acceptable for the low-frequency metrics use case
