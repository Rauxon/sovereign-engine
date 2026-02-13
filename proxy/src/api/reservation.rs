use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use chrono::{NaiveDateTime, Utc};
use serde::Deserialize;
use tracing::{error, info};
use uuid::Uuid;

use super::error;
use crate::auth::SessionAuth;
use crate::scheduler::reservation::{ActiveReservation, Reservation, ReservationWithUser};
use crate::AppState;

// ---------------------------------------------------------------------------
// User Routes
// ---------------------------------------------------------------------------

pub fn user_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/reservations", get(list_own).post(create))
        .route("/reservations/{id}/cancel", post(cancel_own))
        .route("/reservations/active", get(get_active))
        .route("/reservations/calendar", get(calendar))
        .route("/reservations/containers/start", post(user_start_container))
        .route("/reservations/containers/stop", post(user_stop_container))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Admin Routes
// ---------------------------------------------------------------------------

pub fn admin_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/reservations", get(admin_list))
        .route("/reservations/{id}/approve", post(approve))
        .route("/reservations/{id}/reject", post(reject))
        .route("/reservations/{id}/activate", post(force_activate))
        .route("/reservations/{id}/deactivate", post(force_deactivate))
        .route("/reservations/{id}", delete(admin_delete))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Shared Request Types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CreateReservationRequest {
    start_time: String,
    end_time: String,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AdminNoteRequest {
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContainerRequest {
    model_id: String,
    backend_type: Option<String>,
    gpu_type: Option<String>,
    gpu_layers: Option<u32>,
    context_size: Option<u32>,
    parallel: Option<u32>,
}

// ---------------------------------------------------------------------------
// Validation Helpers
// ---------------------------------------------------------------------------

fn parse_iso_time(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok()
}

fn is_30min_boundary(dt: &NaiveDateTime) -> bool {
    (dt.minute() == 0 || dt.minute() == 30) && dt.second() == 0
}

use chrono::Timelike;

// ---------------------------------------------------------------------------
// User Handlers
// ---------------------------------------------------------------------------

/// POST /api/user/reservations — Create a new reservation request.
async fn create(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
    Json(req): Json<CreateReservationRequest>,
) -> impl IntoResponse {
    // Parse and validate times
    let start = match parse_iso_time(&req.start_time) {
        Some(dt) => dt,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid start_time format (expected YYYY-MM-DDTHH:MM:SS)" })),
            ).into_response();
        }
    };
    let end = match parse_iso_time(&req.end_time) {
        Some(dt) => dt,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid end_time format (expected YYYY-MM-DDTHH:MM:SS)" })),
            ).into_response();
        }
    };

    if !is_30min_boundary(&start) || !is_30min_boundary(&end) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Times must be on 30-minute boundaries (minute 0 or 30, seconds 0)" })),
        ).into_response();
    }

    if end <= start {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "end_time must be after start_time" })),
        )
            .into_response();
    }

    let duration = end - start;
    if duration < chrono::Duration::minutes(30) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Minimum reservation duration is 30 minutes" })),
        )
            .into_response();
    }
    let now = Utc::now().naive_utc();
    if start <= now {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "start_time must be in the future" })),
        )
            .into_response();
    }

    // Check for overlap with approved/active reservations
    let overlap: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM reservations \
         WHERE status IN ('approved', 'active') \
         AND start_time < ? AND end_time > ? \
         LIMIT 1",
    )
    .bind(&req.end_time)
    .bind(&req.start_time)
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None);

    if overlap.is_some() {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "Time slot overlaps with an existing approved or active reservation" })),
        ).into_response();
    }

    let id = Uuid::new_v4().to_string();
    let reason = req.reason.unwrap_or_default();

    if let Some(r) = error::validate_len("reason", &reason, error::MAX_DESCRIPTION) {
        return r;
    }

    match sqlx::query(
        "INSERT INTO reservations (id, user_id, start_time, end_time, reason) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&session.user_id)
    .bind(&req.start_time)
    .bind(&req.end_time)
    .bind(&reason)
    .execute(&state.db.pool)
    .await
    {
        Ok(_) => {
            info!(target: "audit", action = "reservation.create", actor = %session.user_id, resource = %id, "User created reservation request");
            state.reservations.notify();
            (
                StatusCode::CREATED,
                Json(serde_json::json!({ "id": id, "status": "pending" })),
            ).into_response()
        }
        Err(e) => error::internal_error("reservation:create", e),
    }
}

