pub mod fairness;
pub mod gate;
pub mod queue;
pub mod reservation;
pub mod resolver;
pub mod settings;
pub mod usage;

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::db::Database;
use gate::ConcurrencyGate;
use queue::{QueueStats, RequestQueue};
use reservation::ActiveReservation;
use resolver::ResolvedModel;
use settings::FairnessSettings;

/// The scheduler manages per-model queues, concurrency gating, fair-use priority,
/// and model resolution.
///
/// Cloning is cheap — clones share the same underlying data via Arc.
#[derive(Debug, Clone)]
pub struct Scheduler {
    queue: RequestQueue,
    gate: ConcurrencyGate,
    settings: Arc<RwLock<FairnessSettings>>,
    active_reservation: Arc<RwLock<Option<ActiveReservation>>>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            queue: RequestQueue::new(),
            gate: ConcurrencyGate::new(),
            settings: Arc::new(RwLock::new(FairnessSettings::default())),
            active_reservation: Arc::new(RwLock::new(None)),
        }
    }

    /// Resolve a model for an inference request.
    pub async fn resolve_model(
        &self,
        db: &Database,
        model_name: &str,
        category_id: Option<&str>,
        specific_model_id: Option<&str>,
    ) -> anyhow::Result<ResolvedModel> {
        resolver::resolve_model(db, model_name, category_id, specific_model_id).await
    }

    /// Get the queue depth for a specific key.
    pub async fn get_queue_depth(&self, queue_key: &str) -> usize {
        self.queue.depth(queue_key).await
    }

    /// Get depths for all queues.
    pub async fn get_all_depths(&self) -> HashMap<String, usize> {
        self.queue.all_depths().await
    }

    /// Get stats (depth + avg wait) for all queues.
    pub async fn get_queue_stats(&self) -> HashMap<String, QueueStats> {
        self.queue.all_stats().await
    }

    /// Access the underlying request queue.
    pub fn queue(&self) -> &RequestQueue {
        &self.queue
    }

    /// Access the concurrency gate.
    pub fn gate(&self) -> &ConcurrencyGate {
        &self.gate
    }

    /// Get a read-locked snapshot of the current fairness settings.
    pub async fn settings(&self) -> FairnessSettings {
        self.settings.read().await.clone()
    }

    /// Reload settings from the database into the cached Arc.
    pub async fn reload_settings(&self, db: &Database) -> anyhow::Result<()> {
        let new_settings = settings::load_settings(db).await?;
        let mut locked = self.settings.write().await;
        *locked = new_settings;
        Ok(())
    }

    /// Get the currently active reservation (if any).
    pub async fn active_reservation(&self) -> Option<ActiveReservation> {
        self.active_reservation.read().await.clone()
    }

    /// Set or clear the active reservation cache.
    pub async fn set_active_reservation(&self, reservation: Option<ActiveReservation>) {
        let mut locked = self.active_reservation.write().await;
        *locked = reservation;
    }
}
