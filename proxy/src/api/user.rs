use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use futures::stream::{self, StreamExt};
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use tracing::info;

use super::error;
use crate::auth::tokens;
use crate::auth::SessionAuth;
use crate::db::models::{Model, ModelCategory, TokenListItem};
use crate::AppState;

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/tokens", get(list_tokens).post(create_token))
        .route("/tokens/{id}/revoke", post(revoke_token))
        .route("/usage", get(usage_stats))
        .route("/usage/timeline", get(usage_timeline))
        .route("/categories", get(list_categories))
        .route("/models", get(list_models))
        .route("/disk", get(disk_usage))
        .route("/events", get(unified_events))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Token Management
// ---------------------------------------------------------------------------

/// GET /api/user/tokens — List the authenticated user's API tokens.
async fn list_tokens(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
) -> impl IntoResponse {
    match sqlx::query_as::<_, TokenListItem>(
        "SELECT t.id, t.name, t.category_id, mc.name AS category_name, t.specific_model_id, t.expires_at, t.revoked, t.created_at FROM tokens t LEFT JOIN model_categories mc ON mc.id = t.category_id WHERE t.user_id = ? AND t.internal = 0 AND t.meta = 0",
    )
    .bind(&session.user_id)
    .fetch_all(&state.db.pool)
    .await
    {
        Ok(tokens) => Json(serde_json::json!({ "tokens": tokens })).into_response(),
        Err(e) => error::internal_error("list_tokens", e),
    }
}

#[derive(Debug, Deserialize)]
struct CreateTokenRequest {
    name: String,
    category_id: Option<String>,
    specific_model_id: Option<String>,
    expires_in_days: Option<i64>,
}

/// POST /api/user/tokens — Mint a new API token (default 90-day expiry).
async fn create_token(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
    Json(req): Json<CreateTokenRequest>,
) -> impl IntoResponse {
    if let Some(r) = error::validate_len("name", &req.name, error::MAX_NAME) {
        return r;
    }
    match tokens::create_token(
        &state.db,
        &session.user_id,
        &req.name,
        req.category_id.as_deref(),
        req.specific_model_id.as_deref(),
        req.expires_in_days,
    )
    .await
    {
        Ok(token) => {
            info!(target: "audit", action = "token.create", actor = %session.user_id, name = %req.name, "User created API token");
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "token": token,
                    "name": req.name,
                    "warning": "Save this token — it cannot be shown again."
                })),
            )
                .into_response()
        }
        Err(e) => error::internal_error("create_token", e),
    }
}

