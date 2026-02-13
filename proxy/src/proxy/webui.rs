use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use hyper_util::client::legacy::Client as HyperClient;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tracing::{debug, error};

use crate::auth::SessionAuth;
use crate::AppState;

/// Hop-by-hop headers that must not be forwarded (RFC 2616 §13.5.1).
/// `connection` and `upgrade` are deliberately excluded — they are needed
/// for WebSocket upgrades and are harmless for regular HTTP on an internal proxy.
const HOP_BY_HOP: &[&str] = &[
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
];

/// Reverse-proxy handler for Open WebUI.
///
/// Forwards all HTTP requests to the configured backend, injecting trusted
/// identity headers from the authenticated session. For WebSocket upgrades
/// (101 Switching Protocols), bridges the two upgraded connections so frames
/// flow transparently between client and backend.
pub async fn webui_proxy_handler(State(state): State<Arc<AppState>>, mut req: Request) -> Response {
    // Session is inserted by session_auth_redirect_middleware
    let session = match req.extensions_mut().remove::<SessionAuth>() {
        Some(s) => s,
        None => {
            return (StatusCode::UNAUTHORIZED, "Authentication required").into_response();
        }
    };

    // If system is reserved, only the reservation holder may use Open WebUI
    if let Some(active) = state.scheduler.active_reservation().await {
        if active.user_id != session.user_id {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "System is currently reserved for exclusive use",
            )
                .into_response();
        }
    }

    let is_upgrade = req
        .headers()
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));

    // Grab the server-side upgrade handle BEFORE forwarding the request.
    // When we later return a 101 response, hyper resolves this future with
    // the client's upgraded IO.
    let request_upgrade = if is_upgrade {
        Some(hyper::upgrade::on(&mut req))
    } else {
        None
    };

    let backend_base = state.config.webui_backend_url.trim_end_matches('/');
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");

    let backend_uri = match format!("{}{}", backend_base, path_and_query).parse::<hyper::Uri>() {
        Ok(uri) => uri,
        Err(e) => {
            error!(error = %e, "Failed to build backend URI");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Rewrite the request URI to point at the backend
    *req.uri_mut() = backend_uri;

    // Strip hop-by-hop and X-SE-* headers, inject trusted identity headers
    strip_and_inject_headers(req.headers_mut(), &session);

    // Forward using hyper
    let client = HyperClient::builder(TokioExecutor::new()).build_http::<Body>();

    match client.request(req).await {
        Ok(mut resp) => {
            // If the backend accepted a WebSocket upgrade, bridge the connections
            if resp.status() == StatusCode::SWITCHING_PROTOCOLS {
                if let Some(request_upgrade) = request_upgrade {
                    let response_upgrade = hyper::upgrade::on(&mut resp);

                    tokio::spawn(async move {
                        match tokio::try_join!(request_upgrade, response_upgrade) {
                            Ok((client_conn, backend_conn)) => {
                                let mut client_io = TokioIo::new(client_conn);
                                let mut backend_io = TokioIo::new(backend_conn);

                                match tokio::io::copy_bidirectional(&mut client_io, &mut backend_io)
                                    .await
                                {
                                    Ok((c2b, b2c)) => {
                                        debug!(
                                            client_to_backend = c2b,
                                            backend_to_client = b2c,
                                            "WebSocket proxy closed"
                                        );
                                    }
                                    Err(e) => {
                                        debug!(error = %e, "WebSocket proxy IO error");
                                    }
                                }
                            }
                            Err(e) => {
                                error!(error = %e, "WebSocket upgrade failed");
                            }
                        }
                    });
                }
            }

            resp.into_response()
        }
        Err(e) => {
            error!(error = %e, "WebUI backend unavailable");
            (StatusCode::BAD_GATEWAY, "Open WebUI backend unavailable").into_response()
        }
    }
}

/// Strip hop-by-hop and X-SE-* headers from the request, then inject
/// trusted identity headers from the authenticated session.
fn strip_and_inject_headers(headers: &mut HeaderMap, session: &SessionAuth) {
    // Remove hop-by-hop headers (connection/upgrade deliberately preserved)
    for name in HOP_BY_HOP {
        headers.remove(*name);
    }

    // Remove any incoming X-SE-* headers (prevent spoofing)
    let se_headers: Vec<_> = headers
        .keys()
        .filter(|k| k.as_str().starts_with("x-se-"))
        .cloned()
        .collect();
    for key in se_headers {
        headers.remove(&key);
    }

    // Inject trusted identity headers
    let email = session.email.as_deref().unwrap_or("");
    let name = session
        .display_name
        .as_deref()
        .or(session.email.as_deref())
        .unwrap_or(&session.user_id);

    if let Ok(v) = email.parse() {
        headers.insert("x-se-user-email", v);
    }
    if let Ok(v) = name.parse() {
        headers.insert("x-se-user-name", v);
    }
}