/// GET /api/user/reservations — List own reservations.
async fn list_own(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
) -> impl IntoResponse {
    match sqlx::query_as::<_, Reservation>(
        "SELECT id, user_id, status, start_time, end_time, reason, admin_note, approved_by, created_at, updated_at \
         FROM reservations WHERE user_id = ? ORDER BY start_time DESC",
    )
    .bind(&session.user_id)
    .fetch_all(&state.db.pool)
    .await
    {
        Ok(rows) => Json(serde_json::json!({ "reservations": rows })).into_response(),
        Err(e) => error::internal_error("reservation:list_own", e),
    }
}

/// POST /api/user/reservations/:id/cancel — Cancel own pending/approved reservation.
async fn cancel_own(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match sqlx::query(
        "UPDATE reservations SET status = 'cancelled', updated_at = datetime('now') \
         WHERE id = ? AND user_id = ? AND status IN ('pending', 'approved')",
    )
    .bind(&id)
    .bind(&session.user_id)
    .execute(&state.db.pool)
    .await
    {
        Ok(result) => {
            if result.rows_affected() == 0 {
                (
                    StatusCode::NOT_FOUND,
                    Json(
                        serde_json::json!({ "error": "Reservation not found or not cancellable" }),
                    ),
                )
                    .into_response()
            } else {
                info!(target: "audit", action = "reservation.cancel", actor = %session.user_id, resource = %id, "User cancelled reservation");
                state.reservations.notify();
                Json(serde_json::json!({ "status": "cancelled" })).into_response()
            }
        }
        Err(e) => error::internal_error("reservation:cancel", e),
    }
}

/// GET /api/user/reservations/active — Get current active reservation (any user can check).
async fn get_active(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.scheduler.active_reservation().await {
        Some(active) => Json(serde_json::json!({
            "active": true,
            "reservation_id": active.reservation_id,
            "user_id": active.user_id,
            "user_display_name": active.user_display_name,
            "end_time": active.end_time,
        }))
        .into_response(),
        None => Json(serde_json::json!({ "active": false })).into_response(),
    }
}

/// GET /api/user/reservations/calendar — All approved+active+pending reservations for calendar display.
async fn calendar(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match sqlx::query_as::<_, ReservationWithUser>(
        "SELECT r.id, r.user_id, r.status, r.start_time, r.end_time, r.reason, r.admin_note, r.approved_by, r.created_at, r.updated_at, \
         u.email AS user_email, u.display_name AS user_display_name \
         FROM reservations r LEFT JOIN users u ON u.id = r.user_id \
         WHERE r.status IN ('approved', 'active', 'pending') \
         ORDER BY r.start_time ASC",
    )
    .fetch_all(&state.db.pool)
    .await
    {
        Ok(rows) => Json(serde_json::json!({ "reservations": rows })).into_response(),
        Err(e) => error::internal_error("reservation:calendar", e),
    }
}

