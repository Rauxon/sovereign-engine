# Sovereign Engine — Reservation System

## Purpose

The reservation system provides exclusive-use time windows for GPU resources. Users request time slots for batch inference jobs or experiments; admins approve or reject them. During an active reservation, the reservation holder can start and stop backend containers independently.

## State Machine

```
            ┌──────────┐
   create   │          │  admin reject
  ────────► │ pending  │ ──────────────► rejected
            │          │
            └────┬─────┘
                 │ admin approve
                 ▼
            ┌──────────┐
            │          │
            │ approved │
            │          │
            └────┬─────┘
                 │ tick (start_time reached) or admin force-activate
                 ▼
            ┌──────────┐
            │          │
            │  active  │
            │          │
            └────┬─────┘
                 │ tick (end_time reached) or admin force-deactivate
                 ▼
            ┌──────────┐
            │completed │
            └──────────┘

  Any pending/approved → cancelled (by user or auto-cancel when start_time passes)
```

### Status Definitions

| Status | Meaning |
|---|---|
| `pending` | Created by user, awaiting admin approval |
| `approved` | Admin approved; will auto-activate when `start_time` is reached |
| `active` | Currently running; reservation holder has exclusive container access |
| `completed` | Ended normally (end_time reached) or force-deactivated by admin |
| `rejected` | Admin declined the request |
| `cancelled` | User cancelled, or auto-cancelled (start_time passed without approval) |

## Time Boundaries

- All reservation times must be on **30-minute boundaries** (minute 0 or 30, seconds 0)
- Minimum duration: 30 minutes
- `start_time` must be in the future at creation time
- No overlap allowed between approved/active reservations

## Automatic Transitions (`tick_reservations`)

A background task runs every 30 seconds and performs three operations:

1. **Complete expired active reservations:** If an active reservation's `end_time <= now`, set status to `completed` and clear the in-memory active reservation cache.

2. **Activate approved reservations:** If no reservation is currently active and an approved reservation's `start_time <= now`, set status to `active` and populate the in-memory cache. The earliest approved reservation (by start_time) is activated first.

3. **Auto-cancel stale pending reservations:** If a pending reservation's `start_time <= now` (it was never approved), set status to `cancelled` with an admin note explaining the auto-cancellation.

These operations cascade within a single tick: an active reservation can complete, and the next approved reservation activates in the same cycle.

## Admin Approval Workflow

1. User creates a reservation via `POST /api/user/reservations`
2. Reservation appears with status `pending` in the admin panel
3. Admin reviews and either:
   - **Approves** (`POST /api/admin/reservations/:id/approve`) — checks for overlap first
   - **Rejects** (`POST /api/admin/reservations/:id/reject`) — with optional note
4. Approved reservations auto-activate when their start_time arrives
5. Admin can force-activate early or force-deactivate (end early) if needed

## Container Access During Reservation

When a reservation is active, the reservation holder can:
- **Start containers** via `POST /api/user/reservations/containers/start`
- **Stop containers** via `POST /api/user/reservations/containers/stop`

These endpoints verify the caller's user_id matches the active reservation's user_id. Non-holders receive 403 Forbidden.

Container start accepts `model_id` (required) plus optional `backend_type`, `gpu_type`, `gpu_layers`, `context_size`, and `parallel` parameters.

## Internal Token Exemptions

Internal tokens (used by Open WebUI for proxied `/v1` calls) are exempt from reservation-based access restrictions. This ensures Open WebUI continues functioning during active reservations.

## SSE Broadcast Notifications

All reservation state changes trigger a `reservations_changed` event on the unified SSE stream (`GET /api/user/events`). The event carries no payload — clients re-fetch their data upon receipt.

Changes that trigger broadcasts:
- Create, cancel (user)
- Approve, reject, force-activate, force-deactivate, delete (admin)
- Tick transitions (auto-activate, auto-complete, auto-cancel)

## Persistence and Recovery

- **Database:** All reservation state persists in the `reservations` table (SQLite)
- **In-memory cache:** The currently active reservation is cached in `Scheduler` for fast access (avoids DB queries on every API request)
- **Restart recovery:** On startup, `recover_active_reservation()` queries the DB for any reservation with status `active` and repopulates the in-memory cache
- **Calendar view:** `GET /api/user/reservations/calendar` returns all approved, active, and pending reservations for the week calendar UI component
