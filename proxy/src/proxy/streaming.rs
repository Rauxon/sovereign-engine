use axum::body::Body;
use axum::http::{Response, StatusCode};
use bytes::Bytes;
use futures::StreamExt;
use reqwest::Client;
use tracing::error;

/// Result of proxying to a backend. For non-streaming responses, includes
/// the raw body bytes so callers can extract usage data.
pub struct ProxyResult {
    pub response: Response<Body>,
    /// Raw response body bytes (only populated for non-streaming successful responses)
    pub body_bytes: Option<Bytes>,
}

/// Forward a request to an inference backend and stream the response back.
/// Handles both streaming (SSE) and non-streaming responses transparently.
/// If `api_key` is provided, sends `Authorization: Bearer <key>` to the backend.
pub async fn proxy_to_backend(
    client: &Client,
    backend_url: &str,
    body: Bytes,
    is_streaming: bool,
    api_key: Option<&str>,
) -> ProxyResult {
    let mut request = client
        .post(backend_url)
        .header("content-type", "application/json");

    if let Some(key) = api_key {
        request = request.header("authorization", format!("Bearer {}", key));
    }

    let response = match request.body(body).send().await {
        Ok(resp) => resp,
        Err(e) => {
            error!(error = %e, "Failed to connect to backend");
            return ProxyResult {
                response: Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "error": {
                                "message": "Backend unavailable",
                                "type": "server_error",
                                "code": "backend_unavailable"
                            }
                        })
                        .to_string(),
                    ))
                    .unwrap(),
                body_bytes: None,
            };
        }
    };

    let status = response.status();
    let headers = response.headers().clone();

    if is_streaming {
        // Stream SSE events back to the client
        let stream = response.bytes_stream().map(|chunk| {
            chunk.map_err(|e| {
                error!(error = %e, "Error streaming from backend");
                std::io::Error::other(e)
            })
        });

        let mut builder = Response::builder()
            .status(status.as_u16())
            .header("content-type", "text/event-stream")
            .header("cache-control", "no-cache")
            .header("connection", "keep-alive");

        // Preserve transfer-encoding if present
        if let Some(te) = headers.get("transfer-encoding") {
            builder = builder.header("transfer-encoding", te);
        }

        ProxyResult {
            response: builder.body(Body::from_stream(stream)).unwrap(),
            body_bytes: None,
        }
    } else {
        // Non-streaming: collect full response and forward
        match response.bytes().await {
            Ok(body_bytes) => {
                let mut builder = Response::builder().status(status.as_u16());

                if let Some(ct) = headers.get("content-type") {
                    builder = builder.header("content-type", ct);
                } else {
                    builder = builder.header("content-type", "application/json");
                }

                ProxyResult {
                    response: builder.body(Body::from(body_bytes.clone())).unwrap(),
                    body_bytes: Some(body_bytes),
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to read backend response body");
                ProxyResult {
                    response: Response::builder()
                        .status(StatusCode::BAD_GATEWAY)
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "error": {
                                    "message": "Failed to read backend response",
                                    "type": "server_error",
                                    "code": "backend_error"
                                }
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                    body_bytes: None,
                }
            }
        }
    }
}
