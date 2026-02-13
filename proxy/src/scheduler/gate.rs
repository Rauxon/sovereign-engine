use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{oneshot, RwLock};
use tracing::{debug, warn};
use uuid::Uuid;

use super::fairness;
use super::queue::RequestQueue;
use super::settings::FairnessSettings;
use crate::db::Database;

/// Error returned when a request times out waiting for a concurrency slot.
#[derive(Debug)]
pub struct QueueTimeout;

/// Snapshot of a single model's gate state (for observability).
#[derive(Debug, Clone, serde::Serialize)]
pub struct GateSnapshot {
    pub max_slots: u32,
    pub in_flight: u32,
}

/// Per-model concurrency state.
struct GateState {
    max_slots: u32,
    in_flight: u32,
}

/// Per-model concurrency limiter with fair-queue wakeup.
///
/// Cloning is cheap — clones share the same underlying data via Arc.
#[derive(Debug, Clone)]
pub struct ConcurrencyGate {
    state: Arc<RwLock<HashMap<String, GateState>>>,
}

impl std::fmt::Debug for GateState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GateState")
            .field("max_slots", &self.max_slots)
            .field("in_flight", &self.in_flight)
            .finish()
    }
}

/// RAII guard that releases a concurrency slot on drop.
///
/// Holds a reference to the gate and wakes the next queued request when released.
pub struct AcquiredSlot {
    gate: ConcurrencyGate,
    queue: RequestQueue,
    model_id: String,
}

impl Drop for AcquiredSlot {
    fn drop(&mut self) {
        let gate = self.gate.clone();
        let queue = self.queue.clone();
        let model_id = self.model_id.clone();
        // Spawn release as a task so it doesn't block if drop happens outside async context
        tokio::spawn(async move {
            gate.release_and_wake(&model_id, &queue).await;
        });
    }
}

impl Default for ConcurrencyGate {
    fn default() -> Self {
        Self::new()
    }
}

impl ConcurrencyGate {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Return a snapshot of all gate states (for observability / TUI).
    pub async fn status(&self) -> HashMap<String, GateSnapshot> {
        let state = self.state.read().await;
        state
            .iter()
            .map(|(k, gs)| {
                (
                    k.clone(),
                    GateSnapshot {
                        max_slots: gs.max_slots,
                        in_flight: gs.in_flight,
                    },
                )
            })
            .collect()
    }

    /// Register a model with its maximum parallel slots. Called on container start.
    pub async fn register(&self, model_id: &str, max_slots: u32) {
        let mut state = self.state.write().await;
        state.insert(
            model_id.to_string(),
            GateState {
                max_slots,
                in_flight: 0,
            },
        );
        debug!(model = %model_id, max_slots, "Gate registered");
    }

    /// Unregister a model. Called on container stop.
    pub async fn unregister(&self, model_id: &str) {
        let mut state = self.state.write().await;
        state.remove(model_id);
        debug!(model = %model_id, "Gate unregistered");
    }

    /// Non-blocking: try to acquire a slot. Returns true if under the limit.
    async fn try_acquire(&self, model_id: &str) -> bool {
        let mut state = self.state.write().await;
        if let Some(gs) = state.get_mut(model_id) {
            if gs.in_flight < gs.max_slots {
                gs.in_flight += 1;
                return true;
            }
        } else {
            // Model not registered — allow through (no gate configured).
            // This is a safety net; callers should only gate registered models.
            return true;
        }
        false
    }

    /// Decrement in-flight count and wake the highest-priority queued request.
    async fn release_and_wake(&self, model_id: &str, queue: &RequestQueue) {
        {
            let mut state = self.state.write().await;
            if let Some(gs) = state.get_mut(model_id) {
                gs.in_flight = gs.in_flight.saturating_sub(1);
                debug!(model = %model_id, in_flight = gs.in_flight, "Slot released");
            }
        }

        // Wake the highest-priority queued request for this model
        if let Some(req) = queue.dequeue(model_id).await {
            debug!(model = %model_id, user = %req.user_id, "Waking queued request");
            // Send on the oneshot — if the receiver was dropped (timeout), this is a no-op
            let _ = req.waker.send(());
        }
    }