/// POST /api/user/reservations/containers/start — Start a container (active reservation holder only).
async fn user_start_container(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
    Json(req): Json<ContainerRequest>,
) -> impl IntoResponse {
    // Verify caller holds the active reservation
    let active = match state.scheduler.active_reservation().await {
        Some(a) if a.user_id == session.user_id => a,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "You do not hold the active reservation" })),
            )
                .into_response();
        }
    };

    // Look up the model (same logic as admin start_container)
    #[allow(clippy::type_complexity)]
    let model: Option<(String, String, Option<String>, String, Option<i64>)> = match sqlx::query_as(
        "SELECT id, hf_repo, filename, backend_type, context_length FROM models WHERE id = ?",
    )
    .bind(&req.model_id)
    .fetch_optional(&state.db.pool)
    .await
    {
        Ok(m) => m,
        Err(e) => return error::internal_error("reservation:start_container:lookup", e),
    };

    let (model_id, hf_repo, filename, db_backend_type, db_context_length) = match model {
        Some(m) => m,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Model not found" })),
            )
                .into_response();
        }
    };

    let backend_type = req.backend_type.as_deref().unwrap_or(&db_backend_type);

    let uid = match state.docker.allocate_uid().await {
        Ok(uid) => uid,
        Err(e) => return error::internal_error("reservation:start_container:uid", e),
    };
    let api_key = Uuid::new_v4().to_string();

    let container_result = match backend_type {
        "llamacpp" => {
            let safe_repo = hf_repo.replace('/', "--");
            let gguf_path = match &filename {
                Some(f) => format!("{}/{}", safe_repo, f),
                None => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({ "error": "No filename recorded for this model" })),
                    )
                        .into_response();
                }
            };

            let parallel = req.parallel.unwrap_or(1).max(1);
            let llamacpp_config = crate::docker::llamacpp::LlamacppConfig {
                model_id: model_id.clone(),
                gguf_path,
                gpu_type: crate::docker::llamacpp::GpuType::from_str(
                    req.gpu_type.as_deref().unwrap_or("none"),
                ),
                gpu_layers: req.gpu_layers.unwrap_or(99),
                context_size: req
                    .context_size
                    .unwrap_or_else(|| db_context_length.map(|v| v as u32).unwrap_or(4096)),
                parallel,
                uid,
                api_key: api_key.clone(),
                ..Default::default()
            };
            state.docker.start_llamacpp(&llamacpp_config).await
        }
        other => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("Unknown backend type: {other}") })),
            )
                .into_response();
        }
    };

    match container_result {
        Ok(container_name) => {
            info!(target: "audit", action = "reservation.container.start", actor = %session.user_id, reservation = %active.reservation_id, resource = %model_id, "Reservation holder started container");

            let parallel_slots = req.parallel.unwrap_or(1).max(1);
            if let Err(e) = sqlx::query(
                "INSERT OR REPLACE INTO container_secrets (model_id, container_uid, api_key, parallel_slots) VALUES (?, ?, ?, ?)",
            )
            .bind(&model_id)
            .bind(uid as i64)
            .bind(&api_key)
            .bind(parallel_slots as i64)
            .execute(&state.db.pool)
            .await
            {
                error!(model = %model_id, error = %e, "Failed to persist container secrets");
            }

            state
                .scheduler
                .gate()
                .register(&model_id, parallel_slots)
                .await;

            let _ = sqlx::query("UPDATE models SET loaded = 1 WHERE id = ?")
                .bind(&model_id)
                .execute(&state.db.pool)
                .await;

            Json(serde_json::json!({
                "container": container_name,
                "url": state.docker.backend_base_url(&model_id, backend_type),
            }))
            .into_response()
        }
        Err(e) => {
            error!(model = %model_id, error = ?e, "Reservation holder failed to start container");
            error::internal_error("reservation:start_container", e)
        }
    }
}

/// POST /api/user/reservations/containers/stop — Stop a container (active reservation holder only).
async fn user_stop_container(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
    Json(req): Json<ContainerRequest>,
) -> impl IntoResponse {
    // Verify caller holds the active reservation
    let active = match state.scheduler.active_reservation().await {
        Some(a) if a.user_id == session.user_id => a,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "You do not hold the active reservation" })),
            )
                .into_response();
        }
    };

    let backend_type: String =
        match sqlx::query_as::<_, (String,)>("SELECT backend_type FROM models WHERE id = ?")
            .bind(&req.model_id)
            .fetch_optional(&state.db.pool)
            .await
        {
            Ok(Some((bt,))) => bt,
            Ok(None) => "llamacpp".to_string(),
            Err(_) => "llamacpp".to_string(),
        };

    match state
        .docker
        .stop_backend(&req.model_id, &backend_type)
        .await
    {
        Ok(_) => {
            info!(target: "audit", action = "reservation.container.stop", actor = %session.user_id, reservation = %active.reservation_id, resource = %req.model_id, "Reservation holder stopped container");

            state.scheduler.gate().unregister(&req.model_id).await;
            let _ = sqlx::query("DELETE FROM container_secrets WHERE model_id = ?")
                .bind(&req.model_id)
                .execute(&state.db.pool)
                .await;
            let _ = sqlx::query("UPDATE models SET loaded = 0 WHERE id = ?")
                .bind(&req.model_id)
                .execute(&state.db.pool)
                .await;

            Json(serde_json::json!({ "status": "stopped" })).into_response()
        }
        Err(e) => {
            error!(model = %req.model_id, error = %e, "Failed to stop container");
            error::internal_error("reservation:stop_container", e)
        }
    }
}

