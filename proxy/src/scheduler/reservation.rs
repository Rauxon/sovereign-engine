use chrono::Utc;
use serde::Serialize;
use sqlx::{Pool, Sqlite};
use tokio::sync::broadcast;
use tracing::{info, warn};

use super::Scheduler;

/// Broadcasts a unit signal whenever reservations change (create, cancel,
/// approve, reject, activate, deactivate, delete, or tick transitions).
/// Frontend clients subscribe via SSE and re-fetch their data on receipt.
#[derive(Debug, Clone)]
pub struct ReservationBroadcaster {
    tx: broadcast::Sender<()>,
}

impl Default for ReservationBroadcaster {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(16);
        Self { tx }
    }
}

impl ReservationBroadcaster {
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe to reservation change notifications.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.tx.subscribe()
    }

    /// Notify all subscribers that reservations have changed.
    pub fn notify(&self) {
        let _ = self.tx.send(());
    }
}

/// A reservation row from the database.
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct Reservation {
    pub id: String,
    pub user_id: String,
    pub status: String,
    pub start_time: String,
    pub end_time: String,
    pub reason: String,
    pub admin_note: String,
    pub approved_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Reservation joined with user display info (for admin listing).
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct ReservationWithUser {
    pub id: String,
    pub user_id: String,
    pub status: String,
    pub start_time: String,
    pub end_time: String,
    pub reason: String,
    pub admin_note: String,
    pub approved_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub user_email: Option<String>,
    pub user_display_name: Option<String>,
}

/// In-memory representation of the currently active reservation.
#[derive(Debug, Clone, Serialize)]
pub struct ActiveReservation {
    pub reservation_id: String,
    pub user_id: String,
    pub end_time: String,
    pub user_display_name: Option<String>,
}

/// Background tick: activate approved reservations, complete expired active ones,
/// and cancel stale pending requests.
pub async fn tick_reservations(
    pool: &Pool<Sqlite>,
    scheduler: &Scheduler,
    broadcaster: &ReservationBroadcaster,
) {
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let mut changed = false;

    // 1. Complete active reservations whose end_time <= now
    let completed: Result<sqlx::sqlite::SqliteQueryResult, _> = sqlx::query(
        "UPDATE reservations SET status = 'completed', updated_at = datetime('now') \
         WHERE status = 'active' AND end_time <= ?",
    )
    .bind(&now)
    .execute(pool)
    .await;

    if let Ok(result) = &completed {
        if result.rows_affected() > 0 {
            info!(
                count = result.rows_affected(),
                "Completed expired active reservations"
            );
            scheduler.set_active_reservation(None).await;
            changed = true;
        }
    }

    // 2. Activate approved reservations whose start_time <= now (if no other active)
    let current_active: Option<ActiveReservation> = scheduler.active_reservation().await;
    if current_active.is_none() {
        if let Ok(Some((id, user_id, end_time, display_name))) =
            sqlx::query_as::<_, (String, String, String, Option<String>)>(
                "SELECT r.id, r.user_id, r.end_time, u.display_name \
                 FROM reservations r LEFT JOIN users u ON u.id = r.user_id \
                 WHERE r.status = 'approved' AND r.start_time <= ? \
                 ORDER BY r.start_time ASC LIMIT 1",
            )
            .bind(&now)
            .fetch_optional(pool)
            .await
        {
            let _ = sqlx::query(
                "UPDATE reservations SET status = 'active', updated_at = datetime('now') \
                 WHERE id = ?",
            )
            .bind(&id)
            .execute(pool)
            .await;

            info!(reservation = %id, user = %user_id, "Activated reservation");

            scheduler
                .set_active_reservation(Some(ActiveReservation {
                    reservation_id: id,
                    user_id,
                    end_time,
                    user_display_name: display_name,
                }))
                .await;
            changed = true;
        }
    }

    // 3. Auto-cancel pending reservations whose start_time has passed
    let cancelled: Result<sqlx::sqlite::SqliteQueryResult, _> = sqlx::query(
        "UPDATE reservations SET status = 'cancelled', \
         admin_note = 'Auto-cancelled: start time passed without approval', \
         updated_at = datetime('now') \
         WHERE status = 'pending' AND start_time <= ?",
    )
    .bind(&now)
    .execute(pool)
    .await;

    if let Ok(result) = &cancelled {
        if result.rows_affected() > 0 {
            info!(
                count = result.rows_affected(),
                "Auto-cancelled stale pending reservations"
            );
            changed = true;
        }
    }

    if changed {
        broadcaster.notify();
    }
}

/// Recover active reservation from DB on startup.
pub async fn recover_active_reservation(pool: &Pool<Sqlite>, scheduler: &Scheduler) {
    match sqlx::query_as::<_, (String, String, String, Option<String>)>(
        "SELECT r.id, r.user_id, r.end_time, u.display_name \
         FROM reservations r LEFT JOIN users u ON u.id = r.user_id \
         WHERE r.status = 'active' LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    {
        Ok(Some((id, user_id, end_time, display_name))) => {
            info!(reservation = %id, user = %user_id, "Recovered active reservation from DB");
            scheduler
                .set_active_reservation(Some(ActiveReservation {
                    reservation_id: id,
                    user_id,
                    end_time,
                    user_display_name: display_name,
                }))
                .await;
        }
        Ok(None) => {}
        Err(e) => {
            warn!(error = %e, "Failed to recover active reservation from DB");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    async fn setup() -> (Database, Scheduler, ReservationBroadcaster) {
        let db = Database::test_db().await;
        let scheduler = Scheduler::new();
        let broadcaster = ReservationBroadcaster::new();
        (db, scheduler, broadcaster)
    }

    async fn ensure_test_user(pool: &Pool<Sqlite>, user_id: &str) {
        sqlx::query(
            "INSERT OR IGNORE INTO idp_configs (id, name, issuer, client_id, client_secret_enc) \
             VALUES ('test-idp', 'test', 'https://test', 'client', 'secret')",
        )
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT OR IGNORE INTO users (id, idp_id, subject, email) \
             VALUES (?, 'test-idp', ?, ?)",
        )
        .bind(user_id)
        .bind(user_id)
        .bind(format!("{}@test.com", user_id))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_reservation(
        pool: &Pool<Sqlite>,
        user_id: &str,
        status: &str,
        start: &str,
        end: &str,
    ) -> String {
        ensure_test_user(pool, user_id).await;
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO reservations (id, user_id, status, start_time, end_time) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(user_id)
        .bind(status)
        .bind(start)
        .bind(end)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    async fn get_status(pool: &Pool<Sqlite>, id: &str) -> String {
        sqlx::query_as::<_, (String,)>("SELECT status FROM reservations WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
            .0
    }

    #[tokio::test]
    async fn tick_completes_expired_active() {
        let (db, scheduler, broadcaster) = setup().await;
        let past_start = "2020-01-01T00:00:00";
        let past_end = "2020-01-01T01:00:00";
        let id = insert_reservation(&db.pool, "user1", "active", past_start, past_end).await;

        // Set scheduler cache to simulate an active reservation
        scheduler
            .set_active_reservation(Some(ActiveReservation {
                reservation_id: id.clone(),
                user_id: "user1".to_string(),
                end_time: past_end.to_string(),
                user_display_name: None,
            }))
            .await;

        tick_reservations(&db.pool, &scheduler, &broadcaster).await;

        assert_eq!(get_status(&db.pool, &id).await, "completed");
        assert!(scheduler.active_reservation().await.is_none());
    }

    #[tokio::test]
    async fn tick_activates_approved_when_due() {
        let (db, scheduler, broadcaster) = setup().await;
        let past_start = "2020-01-01T00:00:00";
        let future_end = "2099-12-31T23:30:00";
        let id = insert_reservation(&db.pool, "user1", "approved", past_start, future_end).await;

        tick_reservations(&db.pool, &scheduler, &broadcaster).await;

        assert_eq!(get_status(&db.pool, &id).await, "active");
        let active = scheduler.active_reservation().await.unwrap();
        assert_eq!(active.reservation_id, id);
    }

    #[tokio::test]
    async fn tick_skips_activation_if_already_active() {
        let (db, scheduler, broadcaster) = setup().await;
        let future_end = "2099-12-31T23:30:00";

        // One active reservation (still valid)
        let active_id = insert_reservation(
            &db.pool,
            "user1",
            "active",
            "2020-01-01T00:00:00",
            future_end,
        )
        .await;
        scheduler
            .set_active_reservation(Some(ActiveReservation {
                reservation_id: active_id.clone(),
                user_id: "user1".to_string(),
                end_time: future_end.to_string(),
                user_display_name: None,
            }))
            .await;

        // An approved reservation also past start
        let approved_id = insert_reservation(
            &db.pool,
            "user2",
            "approved",
            "2020-06-01T00:00:00",
            future_end,
        )
        .await;

        tick_reservations(&db.pool, &scheduler, &broadcaster).await;

        // Active stays active, approved stays approved
        assert_eq!(get_status(&db.pool, &active_id).await, "active");
        assert_eq!(get_status(&db.pool, &approved_id).await, "approved");
    }

    #[tokio::test]
    async fn tick_activates_earliest_of_multiple() {
        let (db, scheduler, broadcaster) = setup().await;
        let future_end = "2099-12-31T23:30:00";

        // Two approved, both past start — earlier one should activate
        let earlier_id = insert_reservation(
            &db.pool,
            "user1",
            "approved",
            "2020-01-01T00:00:00",
            future_end,
        )
        .await;
        let later_id = insert_reservation(
            &db.pool,
            "user2",
            "approved",
            "2020-06-01T00:00:00",
            future_end,
        )
        .await;

        tick_reservations(&db.pool, &scheduler, &broadcaster).await;

        assert_eq!(get_status(&db.pool, &earlier_id).await, "active");
        assert_eq!(get_status(&db.pool, &later_id).await, "approved");
    }

    #[tokio::test]
    async fn tick_cancels_stale_pending() {
        let (db, scheduler, broadcaster) = setup().await;
        let past_start = "2020-01-01T00:00:00";
        let past_end = "2020-01-01T01:00:00";
        let id = insert_reservation(&db.pool, "user1", "pending", past_start, past_end).await;

        tick_reservations(&db.pool, &scheduler, &broadcaster).await;

        assert_eq!(get_status(&db.pool, &id).await, "cancelled");
        // Verify admin_note is set
        let note: String =
            sqlx::query_as::<_, (String,)>("SELECT admin_note FROM reservations WHERE id = ?")
                .bind(&id)
                .fetch_one(&db.pool)
                .await
                .unwrap()
                .0;
        assert!(note.contains("Auto-cancelled"));
    }

    #[tokio::test]
    async fn tick_cascade_complete_then_activate() {
        let (db, scheduler, broadcaster) = setup().await;

        // Active reservation ending now (in the past)
        let active_id = insert_reservation(
            &db.pool,
            "user1",
            "active",
            "2020-01-01T00:00:00",
            "2020-01-01T01:00:00",
        )
        .await;
        scheduler
            .set_active_reservation(Some(ActiveReservation {
                reservation_id: active_id.clone(),
                user_id: "user1".to_string(),
                end_time: "2020-01-01T01:00:00".to_string(),
                user_display_name: None,
            }))
            .await;

        // Approved reservation starting now (in the past), ending in future
        let approved_id = insert_reservation(
            &db.pool,
            "user2",
            "approved",
            "2020-01-01T01:00:00",
            "2099-12-31T23:30:00",
        )
        .await;

        tick_reservations(&db.pool, &scheduler, &broadcaster).await;

        assert_eq!(get_status(&db.pool, &active_id).await, "completed");
        assert_eq!(get_status(&db.pool, &approved_id).await, "active");
        let active = scheduler.active_reservation().await.unwrap();
        assert_eq!(active.reservation_id, approved_id);
    }

    #[tokio::test]
    async fn tick_noop_when_nothing_due() {
        let (db, scheduler, broadcaster) = setup().await;
        let future_start = "2099-12-01T00:00:00";
        let future_end = "2099-12-31T23:30:00";

        let pending_id =
            insert_reservation(&db.pool, "user1", "pending", future_start, future_end).await;
        let approved_id =
            insert_reservation(&db.pool, "user2", "approved", future_start, future_end).await;

        // Subscribe before tick to check for no broadcast
        let mut rx = broadcaster.subscribe();

        tick_reservations(&db.pool, &scheduler, &broadcaster).await;

        assert_eq!(get_status(&db.pool, &pending_id).await, "pending");
        assert_eq!(get_status(&db.pool, &approved_id).await, "approved");
        assert!(scheduler.active_reservation().await.is_none());
        // No broadcast should have happened
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn tick_broadcasts_on_change() {
        let (db, scheduler, broadcaster) = setup().await;
        let past_start = "2020-01-01T00:00:00";
        let past_end = "2020-01-01T01:00:00";
        insert_reservation(&db.pool, "user1", "active", past_start, past_end).await;

        let mut rx = broadcaster.subscribe();

        tick_reservations(&db.pool, &scheduler, &broadcaster).await;

        // Should have received a notification
        assert!(rx.try_recv().is_ok());
    }

    #[tokio::test]
    async fn recover_loads_active_into_scheduler() {
        let (db, scheduler, _) = setup().await;
        let id = insert_reservation(
            &db.pool,
            "user1",
            "active",
            "2020-01-01T00:00:00",
            "2099-12-31T23:30:00",
        )
        .await;

        assert!(scheduler.active_reservation().await.is_none());

        recover_active_reservation(&db.pool, &scheduler).await;

        let active = scheduler.active_reservation().await.unwrap();
        assert_eq!(active.reservation_id, id);
        assert_eq!(active.user_id, "user1");
    }

    #[tokio::test]
    async fn recover_noop_when_no_active() {
        let (db, scheduler, _) = setup().await;

        // Add some non-active reservations
        insert_reservation(
            &db.pool,
            "user1",
            "pending",
            "2099-01-01T00:00:00",
            "2099-01-01T01:00:00",
        )
        .await;
        insert_reservation(
            &db.pool,
            "user1",
            "completed",
            "2020-01-01T00:00:00",
            "2020-01-01T01:00:00",
        )
        .await;

        recover_active_reservation(&db.pool, &scheduler).await;

        assert!(scheduler.active_reservation().await.is_none());
    }
}
