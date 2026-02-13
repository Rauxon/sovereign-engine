use anyhow::{Context, Result};
use uuid::Uuid;

use crate::db::Database;

/// A completed inference request to be logged.
pub struct UsageEntry<'a> {
    pub token_id: &'a str,
    pub user_id: &'a str,
    pub model_id: &'a str,
    pub category_id: Option<&'a str>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub latency_ms: i64,
    pub queued_ms: i64,
}

/// Log a completed inference request to the usage_log table.
pub async fn log_usage(db: &Database, entry: &UsageEntry<'_>) -> Result<()> {
    let id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO usage_log (id, token_id, user_id, model_id, category_id,
                               input_tokens, output_tokens, latency_ms, queued_ms)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(entry.token_id)
    .bind(entry.user_id)
    .bind(entry.model_id)
    .bind(entry.category_id)
    .bind(entry.input_tokens)
    .bind(entry.output_tokens)
    .bind(entry.latency_ms)
    .bind(entry.queued_ms)
    .execute(&db.pool)
    .await
    .context("Failed to insert usage log entry")?;

    Ok(())
}
