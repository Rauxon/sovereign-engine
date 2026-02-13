use anyhow::Result;
use chrono::Utc;

use super::settings::FairnessSettings;
use crate::db::Database;

/// Calculate a fair-use priority score for a request.
///
/// Formula: priority = base_priority + (wait_weight * wait_seconds) - (usage_weight * ln(1 + recent_tokens / usage_scale))
///
/// The ln() curve means:
/// - Small differences in low usage (200 vs 400 tokens) produce negligible penalty differences
/// - Large differences in high usage (200 vs 200,000 tokens) produce significant penalty differences
///
/// Higher score = higher priority (dequeued first).
pub fn calculate_priority(
    settings: &FairnessSettings,
    wait_seconds: f64,
    recent_tokens: i64,
) -> f64 {
    let wait_time_bonus = settings.wait_weight * wait_seconds;
    let usage_penalty =
        settings.usage_weight * (1.0 + recent_tokens as f64 / settings.usage_scale).ln();

    settings.base_priority + wait_time_bonus - usage_penalty
}

/// Query recent token usage for a user within a rolling window.
///
/// Returns total tokens (input + output) consumed in the last `window_minutes` minutes.
pub async fn get_recent_usage(db: &Database, user_id: &str, window_minutes: i64) -> Result<i64> {
    let cutoff = Utc::now() - chrono::Duration::minutes(window_minutes);
    let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT COALESCE(SUM(input_tokens + output_tokens), 0) as total
        FROM usage_log
        WHERE user_id = ? AND created_at >= ?
        "#,
    )
    .bind(user_id)
    .bind(&cutoff_str)
    .fetch_one(&db.pool)
    .await?;

    Ok(row.0)
}

/// Calculate priority for a user, querying their recent usage from the database.
pub async fn calculate_user_priority(
    db: &Database,
    settings: &FairnessSettings,
    user_id: &str,
    wait_seconds: f64,
) -> Result<f64> {
    let recent_tokens = get_recent_usage(db, user_id, settings.window_minutes).await?;
    Ok(calculate_priority(settings, wait_seconds, recent_tokens))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_settings() -> FairnessSettings {
        FairnessSettings::default()
    }

    #[test]
    fn test_priority_no_wait_no_usage() {
        let s = default_settings();
        let p = calculate_priority(&s, 0.0, 0);
        // ln(1 + 0/1000) = ln(1) = 0, so priority = 100
        assert!((p - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_priority_increases_with_wait() {
        let s = default_settings();
        let p1 = calculate_priority(&s, 0.0, 0);
        let p2 = calculate_priority(&s, 10.0, 0);
        assert!(p2 > p1);
    }

    #[test]
    fn test_priority_decreases_with_usage() {
        let s = default_settings();
        let p1 = calculate_priority(&s, 0.0, 0);
        let p2 = calculate_priority(&s, 0.0, 10_000);
        assert!(p2 < p1);
    }

    #[test]
    fn test_heavy_user_lower_than_light_user() {
        let s = default_settings();
        let heavy = calculate_priority(&s, 5.0, 50_000);
        let light = calculate_priority(&s, 5.0, 1_000);
        assert!(light > heavy);
    }

    #[test]
    fn test_ln_curve_small_differences_negligible() {
        let s = default_settings();
        let p200 = calculate_priority(&s, 0.0, 200);
        let p400 = calculate_priority(&s, 0.0, 400);
        // Difference should be small (< 1.0 priority point)
        assert!((p200 - p400).abs() < 2.0);
    }

    #[test]
    fn test_ln_curve_large_differences_significant() {
        let s = default_settings();
        let p200 = calculate_priority(&s, 0.0, 200);
        let p200k = calculate_priority(&s, 0.0, 200_000);
        // Difference should be significant (> 30 priority points with usage_weight=10)
        assert!((p200 - p200k) > 30.0);
    }

    // --- DB-dependent tests for get_recent_usage ---

    use crate::db::Database;

    /// Ensure a test user exists (with prerequisite idp_config row).
    async fn ensure_test_user(db: &Database, user_id: &str) {
        sqlx::query(
            "INSERT OR IGNORE INTO idp_configs (id, name, issuer, client_id, client_secret_enc)
             VALUES ('test-idp', 'test', 'https://test', 'client', 'secret')",
        )
        .execute(&db.pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT OR IGNORE INTO users (id, idp_id, subject, email)
             VALUES (?, 'test-idp', ?, ?)",
        )
        .bind(user_id)
        .bind(user_id)
        .bind(format!("{}@test.com", user_id))
        .execute(&db.pool)
        .await
        .unwrap();
    }

    /// Insert a usage_log row with a specific timestamp.
    async fn insert_usage(db: &Database, user_id: &str, input: i64, output: i64, created_at: &str) {
        ensure_test_user(db, user_id).await;
        sqlx::query(
            "INSERT INTO usage_log (id, user_id, model_id, input_tokens, output_tokens, created_at)
             VALUES (?, ?, 'test-model', ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(user_id)
        .bind(input)
        .bind(output)
        .bind(created_at)
        .execute(&db.pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn get_recent_usage_no_rows() {
        let db = Database::test_db().await;
        let total = get_recent_usage(&db, "nobody", 60).await.unwrap();
        assert_eq!(total, 0);
    }

    #[tokio::test]
    async fn get_recent_usage_sums_correctly() {
        let db = Database::test_db().await;
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        insert_usage(&db, "alice", 100, 50, &now).await;
        insert_usage(&db, "alice", 200, 100, &now).await;
        insert_usage(&db, "alice", 300, 150, &now).await;

        let total = get_recent_usage(&db, "alice", 60).await.unwrap();
        // (100+50) + (200+100) + (300+150) = 900
        assert_eq!(total, 900);
    }

    #[tokio::test]
    async fn get_recent_usage_respects_window() {
        let db = Database::test_db().await;
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let two_hours_ago = (Utc::now() - chrono::Duration::hours(2))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        insert_usage(&db, "bob", 100, 100, &now).await; // within window
        insert_usage(&db, "bob", 500, 500, &two_hours_ago).await; // outside 60-min window

        let total = get_recent_usage(&db, "bob", 60).await.unwrap();
        assert_eq!(total, 200); // only the recent row
    }

    #[tokio::test]
    async fn get_recent_usage_filters_by_user() {
        let db = Database::test_db().await;
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        insert_usage(&db, "carol", 100, 100, &now).await;
        insert_usage(&db, "dave", 999, 999, &now).await;

        let carol_total = get_recent_usage(&db, "carol", 60).await.unwrap();
        assert_eq!(carol_total, 200); // dave's usage not counted
    }
}
