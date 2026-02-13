use anyhow::{bail, Result};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::db::Database;

/// Check if bootstrap credentials should be active.
/// Bootstrap creds are only active when BREAK_GLASS is true and credentials are configured.
pub fn is_bootstrap_active(config: &AppConfig) -> bool {
    config.break_glass && config.has_bootstrap_creds()
}

/// Validate bootstrap credentials and return a bootstrap admin user ID.
/// Creates the bootstrap user record if it doesn't exist.
pub async fn validate_bootstrap(
    config: &AppConfig,
    db: &Database,
    username: &str,
    password: &str,
) -> Result<String> {
    if !is_bootstrap_active(config) {
        bail!("Bootstrap authentication is not active");
    }

    if !config.validate_bootstrap_creds(username, password) {
        bail!("Invalid bootstrap credentials");
    }

    // Ensure bootstrap user exists in the database
    let user_id = ensure_bootstrap_user(db, username).await?;

    Ok(user_id)
}

/// Ensure a bootstrap admin user record exists.
pub async fn ensure_bootstrap_user(db: &Database, username: &str) -> Result<String> {
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM users WHERE subject = ? AND idp_id = 'bootstrap'")
            .bind(username)
            .fetch_optional(&db.pool)
            .await?;

    if let Some((id,)) = existing {
        return Ok(id);
    }

    // Create the bootstrap IdP config if it doesn't exist
    sqlx::query(
        "INSERT OR IGNORE INTO idp_configs (id, name, issuer, client_id, client_secret_enc, scopes, enabled) VALUES ('bootstrap', 'Bootstrap', 'local', 'bootstrap', '', '', 0)",
    )
    .execute(&db.pool)
    .await?;

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, idp_id, subject, display_name, is_admin) VALUES (?, 'bootstrap', ?, ?, 1)",
    )
    .bind(&id)
    .bind(username)
    .bind(username)
    .execute(&db.pool)
    .await?;

    Ok(id)
}
