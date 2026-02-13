use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IdpConfig {
    pub id: String,
    pub name: String,
    pub issuer: String,
    pub client_id: String,
    pub client_secret_enc: String,
    pub scopes: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ModelCategory {
    pub id: String,
    pub name: String,
    pub description: String,
    pub preferred_model_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Model {
    pub id: String,
    pub hf_repo: String,
    pub filename: Option<String>,
    #[sqlx(default)]
    pub size_bytes: i64,
    pub category_id: Option<String>,
    pub loaded: bool,
    pub backend_port: Option<i32>,
    pub backend_type: String,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub context_length: Option<i64>,
    pub n_layers: Option<i64>,
    pub n_heads: Option<i64>,
    pub n_kv_heads: Option<i64>,
    pub embedding_length: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: String,
    pub idp_id: String,
    pub subject: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IdpConfigPublic {
    pub id: String,
    pub name: String,
    pub issuer: String,
    pub client_id: String,
    pub scopes: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TokenListItem {
    pub id: String,
    pub name: String,
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    pub specific_model_id: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked: bool,
    pub created_at: DateTime<Utc>,
}
