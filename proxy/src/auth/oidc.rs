use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use openidconnect::core::{
    CoreClient, CoreIdToken, CoreIdTokenClaims, CoreIdTokenVerifier, CoreProviderMetadata,
    CoreResponseType, CoreTokenResponse,
};
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet,
    EndpointNotSet, EndpointSet, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope, TokenResponse,
};
use tracing::{error, info};

use crate::auth::sessions;
use crate::db::models::IdpConfig;
use crate::db::Database;
use crate::AppState;

/// The concrete client type returned by `from_provider_metadata`:
/// - auth URL is always set (EndpointSet)
/// - token URL may or may not be present (EndpointMaybeSet)
/// - userinfo URL may or may not be present (EndpointMaybeSet)
/// - device auth, introspection, revocation are not set
type DiscoveredClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

/// Build OIDC + auth routes.
pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/login", get(login))
        .route("/callback", get(callback))
        .route("/providers", get(list_providers))
        .route("/me", get(me))
        .route("/logout", post(logout))
        .with_state(state)
}

/// GET /auth/providers — List enabled OIDC providers for the login page.
async fn list_providers(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let providers =
        sqlx::query_as::<_, (String, String)>("SELECT id, name FROM idp_configs WHERE enabled = 1")
            .fetch_all(&state.db.pool)
            .await;

    match providers {
        Ok(list) => {
            let data: Vec<serde_json::Value> = list
                .into_iter()
                .map(|(id, name)| serde_json::json!({ "id": id, "name": name }))
                .collect();
            Json(serde_json::json!({ "providers": data })).into_response()
        }
        Err(e) => {
            tracing::error!(context = "list_providers", error = %e, "Internal error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal server error" })),
            )
                .into_response()
        }
    }
}

#[derive(serde::Deserialize)]
struct LoginQuery {
    idp: String,
}

/// GET /auth/login?idp=<id> — Initiate OIDC authorization redirect.
async fn login(State(state): State<Arc<AppState>>, Query(query): Query<LoginQuery>) -> Response {
    let idp = match load_idp(&state.db, &query.idp).await {
        Ok(idp) => idp,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let client = match build_oidc_client(
        &idp,
        &state.config.external_url,
        state.config.db_encryption_key.as_deref(),
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, idp = %query.idp, "Failed to build OIDC client");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "OIDC provider configuration error" })),
            )
                .into_response();
        }
    };

    let scopes: Vec<Scope> = idp
        .scopes
        .split_whitespace()
        .map(|s| Scope::new(s.to_string()))
        .collect();

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let mut auth_request = client
        .authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .set_pkce_challenge(pkce_challenge);

    for scope in scopes {
        auth_request = auth_request.add_scope(scope);
    }

    let (auth_url, csrf_token, nonce) = auth_request.url();

    // Store CSRF, nonce, and PKCE verifier in DB for callback validation
    if let Err(e) = store_auth_state(
        &state.db,
        csrf_token.secret(),
        nonce.secret(),
        &query.idp,
        pkce_verifier.secret(),
    )
    .await
    {
        error!(error = %e, "Failed to store auth state");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to initiate login" })),
        )
            .into_response();
    }

    Redirect::temporary(auth_url.as_str()).into_response()
}

#[derive(serde::Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

