use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use tracing::error;

/// Max lengths for user-provided string fields.
pub const MAX_NAME: usize = 256;
pub const MAX_URL: usize = 2048;
pub const MAX_DESCRIPTION: usize = 4096;
pub const MAX_SECRET: usize = 4096;

/// Validate that a string field does not exceed the given max length.
/// Returns `Some(Response)` with a 400 error if it does, `None` if OK.
pub fn validate_len(field: &str, value: &str, max: usize) -> Option<Response> {
    if value.len() > max {
        return Some(
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("{field} exceeds maximum length of {max} characters")
                })),
            )
                .into_response(),
        );
    }
    None
}

/// Validate that an hf_repo value matches the expected HuggingFace `owner/model-name` format.
/// Prevents path traversal (e.g. `..`) and other unsafe patterns.
/// Returns `Some(Response)` with a 400 error if invalid, `None` if OK.
pub fn validate_hf_repo(value: &str) -> Option<Response> {
    // Must be "owner/repo" with only safe characters
    let valid = value.contains('/')
        && !value.contains("..")
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
    if !valid {
        return Some(
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "hf_repo must be in 'owner/model-name' format \
                              (alphanumeric, hyphens, underscores, dots)"
                })),
            )
                .into_response(),
        );
    }
    None
}

/// Return a generic 500 response, logging the real error server-side.
pub fn internal_error(context: &str, err: impl std::fmt::Display) -> Response {
    error!(context = context, error = %err, "Internal error");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "Internal server error" })),
    )
        .into_response()
}

/// Return a generic error response at the given status, logging the real error server-side.
pub fn api_error(status: StatusCode, context: &str, err: impl std::fmt::Display) -> Response {
    error!(context = context, error = %err, "API error");
    (
        status,
        Json(serde_json::json!({ "error": "Internal server error" })),
    )
        .into_response()
}