// ---------------------------------------------------------------------------
// Admin Handlers
// ---------------------------------------------------------------------------

/// GET /api/admin/reservations — List all reservations with user info.
async fn admin_list(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match sqlx::query_as::<_, ReservationWithUser>(
        "SELECT r.id, r.user_id, r.status, r.start_time, r.end_time, r.reason, r.admin_note, r.approved_by, r.created_at, r.updated_at, \
         u.email AS user_email, u.display_name AS user_display_name \
         FROM reservations r LEFT JOIN users u ON u.id = r.user_id \
         ORDER BY r.start_time DESC",
    )
    .fetch_all(&state.db.pool)
    .await
    {
        Ok(rows) => Json(serde_json::json!({ "reservations": rows })).into_response(),
        Err(e) => error::internal_error("reservation:admin_list", e),
    }
}

/// POST /api/admin/reservations/:id/approve — Approve a pending reservation.
async fn approve(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
    Path(id): Path<String>,
    Json(req): Json<AdminNoteRequest>,
) -> impl IntoResponse {
    let note = req.note.unwrap_or_default();
    if let Some(r) = super::error::validate_len("note", &note, super::error::MAX_DESCRIPTION) {
        return r;
    }

    // Check the reservation exists and is pending
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT status FROM reservations WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.db.pool)
            .await
            .unwrap_or(None);

    match existing {
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Reservation not found" })),
            )
                .into_response();
        }
        Some((status,)) if status != "pending" => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("Cannot approve reservation with status '{status}'") })),
            ).into_response();
        }
        _ => {}
    }

    // Check for overlap with other approved/active reservations
    let times: Option<(String, String)> =
        sqlx::query_as("SELECT start_time, end_time FROM reservations WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.db.pool)
            .await
            .unwrap_or(None);

    if let Some((start, end)) = times {
        let overlap: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM reservations \
             WHERE id != ? AND status IN ('approved', 'active') \
             AND start_time < ? AND end_time > ? \
             LIMIT 1",
        )
        .bind(&id)
        .bind(&end)
        .bind(&start)
        .fetch_optional(&state.db.pool)
        .await
        .unwrap_or(None);

        if overlap.is_some() {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "Approving would create an overlap with another approved/active reservation" })),
            ).into_response();
        }
    }

    match sqlx::query(
        "UPDATE reservations SET status = 'approved', admin_note = ?, approved_by = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&note)
    .bind(&session.user_id)
    .bind(&id)
    .execute(&state.db.pool)
    .await
    {
        Ok(_) => {
            info!(target: "audit", action = "reservation.approve", actor = %session.user_id, resource = %id, "Admin approved reservation");
            state.reservations.notify();
            Json(serde_json::json!({ "status": "approved" })).into_response()
        }
        Err(e) => error::internal_error("reservation:approve", e),
    }
}

