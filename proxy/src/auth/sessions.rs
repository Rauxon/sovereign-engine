use anyhow::{bail, Context, Result};
use rand::RngExt;
use sha2::{Digest, Sha256};

use crate::db::Database;

const SESSION_COOKIE_NAME: &str = "se_session";
const SESSION_TTL_HOURS: i64 = 24;

/// Generate a random session token.
pub fn generate_session_token() -> String {
    let bytes: [u8; 32] = rand::rng().random();
    hex::encode(bytes)
}

fn hash_session(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Create a new session for a user. Returns the plaintext session token.
pub async fn create_session(db: &Database, user_id: &str) -> Result<String> {
    let token = generate_session_token();
    let token_hash = hash_session(&token);
    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO sessions (id, user_id, token_hash, expires_at) VALUES (?, ?, ?, datetime('now', '+' || ? || ' hours'))",
    )
    .bind(&id)
    .bind(user_id)
    .bind(&token_hash)
    .bind(SESSION_TTL_HOURS)
    .execute(&db.pool)
    .await
    .context("Failed to create session")?;

    Ok(token)
}

/// Validate a session token, return user_id if valid.
pub async fn validate_session(db: &Database, token: &str) -> Result<SessionUser> {
    let token_hash = hash_session(token);

    let row = sqlx::query_as::<_, SessionUser>(
        r#"
        SELECT s.id as session_id, s.user_id, u.is_admin, u.email, u.display_name
        FROM sessions s
        JOIN users u ON u.id = s.user_id
        WHERE s.token_hash = ? AND s.expires_at > datetime('now')
        "#,
    )
    .bind(&token_hash)
    .fetch_optional(&db.pool)
    .await
    .context("Failed to query session")?;

    match row {
        Some(r) => Ok(r),
        None => bail!("Invalid or expired session"),
    }
}

/// Delete a session (logout).
pub async fn delete_session(db: &Database, token: &str) -> Result<()> {
    let token_hash = hash_session(token);
    sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
        .bind(&token_hash)
        .execute(&db.pool)
        .await
        .context("Failed to delete session")?;
    Ok(())
}

/// Clean up expired sessions.
pub async fn cleanup_expired(db: &Database) -> Result<u64> {
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at < datetime('now')")
        .execute(&db.pool)
        .await
        .context("Failed to clean up sessions")?;
    Ok(result.rows_affected())
}

pub fn cookie_name() -> &'static str {
    SESSION_COOKIE_NAME
}

/// Build a Set-Cookie header value for the session cookie.
pub fn build_cookie(token: &str, max_age: i64, secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        SESSION_COOKIE_NAME, token, max_age, secure_flag
    )
}

/// Build a Set-Cookie header value that clears the session cookie.
pub fn clear_cookie(secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    format!(
        "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        SESSION_COOKIE_NAME, secure_flag
    )
}

/// Session-authenticated user, populated from a JOIN of sessions + users.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionUser {
    #[allow(dead_code)] // populated by sqlx; available for session management
    pub session_id: String,
    pub user_id: String,
    pub is_admin: bool,
    pub email: Option<String>,
    pub display_name: Option<String>,
}
