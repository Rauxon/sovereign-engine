# ADR 016: Logarithmic Fairness

**Status:** Accepted
**Date:** 2026-02-17

## Context
Multiple users share GPU resources via a request queue. Heavy users should not monopolize the queue, but light usage spikes should not be penalized disproportionately. A linear penalty would punish moderate users too harshly.

## Decision
Calculate queue priority using a logarithmic formula:
```
priority = base_priority + (wait_weight * wait_seconds) - (usage_weight * ln(1 + recent_tokens / usage_scale))
```
Higher priority = dequeued first. The logarithm curve means small usage differences (200 vs 400 tokens) produce negligible penalty differences, while large differences (200 vs 200,000 tokens) produce significant differences. All parameters (`base_priority`, `wait_weight`, `usage_weight`, `usage_scale`, `window_minutes`) are stored in the `settings` table and adjustable at runtime via `/api/admin/settings`.

## Consequences
- **Positive:** Fairness scales intuitively — doubling usage from 100 to 200 tokens barely affects priority, but going from 1,000 to 100,000 does. Wait time bonus prevents starvation. Runtime-tunable without recompilation.
- **Negative:** Logarithmic priority is harder to reason about than simple round-robin or strict FIFO. Operators need to understand the formula to tune effectively.
