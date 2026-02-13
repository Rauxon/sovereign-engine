pub mod bootstrap;
pub mod oidc;
pub mod sessions;
pub mod tokens;

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine as _;

use crate::AppState;

/// Authenticated user context extracted from a valid Bearer token.
///
/// Fields are populated from the DB during token validation and consumed
/// by handlers via `Extension<AuthUser>`.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub token_id: String,
    pub category_id: Option<String>,
    pub specific_model_id: Option<String>,
    #[allow(dead_code)] // populated from DB; will be consumed by authorization middleware
    pub is_admin: bool,
    #[allow(dead_code)] // populated from DB; will be consumed by authorization middleware
    pub is_internal: bool,
}

/// Authenticated session user (from cookie).
#[derive(Debug, Clone)]
pub struct SessionAuth {
    pub user_id: String,
    pub is_admin: bool,
    pub email: Option<String>,
    pub display_name: Option<String>,
}

/// Middleware: validate Bearer token on /v1/* API requests.
pub async fn bearer_auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let bearer_token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let auth_user = tokens::validate_token(&state.db, bearer_token)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    req.extensions_mut().insert(auth_user);
    Ok(next.run(req).await)
}

/// Middleware: validate session cookie on /api/* portal requests.
pub async fn session_auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, Response> {
    // Try bootstrap auth from header first
    if let Some(auth_header) = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(basic) = auth_header.strip_prefix("Basic ") {
            if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(basic) {
                if let Ok(creds) = String::from_utf8(decoded) {
                    if let Some((user, pass)) = creds.split_once(':') {
                        if let Ok(user_id) =
                            bootstrap::validate_bootstrap(&state.config, &state.db, user, pass)
                                .await
                        {
                            req.extensions_mut().insert(SessionAuth {
                                user_id,
                                is_admin: true,
                                email: None,
                                display_name: Some(user.to_string()),
                            });
                            return Ok(next.run(req).await);
                        }
                    }
                }
            }
        }
    }

    // Try session cookie
    let cookie_header = req
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let session_token = cookie_header
        .split(';')
        .filter_map(|c| {
            let c = c.trim();
            c.strip_prefix(&format!("{}=", sessions::cookie_name()))
        })
        .next();

    let session_token = match session_token {
        Some(t) => t,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Authentication required" })),
            )
                .into_response());
        }
    };

    let session_user = sessions::validate_session(&state.db, session_token)
        .await
        .map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Invalid or expired session" })),
            )
                .into_response()
        })?;

    req.extensions_mut().insert(SessionAuth {
        user_id: session_user.user_id,
        is_admin: session_user.is_admin,
        email: session_user.email,
        display_name: session_user.display_name,
    });

    Ok(next.run(req).await)
}

/// Middleware: validate session for browser routes, redirecting to /portal if unauthenticated.
///
/// Unlike `session_auth_middleware` which returns 401 JSON, this variant:
/// - Redirects unauthenticated browser requests (Accept: text/html) to `/portal`
/// - Returns 401 JSON for API-style requests (XHR, fetch, etc.)
pub async fn session_auth_redirect_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, Response> {
    // Try bootstrap auth from header first
    if let Some(auth_header) = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(basic) = auth_header.strip_prefix("Basic ") {
            if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(basic) {
                if let Ok(creds) = String::from_utf8(decoded) {
                    if let Some((user, pass)) = creds.split_once(':') {
                        if let Ok(user_id) =
                            bootstrap::validate_bootstrap(&state.config, &state.db, user, pass)
                                .await
                        {
                            req.extensions_mut().insert(SessionAuth {
                                user_id,
                                is_admin: true,
                                email: None,
                                display_name: Some(user.to_string()),
                            });
                            return Ok(next.run(req).await);
                        }
                    }
                }
            }
        }
    }

    // Try session cookie
    let cookie_header = req
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let session_token = cookie_header
        .split(';')
        .filter_map(|c| {
            let c = c.trim();
            c.strip_prefix(&format!("{}=", sessions::cookie_name()))
        })
        .next();

    let session_token = match session_token {
        Some(t) => t,
        None => {
            return Err(unauth_response(&req));
        }
    };

    let session_user = sessions::validate_session(&state.db, session_token)
        .await
        .map_err(|_| unauth_response(&req))?;

    req.extensions_mut().insert(SessionAuth {
        user_id: session_user.user_id,
        is_admin: session_user.is_admin,
        email: session_user.email,
        display_name: session_user.display_name,
    });

    Ok(next.run(req).await)
}

/// Return a redirect for browser requests, or 401 JSON for API requests.
fn unauth_response(req: &Request) -> Response {
    let accepts_html = req
        .headers()
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("text/html"))
        .unwrap_or(false);

    if accepts_html {
        axum::response::Redirect::temporary("/portal").into_response()
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Authentication required" })),
        )
            .into_response()
    }
}

/// Middleware: require admin role (must be chained after session_auth_middleware).
pub async fn admin_only_middleware(req: Request, next: Next) -> Result<Response, Response> {
    let session = req.extensions().get::<SessionAuth>().ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Authentication required" })),
        )
            .into_response()
    })?;

    if !session.is_admin {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "Admin access required" })),
        )
            .into_response());
    }

    Ok(next.run(req).await)
}
