# ADR 009: Unified SSE event stream

**Status:** Accepted
**Date:** 2026-02-17

## Context

The system needs to push real-time data (GPU metrics, reservation changes) to the React dashboard. Options: WebSockets, polling, SSE. WebSockets add complexity (upgrade handling, state management). Polling wastes bandwidth and adds latency. The data flow is unidirectional (server→client).

## Decision

Use a single SSE endpoint (`/api/user/events`) that merges two streams: `metrics` events (pushed every 2s with GPU memory, CPU, disk, queue stats) and `reservations_changed` events (signal-only; clients re-fetch authoritative state). Single connection reduces overhead. The frontend (`EventStreamProvider.tsx`) reconnects with exponential backoff (3–30s).

## Consequences

- **Positive:** Simple, HTTP-native, no WebSocket complexity, single connection per client, works through proxies/load balancers
- **Negative:** Unidirectional only (client-to-server requires separate HTTP calls), no binary framing
