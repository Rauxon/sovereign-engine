# ADR 023: SSE connection delay on page load

**Status:** Accepted
**Date:** 2026-02-17

## Context

The React dashboard establishes an SSE connection to `/api/user/events` for real-time metrics. Browsers limit concurrent connections per domain to 6 on HTTP/1.1. During a hard refresh, the HTML, CSS, JS, fonts, and other critical assets compete for those 6 slots. If the SSE connection opens immediately, it occupies a slot indefinitely (long-lived), potentially causing other assets to queue and the page to appear slow or hang.

## Decision

Delay the SSE connection by 2 seconds after component mount (`setTimeout(connect, 2000)` in `EventStreamProvider.tsx`). This allows critical page assets to load first. The connection then establishes and begins receiving metrics updates.

## Consequences

- **Positive:** Prevents page load stalls on HTTP/1.1. Critical assets (CSS, JS) load without contention.
- **Negative:** 2-second delay before real-time metrics appear on the dashboard. Imperceptible in practice since the page is still rendering during this window. Not needed on HTTP/2 (multiplexed connections), but harmless.