/// POST /api/admin/reservations/:id/reject — Reject a pending reservation.
async fn reject(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
    Path(id): Path<String>,
    Json(req): Json<AdminNoteRequest>,
) -> impl IntoResponse {
    let note = req.note.unwrap_or_default();
    if let Some(r) = super::error::validate_len("note", &note, super::error::MAX_DESCRIPTION) {
        return r;
    }

    match sqlx::query(
        "UPDATE reservations SET status = 'rejected', admin_note = ?, updated_at = datetime('now') WHERE id = ? AND status = 'pending'",
    )
    .bind(&note)
    .bind(&id)
    .execute(&state.db.pool)
    .await
    {
        Ok(result) => {
            if result.rows_affected() == 0 {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Reservation not found or not pending" })),
                ).into_response()
            } else {
                info!(target: "audit", action = "reservation.reject", actor = %session.user_id, resource = %id, "Admin rejected reservation");
                state.reservations.notify();
                Json(serde_json::json!({ "status": "rejected" })).into_response()
            }
        }
        Err(e) => error::internal_error("reservation:reject", e),
    }
}

/// POST /api/admin/reservations/:id/activate — Force-activate an approved reservation now.
async fn force_activate(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Check no other reservation is currently active
    if state.scheduler.active_reservation().await.is_some() {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "Another reservation is already active" })),
        )
            .into_response();
    }

    // Fetch the reservation
    let row: Option<(String, String, String, Option<String>)> = match sqlx::query_as(
        "SELECT r.id, r.user_id, r.end_time, u.display_name \
         FROM reservations r LEFT JOIN users u ON u.id = r.user_id \
         WHERE r.id = ? AND r.status = 'approved'",
    )
    .bind(&id)
    .fetch_optional(&state.db.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => return error::internal_error("reservation:force_activate:lookup", e),
    };

    let (res_id, user_id, end_time, display_name) = match row {
        Some(r) => r,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Reservation not found or not approved" })),
            )
                .into_response();
        }
    };

    let _ = sqlx::query(
        "UPDATE reservations SET status = 'active', updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&res_id)
    .execute(&state.db.pool)
    .await;

    state
        .scheduler
        .set_active_reservation(Some(ActiveReservation {
            reservation_id: res_id.clone(),
            user_id,
            end_time,
            user_display_name: display_name,
        }))
        .await;

    info!(target: "audit", action = "reservation.force_activate", actor = %session.user_id, resource = %res_id, "Admin force-activated reservation");
    state.reservations.notify();
    Json(serde_json::json!({ "status": "active" })).into_response()
}

/// POST /api/admin/reservations/:id/deactivate — Force-end an active reservation.
async fn force_deactivate(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match sqlx::query(
        "UPDATE reservations SET status = 'completed', admin_note = 'Ended early by admin', updated_at = datetime('now') WHERE id = ? AND status = 'active'",
    )
    .bind(&id)
    .execute(&state.db.pool)
    .await
    {
        Ok(result) => {
            if result.rows_affected() == 0 {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Reservation not found or not active" })),
                ).into_response()
            } else {
                // Clear in-memory cache
                state.scheduler.set_active_reservation(None).await;
                info!(target: "audit", action = "reservation.deactivate", actor = %session.user_id, resource = %id, "Admin force-deactivated reservation");
                state.reservations.notify();
                Json(serde_json::json!({ "status": "completed" })).into_response()
            }
        }
        Err(e) => error::internal_error("reservation:deactivate", e),
    }
}

/// DELETE /api/admin/reservations/:id — Delete a reservation record.
async fn admin_delete(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionAuth>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Don't allow deleting active reservations — deactivate first
    let status: Option<(String,)> = sqlx::query_as("SELECT status FROM reservations WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db.pool)
        .await
        .unwrap_or(None);

    if let Some((s,)) = &status {
        if s == "active" {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Cannot delete an active reservation — deactivate it first" })),
            ).into_response();
        }
    }

    match sqlx::query("DELETE FROM reservations WHERE id = ?")
        .bind(&id)
        .execute(&state.db.pool)
        .await
    {
        Ok(result) => {
            if result.rows_affected() == 0 {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Reservation not found" })),
                )
                    .into_response()
            } else {
                info!(target: "audit", action = "reservation.delete", actor = %session.user_id, resource = %id, "Admin deleted reservation");
                state.reservations.notify();
                Json(serde_json::json!({ "status": "deleted" })).into_response()
            }
        }
        Err(e) => error::internal_error("reservation:delete", e),
    }
}