    /// Acquire a concurrency slot, waiting up to `timeout` if all slots are busy.
    ///
    /// When waiting, the request is enqueued with its fair-use priority so that
    /// heavy users wait longer than light users.
    ///
    /// Returns an `AcquiredSlot` RAII guard that auto-releases on drop.
    pub async fn acquire_with_timeout(
        &self,
        model_id: &str,
        user_id: &str,
        db: &Database,
        settings: &FairnessSettings,
        queue: &RequestQueue,
        timeout: Duration,
    ) -> Result<AcquiredSlot, QueueTimeout> {
        // Fast path: slot available immediately
        if self.try_acquire(model_id).await {
            return Ok(AcquiredSlot {
                gate: self.clone(),
                queue: queue.clone(),
                model_id: model_id.to_string(),
            });
        }

        // Slow path: enqueue and wait
        let priority = match fairness::calculate_user_priority(db, settings, user_id, 0.0).await {
            Ok(p) => p,
            Err(e) => {
                warn!(error = %e, "Failed to calculate priority, using base");
                settings.base_priority
            }
        };

        let request_id = Uuid::new_v4().to_string();

        let (tx, rx) = oneshot::channel();

        queue
            .enqueue(super::queue::QueuedRequest {
                request_id: request_id.clone(),
                user_id: user_id.to_string(),
                queue_key: model_id.to_string(),
                priority,
                enqueued_at: chrono::Utc::now(),
                waker: tx,
            })
            .await;

        // Wait for wakeup or timeout
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(())) => {
                // We were woken — the slot was already accounted for by release_and_wake
                // We need to actually acquire the slot now
                // The release_and_wake dequeued us AND released a slot, so re-acquire
                if self.try_acquire(model_id).await {
                    Ok(AcquiredSlot {
                        gate: self.clone(),
                        queue: queue.clone(),
                        model_id: model_id.to_string(),
                    })
                } else {
                    // Race condition — another request grabbed the slot.
                    // This shouldn't happen with the current design but handle gracefully.
                    warn!(model = %model_id, "Woken but slot gone — treating as timeout");
                    Err(QueueTimeout)
                }
            }
            Ok(Err(_)) => {
                // Sender dropped — gate was unregistered or similar
                Err(QueueTimeout)
            }
            Err(_) => {
                // Timeout — remove from queue and return 429
                queue.remove_by_id(model_id, &request_id).await;
                Err(QueueTimeout)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::scheduler::queue::{QueuedRequest, RequestQueue};
    use crate::scheduler::settings::FairnessSettings;

    // ── Group A: pure gate mechanics (no DB) ──

    #[tokio::test]
    async fn register_and_try_acquire() {
        let gate = ConcurrencyGate::new();
        gate.register("m1", 2).await;

        assert!(gate.try_acquire("m1").await);
        assert!(gate.try_acquire("m1").await);
        assert!(!gate.try_acquire("m1").await); // full
    }

    #[tokio::test]
    async fn release_frees_slot() {
        let gate = ConcurrencyGate::new();
        let queue = RequestQueue::new();
        gate.register("m1", 1).await;

        assert!(gate.try_acquire("m1").await);
        assert!(!gate.try_acquire("m1").await); // full

        gate.release_and_wake("m1", &queue).await;
        assert!(gate.try_acquire("m1").await); // freed
    }

    #[tokio::test]
    async fn release_wakes_queued_request() {
        let gate = ConcurrencyGate::new();
        let queue = RequestQueue::new();
        gate.register("m1", 1).await;

        let (tx, rx) = oneshot::channel();
        queue
            .enqueue(QueuedRequest {
                request_id: "r1".to_string(),
                user_id: "u1".to_string(),
                queue_key: "m1".to_string(),
                priority: 1.0,
                enqueued_at: chrono::Utc::now(),
                waker: tx,
            })
            .await;

        gate.release_and_wake("m1", &queue).await;

        // The waker should have fired
        assert!(rx.await.is_ok());
        // Queue should be empty after dequeue
        assert_eq!(queue.depth("m1").await, 0);
    }

    #[tokio::test]
    async fn unregistered_model_allows_through() {
        let gate = ConcurrencyGate::new();
        // No register call — should fail-open
        assert!(gate.try_acquire("unknown").await);
    }

    #[tokio::test]
    async fn unregister_removes_gate() {
        let gate = ConcurrencyGate::new();
        gate.register("m1", 1).await;
        assert!(gate.try_acquire("m1").await);
        assert!(!gate.try_acquire("m1").await); // full

        gate.unregister("m1").await;
        // After unregistering, fail-open applies
        assert!(gate.try_acquire("m1").await);
    }

    #[tokio::test]
    async fn release_no_underflow() {
        let gate = ConcurrencyGate::new();
        let queue = RequestQueue::new();
        gate.register("m1", 2).await;

        // Release without any acquire — should not underflow below 0
        gate.release_and_wake("m1", &queue).await;
        gate.release_and_wake("m1", &queue).await;

        // Should still be able to acquire max_slots times
        assert!(gate.try_acquire("m1").await);
        assert!(gate.try_acquire("m1").await);
        assert!(!gate.try_acquire("m1").await);
    }

    // ── Group B: full acquire flow (DB needed) ──

    #[tokio::test]
    async fn acquire_immediate_when_slot_free() {
        let db = Database::test_db().await;
        let gate = ConcurrencyGate::new();
        let queue = RequestQueue::new();
        let settings = FairnessSettings::default();
        gate.register("m1", 2).await;

        let result = gate
            .acquire_with_timeout(
                "m1",
                "user1",
                &db,
                &settings,
                &queue,
                Duration::from_secs(1),
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn acquire_blocks_then_wakes() {
        let db = Database::test_db().await;
        let gate = ConcurrencyGate::new();
        let queue = RequestQueue::new();
        let settings = FairnessSettings::default();
        gate.register("m1", 1).await;

        // Occupy the single slot
        let _slot1 = gate
            .acquire_with_timeout(
                "m1",
                "user1",
                &db,
                &settings,
                &queue,
                Duration::from_secs(5),
            )
            .await
            .unwrap();

        // Spawn a second acquire that will block
        let gate2 = gate.clone();
        let queue2 = queue.clone();
        let db2 = db.clone();
        let settings2 = settings.clone();
        let handle = tokio::spawn(async move {
            gate2
                .acquire_with_timeout(
                    "m1",
                    "user2",
                    &db2,
                    &settings2,
                    &queue2,
                    Duration::from_secs(5),
                )
                .await
        });

        // Give the spawned task time to enqueue
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(queue.depth("m1").await, 1);

        // Drop the first slot — triggers release_and_wake via spawned task
        drop(_slot1);
        // Give the drop's spawned task time to run
        tokio::time::sleep(Duration::from_millis(50)).await;

        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn acquire_timeout_returns_error() {
        let db = Database::test_db().await;
        let gate = ConcurrencyGate::new();
        let queue = RequestQueue::new();
        let settings = FairnessSettings::default();
        gate.register("m1", 1).await;

        // Occupy the slot
        let _slot = gate
            .acquire_with_timeout(
                "m1",
                "user1",
                &db,
                &settings,
                &queue,
                Duration::from_secs(5),
            )
            .await
            .unwrap();

        // This should timeout
        let result = gate
            .acquire_with_timeout(
                "m1",
                "user2",
                &db,
                &settings,
                &queue,
                Duration::from_millis(50),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn acquire_timeout_cleans_up_queue() {
        let db = Database::test_db().await;
        let gate = ConcurrencyGate::new();
        let queue = RequestQueue::new();
        let settings = FairnessSettings::default();
        gate.register("m1", 1).await;

        // Occupy the slot
        let _slot = gate
            .acquire_with_timeout(
                "m1",
                "user1",
                &db,
                &settings,
                &queue,
                Duration::from_secs(5),
            )
            .await
            .unwrap();

        // Timeout
        let _ = gate
            .acquire_with_timeout(
                "m1",
                "user2",
                &db,
                &settings,
                &queue,
                Duration::from_millis(50),
            )
            .await;

        // Queue should be cleaned up after timeout
        assert_eq!(queue.depth("m1").await, 0);
    }

    #[tokio::test]
    async fn raii_guard_releases_on_drop() {
        let db = Database::test_db().await;
        let gate = ConcurrencyGate::new();
        let queue = RequestQueue::new();
        let settings = FairnessSettings::default();
        gate.register("m1", 1).await;

        {
            let _slot = gate
                .acquire_with_timeout(
                    "m1",
                    "user1",
                    &db,
                    &settings,
                    &queue,
                    Duration::from_secs(1),
                )
                .await
                .unwrap();
            // _slot dropped here
        }

        // Give spawned release task time to run
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Slot should be freed — can acquire again
        assert!(gate.try_acquire("m1").await);
    }
}
