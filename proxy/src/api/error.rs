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

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // validate_len
    // -----------------------------------------------------------------------

    #[test]
    fn validate_len_under_max_is_ok() {
        assert!(validate_len("name", "hello", 10).is_none());
    }

    #[test]
    fn validate_len_at_max_is_ok() {
        assert!(validate_len("name", "12345", 5).is_none());
    }

    #[test]
    fn validate_len_over_max_returns_error() {
        let resp = validate_len("name", "123456", 5);
        assert!(resp.is_some());
    }

    #[test]
    fn validate_len_empty_string_is_ok() {
        assert!(validate_len("name", "", 5).is_none());
    }

    #[test]
    fn validate_len_zero_max_empty_string_ok() {
        assert!(validate_len("name", "", 0).is_none());
    }

    #[test]
    fn validate_len_zero_max_nonempty_returns_error() {
        assert!(validate_len("name", "a", 0).is_some());
    }

    // -----------------------------------------------------------------------
    // validate_hf_repo
    // -----------------------------------------------------------------------

    #[test]
    fn validate_hf_repo_valid_simple() {
        assert!(validate_hf_repo("owner/model-name").is_none());
    }

    #[test]
    fn validate_hf_repo_valid_with_dots_and_underscores() {
        assert!(validate_hf_repo("Org_Name/Model.v2").is_none());
    }

    #[test]
    fn validate_hf_repo_path_traversal_rejected() {
        assert!(validate_hf_repo("../etc/passwd").is_some());
    }

    #[test]
    fn validate_hf_repo_double_dot_in_middle_rejected() {
        assert!(validate_hf_repo("owner/..sneaky").is_some());
    }

    #[test]
    fn validate_hf_repo_no_slash_rejected() {
        assert!(validate_hf_repo("justmodelname").is_some());
    }

    #[test]
    fn validate_hf_repo_empty_rejected() {
        assert!(validate_hf_repo("").is_some());
    }

    #[test]
    fn validate_hf_repo_invalid_chars_rejected() {
        assert!(validate_hf_repo("owner/model name").is_some()); // space
        assert!(validate_hf_repo("owner/model@v2").is_some()); // @
        assert!(validate_hf_repo("owner/model;rm").is_some()); // semicolon
    }

    #[test]
    fn validate_hf_repo_multiple_slashes_ok() {
        // The function only checks for presence of '/' and safe chars;
        // "a/b/c" passes the current validation.
        assert!(validate_hf_repo("a/b/c").is_none());
    }
}
