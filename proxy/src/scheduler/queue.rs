use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::{oneshot, RwLock};

/// A request waiting in the queue.
pub struct QueuedRequest {
    pub request_id: String,
    pub user_id: String,
    /// The queue key — typically the model_id for concurrency gating.
    pub queue_key: String,
    pub priority: f64,
    pub enqueued_at: DateTime<Utc>,
    /// Oneshot sender to wake this request when a slot becomes available.
    pub waker: oneshot::Sender<()>,
}

// Manual Debug impl since oneshot::Sender doesn't implement Debug
impl std::fmt::Debug for QueuedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueuedRequest")
            .field("request_id", &self.request_id)
            .field("user_id", &self.user_id)
            .field("queue_key", &self.queue_key)
            .field("priority", &self.priority)
            .field("enqueued_at", &self.enqueued_at)
            .finish()
    }
}

/// Per-queue statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct QueueStats {
    pub depth: usize,
    pub avg_wait_ms: i64,
}

/// Thread-safe per-key request queue.
///
/// Cloning is cheap — clones share the same underlying data via Arc.
#[derive(Debug, Clone)]
pub struct RequestQueue {
    queues: Arc<RwLock<HashMap<String, VecDeque<QueuedRequest>>>>,
}

impl Default for RequestQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestQueue {
    pub fn new() -> Self {
        Self {
            queues: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a request to the queue for a given key.
    pub async fn enqueue(&self, request: QueuedRequest) {
        let mut queues = self.queues.write().await;
        queues
            .entry(request.queue_key.clone())
            .or_default()
            .push_back(request);
    }

    /// Remove and return the highest-priority request from a queue key.
    pub async fn dequeue(&self, queue_key: &str) -> Option<QueuedRequest> {
        let mut queues = self.queues.write().await;
        let queue = queues.get_mut(queue_key)?;

        if queue.is_empty() {
            return None;
        }

        // Find the index of the highest-priority request
        let best_idx = queue
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                a.priority
                    .partial_cmp(&b.priority)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)?;

        queue.remove(best_idx)
    }

    /// Remove a specific request by ID (used for timeout cleanup).
    pub async fn remove_by_id(&self, queue_key: &str, request_id: &str) {
        let mut queues = self.queues.write().await;
        if let Some(queue) = queues.get_mut(queue_key) {
            queue.retain(|r| r.request_id != request_id);
        }
    }

    /// Get the depth of a specific queue.
    pub async fn depth(&self, queue_key: &str) -> usize {
        let queues = self.queues.read().await;
        queues.get(queue_key).map_or(0, |q| q.len())
    }

    /// Get depths for all queues.
    pub async fn all_depths(&self) -> HashMap<String, usize> {
        let queues = self.queues.read().await;
        queues.iter().map(|(k, v)| (k.clone(), v.len())).collect()
    }

    /// Get stats for all queues (depth + average wait time).
    pub async fn all_stats(&self) -> HashMap<String, QueueStats> {
        let queues = self.queues.read().await;
        let now = Utc::now();

        queues
            .iter()
            .map(|(key, queue)| {
                let depth = queue.len();
                let avg_wait_ms = if depth > 0 {
                    let total_ms: i64 = queue
                        .iter()
                        .map(|r| (now - r.enqueued_at).num_milliseconds())
                        .sum();
                    total_ms / depth as i64
                } else {
                    0
                };

                (key.clone(), QueueStats { depth, avg_wait_ms })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a QueuedRequest and its wakeup receiver.
    fn make_request(
        id: &str,
        user: &str,
        key: &str,
        priority: f64,
    ) -> (QueuedRequest, oneshot::Receiver<()>) {
        let (tx, rx) = oneshot::channel();
        let req = QueuedRequest {
            request_id: id.to_string(),
            user_id: user.to_string(),
            queue_key: key.to_string(),
            priority,
            enqueued_at: Utc::now(),
            waker: tx,
        };
        (req, rx)
    }

    #[tokio::test]
    async fn dequeue_empty_returns_none() {
        let q = RequestQueue::new();
        assert!(q.dequeue("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn enqueue_then_dequeue_single() {
        let q = RequestQueue::new();
        let (req, _rx) = make_request("r1", "user1", "model-a", 1.0);
        q.enqueue(req).await;

        let got = q.dequeue("model-a").await.unwrap();
        assert_eq!(got.request_id, "r1");

        // Queue is now empty
        assert!(q.dequeue("model-a").await.is_none());
    }

    #[tokio::test]
    async fn dequeue_returns_highest_priority() {
        let q = RequestQueue::new();
        let (r1, _rx1) = make_request("r1", "u1", "m", 1.0);
        let (r2, _rx2) = make_request("r2", "u2", "m", 3.0);
        let (r3, _rx3) = make_request("r3", "u3", "m", 2.0);

        q.enqueue(r1).await;
        q.enqueue(r2).await;
        q.enqueue(r3).await;

        let first = q.dequeue("m").await.unwrap();
        assert_eq!(first.request_id, "r2"); // priority 3.0

        let second = q.dequeue("m").await.unwrap();
        assert_eq!(second.request_id, "r3"); // priority 2.0

        let third = q.dequeue("m").await.unwrap();
        assert_eq!(third.request_id, "r1"); // priority 1.0
    }

    #[tokio::test]
    async fn dequeue_equal_priority_does_not_panic() {
        let q = RequestQueue::new();
        let (r1, _rx1) = make_request("r1", "u1", "m", 5.0);
        let (r2, _rx2) = make_request("r2", "u2", "m", 5.0);

        q.enqueue(r1).await;
        q.enqueue(r2).await;

        // Just verify it dequeues both without panicking
        let a = q.dequeue("m").await.unwrap();
        let b = q.dequeue("m").await.unwrap();
        assert_ne!(a.request_id, b.request_id);
    }

    #[tokio::test]
    async fn remove_by_id_removes_correct_item() {
        let q = RequestQueue::new();
        let (ra, _rxa) = make_request("a", "u", "m", 1.0);
        let (rb, _rxb) = make_request("b", "u", "m", 2.0);
        let (rc, _rxc) = make_request("c", "u", "m", 3.0);

        q.enqueue(ra).await;
        q.enqueue(rb).await;
        q.enqueue(rc).await;

        q.remove_by_id("m", "b").await;

        assert_eq!(q.depth("m").await, 2);
        let first = q.dequeue("m").await.unwrap();
        assert_eq!(first.request_id, "c");
        let second = q.dequeue("m").await.unwrap();
        assert_eq!(second.request_id, "a");
    }

    #[tokio::test]
    async fn remove_by_id_nonexistent_is_noop() {
        let q = RequestQueue::new();
        let (r1, _rx1) = make_request("r1", "u", "m", 1.0);
        q.enqueue(r1).await;

        q.remove_by_id("m", "nonexistent").await;
        assert_eq!(q.depth("m").await, 1);
    }

    #[tokio::test]
    async fn depth_tracks_correctly() {
        let q = RequestQueue::new();
        assert_eq!(q.depth("m").await, 0);

        let (r1, _rx1) = make_request("r1", "u", "m", 1.0);
        let (r2, _rx2) = make_request("r2", "u", "m", 2.0);
        let (r3, _rx3) = make_request("r3", "u", "m", 3.0);
        q.enqueue(r1).await;
        q.enqueue(r2).await;
        q.enqueue(r3).await;
        assert_eq!(q.depth("m").await, 3);

        q.dequeue("m").await;
        assert_eq!(q.depth("m").await, 2);
    }

    #[tokio::test]
    async fn separate_queues_are_independent() {
        let q = RequestQueue::new();
        let (ra, _rxa) = make_request("a1", "u", "a", 1.0);
        let (rb, _rxb) = make_request("b1", "u", "b", 1.0);

        q.enqueue(ra).await;
        q.enqueue(rb).await;

        assert_eq!(q.depth("a").await, 1);
        assert_eq!(q.depth("b").await, 1);

        q.dequeue("a").await;
        assert_eq!(q.depth("a").await, 0);
        assert_eq!(q.depth("b").await, 1); // unaffected
    }

    #[tokio::test]
    async fn all_stats_returns_all_keys() {
        let q = RequestQueue::new();
        let (r1, _rx1) = make_request("r1", "u", "alpha", 1.0);
        let (r2, _rx2) = make_request("r2", "u", "beta", 1.0);
        let (r3, _rx3) = make_request("r3", "u", "beta", 2.0);

        q.enqueue(r1).await;
        q.enqueue(r2).await;
        q.enqueue(r3).await;

        let stats = q.all_stats().await;
        assert_eq!(stats.len(), 2);
        assert_eq!(stats["alpha"].depth, 1);
        assert_eq!(stats["beta"].depth, 2);
    }
}