/// POST /api/user/tokens/:id/revoke — Revoke a token.
async fn revoke_token(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
    Path(token_id): Path<String>,
) -> impl IntoResponse {
    match tokens::revoke_token(&state.db, &token_id, &session.user_id).await {
        Ok(()) => {
            info!(target: "audit", action = "token.revoke", actor = %session.user_id, resource = %token_id, "User revoked API token");
            Json(serde_json::json!({ "status": "revoked" })).into_response()
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("not owned") {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Token not found" })),
                )
                    .into_response()
            } else {
                error::internal_error("revoke_token", e)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Usage Stats
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct UsageQuery {
    period: Option<String>,
}

/// GET /api/user/usage — Usage statistics for the authenticated user.
async fn usage_stats(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
    Query(params): Query<UsageQuery>,
) -> impl IntoResponse {
    let period = params.period.unwrap_or_else(|| "day".to_string());
    let interval = match period.as_str() {
        "hour" => "-1 hour",
        "day" => "-1 day",
        "week" => "-7 days",
        "month" => "-30 days",
        _ => "-1 day",
    };

    // Summary totals
    let summary: (i64, i64, i64) = sqlx::query_as(
        "SELECT COALESCE(COUNT(*), 0), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0) FROM usage_log WHERE user_id = ? AND created_at >= datetime('now', ?)",
    )
    .bind(&session.user_id)
    .bind(interval)
    .fetch_one(&state.db.pool)
    .await
    .unwrap_or((0, 0, 0));

    // Breakdown by model
    let by_model = sqlx::query_as::<_, (String, Option<String>, i64, i64, i64)>(
        r#"
        SELECT COALESCE(m.hf_repo, ul.model_id) as model_name, mc.name as category_name,
               COUNT(*) as requests,
               COALESCE(SUM(ul.input_tokens), 0) as input_tokens,
               COALESCE(SUM(ul.output_tokens), 0) as output_tokens
        FROM usage_log ul
        LEFT JOIN models m ON m.id = ul.model_id
        LEFT JOIN model_categories mc ON mc.id = m.category_id
        WHERE ul.user_id = ? AND ul.created_at >= datetime('now', ?)
        GROUP BY ul.model_id
        "#,
    )
    .bind(&session.user_id)
    .bind(interval)
    .fetch_all(&state.db.pool)
    .await
    .unwrap_or_default();

    let by_model_json: Vec<serde_json::Value> = by_model
        .into_iter()
        .map(
            |(model_id, category_name, requests, input_tokens, output_tokens)| {
                serde_json::json!({
                    "model_id": model_id,
                    "category_name": category_name,
                    "requests": requests,
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                })
            },
        )
        .collect();

    // Breakdown by API token
    let by_token = sqlx::query_as::<_, (String, i64, i64, i64)>(
        r#"
        SELECT COALESCE(t.name, 'Unknown') as token_name,
               COUNT(*) as requests,
               COALESCE(SUM(ul.input_tokens), 0) as input_tokens,
               COALESCE(SUM(ul.output_tokens), 0) as output_tokens
        FROM usage_log ul
        LEFT JOIN tokens t ON t.id = ul.token_id
        WHERE ul.user_id = ? AND ul.created_at >= datetime('now', ?)
        GROUP BY ul.token_id
        "#,
    )
    .bind(&session.user_id)
    .bind(interval)
    .fetch_all(&state.db.pool)
    .await
    .unwrap_or_default();

    let by_token_json: Vec<serde_json::Value> = by_token
        .into_iter()
        .map(|(token_name, requests, input_tokens, output_tokens)| {
            serde_json::json!({
                "token_name": token_name,
                "requests": requests,
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
            })
        })
        .collect();

    Json(serde_json::json!({
        "summary": {
            "total_requests": summary.0,
            "total_input_tokens": summary.1,
            "total_output_tokens": summary.2,
            "period": period,
        },
        "by_model": by_model_json,
        "by_token": by_token_json,
    }))
    .into_response()
}

/// GET /api/user/usage/timeline — Time-series usage data.
async fn usage_timeline(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
    Query(params): Query<UsageQuery>,
) -> impl IntoResponse {
    let period = params.period.unwrap_or_else(|| "day".to_string());
    let (interval, time_bucket) = match period.as_str() {
        "hour" => ("-1 hour", "%Y-%m-%dT%H:%M:00"),
        "day" => ("-1 day", "%Y-%m-%dT%H:00:00"),
        "week" => ("-7 days", "%Y-%m-%d"),
        "month" => ("-30 days", "%Y-%m-%d"),
        _ => ("-1 day", "%Y-%m-%dT%H:00:00"),
    };

    let timeline = sqlx::query_as::<_, (String, String, i64, i64, i64)>(&format!(
        r#"
            SELECT strftime('{}', ul.created_at) as ts,
                   COALESCE(m.hf_repo, ul.model_id) as model_name,
                   COUNT(*) as requests,
                   COALESCE(SUM(ul.input_tokens), 0) as input_tokens,
                   COALESCE(SUM(ul.output_tokens), 0) as output_tokens
            FROM usage_log ul
            LEFT JOIN models m ON m.id = ul.model_id
            WHERE ul.user_id = ? AND ul.created_at >= datetime('now', ?)
            GROUP BY ts, model_name
            ORDER BY ts
            "#,
        time_bucket
    ))
    .bind(&session.user_id)
    .bind(interval)
    .fetch_all(&state.db.pool)
    .await
    .unwrap_or_default();

    let timeline_json: Vec<serde_json::Value> = timeline
        .into_iter()
        .map(
            |(timestamp, model_name, requests, input_tokens, output_tokens)| {
                serde_json::json!({
                    "timestamp": timestamp,
                    "model": model_name,
                    "requests": requests,
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                })
            },
        )
        .collect();

    // Timeline by API token
    let timeline_by_token = sqlx::query_as::<_, (String, String, i64, i64, i64)>(&format!(
        r#"
            SELECT strftime('{}', ul.created_at) as ts,
                   COALESCE(t.name, 'Unknown') as token_name,
                   COUNT(*) as requests,
                   COALESCE(SUM(ul.input_tokens), 0) as input_tokens,
                   COALESCE(SUM(ul.output_tokens), 0) as output_tokens
            FROM usage_log ul
            LEFT JOIN tokens t ON t.id = ul.token_id
            WHERE ul.user_id = ? AND ul.created_at >= datetime('now', ?)
            GROUP BY ts, token_name
            ORDER BY ts
            "#,
        time_bucket
    ))
    .bind(&session.user_id)
    .bind(interval)
    .fetch_all(&state.db.pool)
    .await
    .unwrap_or_default();

    let timeline_by_token_json: Vec<serde_json::Value> = timeline_by_token
        .into_iter()
        .map(
            |(timestamp, token_name, requests, input_tokens, output_tokens)| {
                serde_json::json!({
                    "timestamp": timestamp,
                    "token_name": token_name,
                    "requests": requests,
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                })
            },
        )
        .collect();

    Json(serde_json::json!({
        "timeline": timeline_json,
        "timeline_by_token": timeline_by_token_json,
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// Categories (read-only for users)
// ---------------------------------------------------------------------------

/// GET /api/user/categories — List available categories.
async fn list_categories(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match sqlx::query_as::<_, ModelCategory>(
        "SELECT id, name, description, preferred_model_id, created_at FROM model_categories",
    )
    .fetch_all(&state.db.pool)
    .await
    {
        Ok(categories) => Json(serde_json::json!({ "categories": categories })).into_response(),
        Err(e) => error::internal_error("user:list_categories", e),
    }
}

// ---------------------------------------------------------------------------
// Models (read-only for users)
// ---------------------------------------------------------------------------

/// GET /api/user/models — List all registered models.
async fn list_models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match sqlx::query_as::<_, Model>(
        "SELECT id, hf_repo, filename, size_bytes, category_id, loaded, backend_port, backend_type, last_used_at, created_at, context_length, n_layers, n_heads, n_kv_heads, embedding_length FROM models",
    )
    .fetch_all(&state.db.pool)
    .await
    {
        Ok(models) => Json(serde_json::json!({ "models": models })).into_response(),
        Err(e) => error::internal_error("user:list_models", e),
    }
}

// ---------------------------------------------------------------------------
// Disk Usage
// ---------------------------------------------------------------------------

/// GET /api/user/disk — Disk usage for the model storage path.
async fn disk_usage(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match super::hf::get_disk_usage(&state.config.model_path) {
        Ok(d) => Json(serde_json::json!({
            "total_bytes": d.total_bytes,
            "used_bytes": d.used_bytes,
            "free_bytes": d.free_bytes,
        }))
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Unified SSE Stream (replaces per-concern SSE endpoints)
// ---------------------------------------------------------------------------

/// Merged SSE item from either the metrics or reservations broadcast channel.
enum UnifiedEvent {
    Metrics(Box<crate::metrics::MetricsSnapshot>),
    ReservationsChanged,
}

/// GET /api/user/events — Single SSE stream merging metrics + reservation signals.
///
/// Admins receive the full MetricsSnapshot as the `"metrics"` event.
/// Non-admin users receive only `gpu_memory`, `active_reservation`, and `timestamp`.
/// Reservation changes are sent as a data-less `"reservations_changed"` event.
async fn unified_events(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let is_admin = session.is_admin;

    let metrics_stream = BroadcastStream::new(state.metrics.subscribe())
        .filter_map(|r| async { r.ok().map(|s| UnifiedEvent::Metrics(Box::new(s))) });

    let reservation_stream = BroadcastStream::new(state.reservations.subscribe())
        .filter_map(|r| async { r.ok().map(|_| UnifiedEvent::ReservationsChanged) });

    let merged = stream::select(metrics_stream, reservation_stream).map(move |item| {
        Ok(match item {
            UnifiedEvent::Metrics(snapshot) => {
                let data = if is_admin {
                    serde_json::to_string(&snapshot).unwrap_or_default()
                } else {
                    serde_json::to_string(&serde_json::json!({
                        "gpu_memory": snapshot.gpu_memory,
                        "active_reservation": snapshot.active_reservation,
                        "timestamp": snapshot.timestamp,
                    }))
                    .unwrap_or_default()
                };
                Event::default().event("metrics").data(data)
            }
            UnifiedEvent::ReservationsChanged => Event::default().event("reservations_changed"),
        })
    });

    Sse::new(merged).keep_alive(KeepAlive::default())
}