/// GET /auth/callback — Handle OIDC authorization callback.
async fn callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CallbackQuery>,
) -> Response {
    // Look up the stored auth state
    let auth_state = match load_auth_state(&state.db, &query.state).await {
        Ok(s) => s,
        Err(e) => {
            error!(error = %e, "Invalid OIDC callback state");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid or expired login state" })),
            )
                .into_response();
        }
    };

    let idp = match load_idp(&state.db, &auth_state.idp_id).await {
        Ok(idp) => idp,
        Err(e) => {
            error!(error = %e, "Failed to load IdP for callback");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "OIDC configuration error" })),
            )
                .into_response();
        }
    };

    let client = match build_oidc_client(
        &idp,
        &state.config.external_url,
        state.config.db_encryption_key.as_deref(),
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "Failed to build OIDC client for callback");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "OIDC configuration error" })),
            )
                .into_response();
        }
    };

    let http_client = build_http_client();

    // Exchange code for tokens with PKCE verifier
    let pkce_verifier = PkceCodeVerifier::new(auth_state.pkce_verifier);
    let token_request = match client.exchange_code(AuthorizationCode::new(query.code)) {
        Ok(req) => req,
        Err(e) => {
            error!(error = %e, "OIDC token endpoint not configured");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Token endpoint not configured" })),
            )
                .into_response();
        }
    };

    let token_response: CoreTokenResponse = match token_request
        .set_pkce_verifier(pkce_verifier)
        .request_async(&http_client)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            error!(error = %e, "OIDC token exchange failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Token exchange failed" })),
            )
                .into_response();
        }
    };

    // Extract ID token claims
    let id_token: &CoreIdToken = match token_response.id_token() {
        Some(t) => t,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "No ID token in response" })),
            )
                .into_response();
        }
    };

    let nonce = Nonce::new(auth_state.nonce);
    let verifier: CoreIdTokenVerifier = client.id_token_verifier();
    let claims: &CoreIdTokenClaims = match id_token.claims(&verifier, &nonce) {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "ID token verification failed");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Token verification failed" })),
            )
                .into_response();
        }
    };

    let subject = claims.subject().to_string();
    let email = claims
        .email()
        .map(|e: &openidconnect::EndUserEmail| e.to_string());
    let display_name = claims
        .preferred_username()
        .map(|u: &openidconnect::EndUserUsername| u.to_string())
        .or_else(|| email.clone());

    // Upsert user
    let user_id = match upsert_user(
        &state.db,
        &auth_state.idp_id,
        &subject,
        email.as_deref(),
        display_name.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            error!(error = %e, "Failed to upsert user");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to create user record" })),
            )
                .into_response();
        }
    };

    info!(user_id = %user_id, subject = %subject, "OIDC login successful");

    // Create session
    let session_token = match sessions::create_session(&state.db, &user_id).await {
        Ok(t) => t,
        Err(e) => {
            error!(error = %e, "Failed to create session");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to create session" })),
            )
                .into_response();
        }
    };

    // Clean up auth state
    let _ = delete_auth_state(&state.db, &query.state).await;

    // Set cookie and redirect to dashboard
    let cookie = sessions::build_cookie(&session_token, 86400, state.config.secure_cookies);

    (
        [("set-cookie", cookie), ("location", "/".to_string())],
        StatusCode::FOUND,
    )
        .into_response()
}

/// GET /auth/me — Return current session user info.
/// Accepts either a session cookie or Basic auth (bootstrap credentials).
async fn me(State(state): State<Arc<AppState>>, headers: axum::http::HeaderMap) -> Response {
    // Try bootstrap Basic auth first
    if let Some(auth) = super::try_bootstrap_auth(&headers, &state.config, &state.db).await {
        // Create a session so subsequent requests work via cookie
        let session_token = match sessions::create_session(&state.db, &auth.user_id).await {
            Ok(t) => t,
            Err(e) => {
                error!(error = %e, "Failed to create session for bootstrap user");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Failed to create session" })),
                )
                    .into_response();
            }
        };

        let cookie = sessions::build_cookie(&session_token, 86400, state.config.secure_cookies);

        return (
            [("set-cookie", cookie)],
            Json(serde_json::json!({
                "user_id": auth.user_id,
                "email": auth.email,
                "display_name": auth.display_name,
                "is_admin": auth.is_admin,
            })),
        )
            .into_response();
    } else if headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|h| h.starts_with("Basic "))
    {
        // Basic auth was provided but invalid
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Invalid credentials" })),
        )
            .into_response();
    }

    // Fall back to session cookie
    let session_token = extract_session_token(&headers);

    let session_token = match session_token {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Authentication required" })),
            )
                .into_response();
        }
    };

    let session_user = match sessions::validate_session(&state.db, session_token).await {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Invalid or expired session" })),
            )
                .into_response();
        }
    };

    Json(serde_json::json!({
        "user_id": session_user.user_id,
        "email": session_user.email,
        "display_name": session_user.display_name,
        "is_admin": session_user.is_admin,
    }))
    .into_response()
}

/// POST /auth/logout — Clear session.
async fn logout(State(state): State<Arc<AppState>>, headers: axum::http::HeaderMap) -> Response {
    if let Some(token) = extract_session_token(&headers) {
        let _ = sessions::delete_session(&state.db, token).await;
    }

    // Clear the cookie regardless
    let clear_cookie = sessions::clear_cookie(state.config.secure_cookies);

    (
        [("set-cookie", clear_cookie)],
        Json(serde_json::json!({ "status": "logged_out" })),
    )
        .into_response()
}

/// Extract the session token from the Cookie header.
fn extract_session_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    let cookie_header = headers.get("cookie").and_then(|v| v.to_str().ok())?;

    cookie_header
        .split(';')
        .filter_map(|c| {
            let c = c.trim();
            c.strip_prefix(&format!("{}=", sessions::cookie_name()))
        })
        .next()
}

