use anyhow::Result;

use crate::db::Database;

/// Runtime-configurable fairness and queue settings.
///
/// Loaded from the `settings` table, with compile-time defaults as fallback.
#[derive(Debug, Clone)]
pub struct FairnessSettings {
    /// Starting priority for all requests.
    pub base_priority: f64,
    /// Priority bonus per second of waiting.
    pub wait_weight: f64,
    /// Multiplier on the ln(usage) penalty.
    pub usage_weight: f64,
    /// Token divisor inside ln — higher = more lenient.
    pub usage_scale: f64,
    /// Rolling usage window (minutes).
    pub window_minutes: i64,
    /// Max seconds to hold a queued request before 429.
    pub queue_timeout_secs: u64,
}

impl Default for FairnessSettings {
    fn default() -> Self {
        Self {
            base_priority: 100.0,
            wait_weight: 1.0,
            usage_weight: 10.0,
            usage_scale: 1000.0,
            window_minutes: 60,
            queue_timeout_secs: 30,
        }
    }
}

/// Load settings from the DB `settings` table, falling back to defaults for missing keys.
pub async fn load_settings(db: &Database) -> Result<FairnessSettings> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(&db.pool)
        .await?;

    let mut settings = FairnessSettings::default();

    for (key, value) in &rows {
        match key.as_str() {
            "fairness_base_priority" => {
                if let Ok(v) = value.parse() {
                    settings.base_priority = v;
                }
            }
            "fairness_wait_weight" => {
                if let Ok(v) = value.parse() {
                    settings.wait_weight = v;
                }
            }
            "fairness_usage_weight" => {
                if let Ok(v) = value.parse() {
                    settings.usage_weight = v;
                }
            }
            "fairness_usage_scale" => {
                if let Ok(v) = value.parse() {
                    settings.usage_scale = v;
                }
            }
            "fairness_window_minutes" => {
                if let Ok(v) = value.parse() {
                    settings.window_minutes = v;
                }
            }
            "queue_timeout_secs" => {
                if let Ok(v) = value.parse() {
                    settings.queue_timeout_secs = v;
                }
            }
            _ => {} // Ignore unknown keys
        }
    }

    Ok(settings)
}

/// Persist a single setting to the DB.
pub async fn save_setting(db: &Database, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .execute(&db.pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[tokio::test]
    async fn load_defaults_from_migration() {
        let db = Database::test_db().await;
        let s = load_settings(&db).await.unwrap();
        let d = FairnessSettings::default();

        assert!((s.base_priority - d.base_priority).abs() < f64::EPSILON);
        assert!((s.wait_weight - d.wait_weight).abs() < f64::EPSILON);
        assert!((s.usage_weight - d.usage_weight).abs() < f64::EPSILON);
        assert!((s.usage_scale - d.usage_scale).abs() < f64::EPSILON);
        assert_eq!(s.window_minutes, d.window_minutes);
        assert_eq!(s.queue_timeout_secs, d.queue_timeout_secs);
    }

    #[tokio::test]
    async fn save_and_reload() {
        let db = Database::test_db().await;
        save_setting(&db, "fairness_base_priority", "42.0")
            .await
            .unwrap();

        let s = load_settings(&db).await.unwrap();
        assert!((s.base_priority - 42.0).abs() < f64::EPSILON);
        // Other settings unchanged
        let d = FairnessSettings::default();
        assert!((s.wait_weight - d.wait_weight).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn save_upsert_overwrites() {
        let db = Database::test_db().await;
        save_setting(&db, "fairness_wait_weight", "5.0")
            .await
            .unwrap();
        save_setting(&db, "fairness_wait_weight", "9.0")
            .await
            .unwrap();

        let s = load_settings(&db).await.unwrap();
        assert!((s.wait_weight - 9.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn unknown_keys_ignored() {
        let db = Database::test_db().await;
        save_setting(&db, "totally_bogus_key", "999").await.unwrap();

        let s = load_settings(&db).await.unwrap();
        let d = FairnessSettings::default();
        assert!((s.base_priority - d.base_priority).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn unparseable_value_keeps_default() {
        let db = Database::test_db().await;
        save_setting(&db, "fairness_base_priority", "not_a_number")
            .await
            .unwrap();

        let s = load_settings(&db).await.unwrap();
        let d = FairnessSettings::default();
        assert!((s.base_priority - d.base_priority).abs() < f64::EPSILON);
    }
}
