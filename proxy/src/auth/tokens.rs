use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use tracing::info;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::config::AppConfig;
use crate::db::Database;

/// Generate a new API token (plaintext UUID).
pub fn generate_token() -> String {
    format!("se-{}", Uuid::new_v4())
}

/// Hash a token for storage (SHA-256).
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Create a new token in the database. Returns the plaintext token (only shown once).
/// Defaults to 90-day expiry if `expires_in_days` is None.
pub async fn create_token(
    db: &Database,
    user_id: &str,
    name: &str,
    category_id: Option<&str>,
    specific_model_id: Option<&str>,
    expires_in_days: Option<i64>,
) -> Result<String> {
    let token = generate_token();
    let token_hash = hash_token(&token);
    let id = Uuid::new_v4().to_string();
    let days = expires_in_days.unwrap_or(90).clamp(1, 365);

    sqlx::query(
        "INSERT INTO tokens (id, user_id, name, token_hash, category_id, specific_model_id, expires_at) VALUES (?, ?, ?, ?, ?, ?, datetime('now', '+' || ? || ' days'))",
    )
    .bind(&id)
    .bind(user_id)
    .bind(name)
    .bind(&token_hash)
    .bind(category_id)
    .bind(specific_model_id)
    .bind(days)
    .execute(&db.pool)
    .await
    .context("Failed to create token")?;

    Ok(token)
}

/// Validate a Bearer token and return the associated user context.
pub async fn validate_token(db: &Database, token: &str) -> Result<AuthUser> {
    let token_hash = hash_token(token);

    let row = sqlx::query_as::<_, TokenWithUser>(
        r#"
        SELECT t.id as token_id, t.user_id, t.category_id, t.specific_model_id,
               t.revoked, t.expires_at, t.internal, u.is_admin
        FROM tokens t
        JOIN users u ON u.id = t.user_id
        WHERE t.token_hash = ?
        "#,
    )
    .bind(&token_hash)
    .fetch_optional(&db.pool)
    .await
    .context("Failed to query token")?;

    let row = match row {
        Some(r) => r,
        None => bail!("Invalid token"),
    };

    if row.revoked {
        bail!("Token has been revoked");
    }

    if let Some(ref expires_at) = row.expires_at {
        let naive = chrono::NaiveDateTime::parse_from_str(expires_at, "%Y-%m-%d %H:%M:%S")
            .context("Invalid expires_at")?;
        let expires = naive.and_utc();
        if expires < chrono::Utc::now() {
            bail!("Token has expired");
        }
    }

    Ok(AuthUser {
        user_id: row.user_id,
        token_id: row.token_id,
        category_id: row.category_id,
        specific_model_id: row.specific_model_id,
        is_admin: row.is_admin,
        is_internal: row.internal,
    })
}

