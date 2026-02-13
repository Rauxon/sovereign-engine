# ADR 017: Concurrency Gate RAII

**Status:** Accepted
**Date:** 2026-02-17

## Context
Each llama.cpp backend has a configurable number of parallel inference slots (`parallel_slots` in `container_secrets`). The proxy must limit concurrent requests to avoid overloading backends. Slots must be released reliably even if handlers panic, time out, or encounter errors.

## Decision
Implement concurrency limiting as an RAII guard (`AcquiredSlot`). `acquire()` returns an `AcquiredSlot` when a slot is available, or enqueues the request in a priority-ordered wait list. The `Drop` implementation on `AcquiredSlot` spawns an async task to release the slot and wake the next queued request. Handlers hold the guard for the lifetime of the backend request — the slot is released when the guard goes out of scope.

## Consequences
- **Positive:** RAII guarantees slot release even on panic or early return. Wake-on-drop prevents thundering herd (only one waiter wakes at a time). Drop spawns release as a `tokio::spawn` task to avoid blocking if drop occurs outside an async context.
- **Negative:** The spawned release task adds minimal overhead. Guard semantics require holding the `AcquiredSlot` variable in scope for the duration of the request — accidentally dropping it early would release the slot prematurely.