// --- Helper functions ---

/// Build a reqwest HTTP client suitable for OIDC operations.
fn build_http_client() -> openidconnect::reqwest::Client {
    openidconnect::reqwest::ClientBuilder::new()
        // Disable redirects to prevent SSRF
        .redirect(openidconnect::reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to build HTTP client")
}

async fn load_idp(db: &Database, idp_id: &str) -> Result<IdpConfig> {
    sqlx::query_as::<_, IdpConfig>(
        "SELECT id, name, issuer, client_id, client_secret_enc, scopes, enabled, created_at FROM idp_configs WHERE id = ? AND enabled = 1",
    )
    .bind(idp_id)
    .fetch_optional(&db.pool)
    .await?
    .context("IdP not found or disabled")
}

async fn build_oidc_client(
    idp: &IdpConfig,
    external_url: &str,
    encryption_key: Option<&str>,
) -> Result<DiscoveredClient> {
    let issuer_url = IssuerUrl::new(idp.issuer.clone()).context("Invalid issuer URL")?;
    let http_client = build_http_client();

    let provider_metadata = CoreProviderMetadata::discover_async(issuer_url, &http_client)
        .await
        .context("OIDC discovery failed")?;

    let redirect_url = RedirectUrl::new(format!("{}/auth/callback", external_url))
        .context("Invalid redirect URL")?;

    // Decrypt client secret if encryption key is configured
    let client_secret = match encryption_key {
        Some(key) => crate::db::crypto::decrypt(&idp.client_secret_enc, key)
            .context("Failed to decrypt IdP client secret")?,
        None => idp.client_secret_enc.clone(),
    };

    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        ClientId::new(idp.client_id.clone()),
        Some(ClientSecret::new(client_secret)),
    )
    .set_redirect_uri(redirect_url);

    Ok(client)
}

#[derive(Debug, sqlx::FromRow)]
struct AuthState {
    idp_id: String,
    nonce: String,
    pkce_verifier: String,
}

async fn store_auth_state(
    db: &Database,
    csrf_token: &str,
    nonce: &str,
    idp_id: &str,
    pkce_verifier: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO oidc_auth_state (csrf_token, nonce, idp_id, pkce_verifier, expires_at) VALUES (?, ?, ?, ?, datetime('now', '+10 minutes'))",
    )
    .bind(csrf_token)
    .bind(nonce)
    .bind(idp_id)
    .bind(pkce_verifier)
    .execute(&db.pool)
    .await?;
    Ok(())
}

async fn load_auth_state(db: &Database, csrf_token: &str) -> Result<AuthState> {
    sqlx::query_as::<_, AuthState>(
        "SELECT idp_id, nonce, pkce_verifier FROM oidc_auth_state WHERE csrf_token = ? AND expires_at > datetime('now')",
    )
    .bind(csrf_token)
    .fetch_optional(&db.pool)
    .await?
    .context("Invalid or expired auth state")
}

async fn delete_auth_state(db: &Database, csrf_token: &str) -> Result<()> {
    sqlx::query("DELETE FROM oidc_auth_state WHERE csrf_token = ?")
        .bind(csrf_token)
        .execute(&db.pool)
        .await?;
    Ok(())
}

async fn upsert_user(
    db: &Database,
    idp_id: &str,
    subject: &str,
    email: Option<&str>,
    display_name: Option<&str>,
) -> Result<String> {
    // Check if user exists
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM users WHERE idp_id = ? AND subject = ?")
            .bind(idp_id)
            .bind(subject)
            .fetch_optional(&db.pool)
            .await?;

    if let Some((id,)) = existing {
        // Update email/display_name
        sqlx::query("UPDATE users SET email = COALESCE(?, email), display_name = COALESCE(?, display_name) WHERE id = ?")
            .bind(email)
            .bind(display_name)
            .bind(&id)
            .execute(&db.pool)
            .await?;
        return Ok(id);
    }

    // Create new user
    let id = uuid::Uuid::new_v4().to_string();
    // First OIDC user for an IdP gets admin
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE idp_id != 'bootstrap'")
        .fetch_one(&db.pool)
        .await?;
    let is_admin = count.0 == 0;

    sqlx::query(
        "INSERT INTO users (id, idp_id, subject, email, display_name, is_admin) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(idp_id)
    .bind(subject)
    .bind(email)
    .bind(display_name)
    .bind(is_admin)
    .execute(&db.pool)
    .await?;

    if is_admin {
        info!(user_id = %id, "First OIDC user promoted to admin");
    }

    Ok(id)
}