/// Revoke a token by its ID.
pub async fn revoke_token(db: &Database, token_id: &str, user_id: &str) -> Result<()> {
    let result = sqlx::query("UPDATE tokens SET revoked = 1 WHERE id = ? AND user_id = ?")
        .bind(token_id)
        .bind(user_id)
        .execute(&db.pool)
        .await
        .context("Failed to revoke token")?;

    if result.rows_affected() == 0 {
        bail!("Token not found or not owned by user");
    }

    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
struct TokenWithUser {
    token_id: String,
    user_id: String,
    category_id: Option<String>,
    specific_model_id: Option<String>,
    revoked: bool,
    expires_at: Option<String>,
    internal: bool,
    is_admin: bool,
}

/// Result of resolving a `user` email to a meta token for usage attribution.
#[derive(Debug, Clone)]
pub struct MetaResolution {
    pub user_id: String,
    pub token_id: String,
}

/// Ensure a meta token exists for the given user, creating one if needed.
/// Returns the token ID (not the hash — meta tokens are never used for auth).
pub async fn ensure_meta_token(db: &Database, user_id: &str) -> Result<String> {
    // Check for an existing non-revoked meta token
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM tokens WHERE user_id = ? AND meta = 1 AND revoked = 0")
            .bind(user_id)
            .fetch_optional(&db.pool)
            .await
            .context("Failed to check existing meta token")?;

    if let Some((id,)) = existing {
        return Ok(id);
    }

    // Create a new meta token (random hash — never used for auth)
    let id = Uuid::new_v4().to_string();
    let dummy_hash = hash_token(&Uuid::new_v4().to_string());

    sqlx::query(
        "INSERT INTO tokens (id, user_id, name, token_hash, meta) VALUES (?, ?, 'Open WebUI', ?, 1)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(&dummy_hash)
    .execute(&db.pool)
    .await
    .context("Failed to create meta token")?;

    Ok(id)
}

/// Look up a user by email and return their meta token for usage attribution.
/// Returns `None` if no user with that email exists.
pub async fn resolve_meta_user(db: &Database, email: &str) -> Result<Option<MetaResolution>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = ? LIMIT 1")
        .bind(email)
        .fetch_optional(&db.pool)
        .await
        .context("Failed to look up user by email")?;

    let user_id = match row {
        Some((id,)) => id,
        None => return Ok(None),
    };

    let token_id = ensure_meta_token(db, &user_id).await?;
    Ok(Some(MetaResolution { user_id, token_id }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_token_has_correct_prefix_and_uuid() {
        let token = generate_token();
        assert!(token.starts_with("se-"), "token should start with 'se-'");
        let uuid_part = &token[3..];
        assert!(
            Uuid::parse_str(uuid_part).is_ok(),
            "part after 'se-' should be a valid UUID, got: {uuid_part}"
        );
    }

    #[test]
    fn hash_token_is_deterministic() {
        let hash1 = hash_token("se-test-token");
        let hash2 = hash_token("se-test-token");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn hash_token_output_is_64_char_hex() {
        let hash = hash_token("se-anything");
        assert_eq!(hash.len(), 64);
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "hash should be valid hex, got: {hash}"
        );
    }

    #[test]
    fn hash_token_different_inputs_produce_different_hashes() {
        let h1 = hash_token("se-token-a");
        let h2 = hash_token("se-token-b");
        assert_ne!(h1, h2);
    }
}

/// Ensure an internal API token exists for Open WebUI ↔ proxy communication.
///
/// If WEBUI_API_KEY is set, hashes it and ensures a matching token row exists.
/// If the key changed (hash mismatch), revokes old internal tokens and creates a new one.
/// The token is owned by the bootstrap admin user.
pub async fn ensure_internal_token(config: &AppConfig, db: &Database) -> Result<()> {
    let api_key = match &config.webui_api_key {
        Some(k) => k.clone(),
        None => {
            info!("WEBUI_API_KEY not set — skipping internal token provisioning");
            return Ok(());
        }
    };

    let key_hash = hash_token(&api_key);

    // Check if a non-revoked token with this hash already exists
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM tokens WHERE token_hash = ? AND revoked = 0 AND internal = 1",
    )
    .bind(&key_hash)
    .fetch_optional(&db.pool)
    .await
    .context("Failed to check existing internal token")?;

    if existing.is_some() {
        info!("Internal API token already registered");
        return Ok(());
    }

    // Revoke any previous internal tokens
    sqlx::query("UPDATE tokens SET revoked = 1 WHERE internal = 1")
        .execute(&db.pool)
        .await
        .context("Failed to revoke old internal tokens")?;

    // Ensure bootstrap user exists (internal token is owned by the bootstrap admin)
    let bootstrap_name = config.bootstrap_user.as_deref().unwrap_or("admin");
    let user_id = super::bootstrap::ensure_bootstrap_user(db, bootstrap_name).await?;

    // Insert new internal token
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO tokens (id, user_id, name, token_hash, internal) VALUES (?, ?, 'Open WebUI (internal)', ?, 1)",
    )
    .bind(&id)
    .bind(&user_id)
    .bind(&key_hash)
    .execute(&db.pool)
    .await
    .context("Failed to create internal token")?;

    info!("Internal API token provisioned for Open WebUI");
    Ok(())
}
