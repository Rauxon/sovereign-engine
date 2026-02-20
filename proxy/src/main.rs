mod api;
mod auth;
mod config;
mod db;
mod docker;
mod metrics;
mod proxy;
mod scheduler;
mod tls;

#[cfg(test)]
mod meta_token_tests;
#[cfg(test)]
mod reservation_tests;

use std::sync::{Arc, OnceLock};

use anyhow::Result;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Method, StatusCode};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::Router;
use base64::Engine;
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

/// CSP header value computed at startup from the built index.html.
/// Falls back to a hardcoded hash when the UI bundle is absent (dev mode).
static CSP_HEADER: OnceLock<String> = OnceLock::new();

use crate::config::AppConfig;
use crate::db::Database;
use crate::docker::DockerManager;
use crate::metrics::MetricsBroadcaster;
use crate::scheduler::reservation::ReservationBroadcaster;
use crate::scheduler::Scheduler;

/// Shared application state available to all handlers.
pub struct AppState {
    pub config: AppConfig,
    pub db: Database,
    pub docker: DockerManager,
    pub scheduler: Scheduler,
    pub metrics: MetricsBroadcaster,
    pub reservations: ReservationBroadcaster,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env if present (not required)
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sovereign_engine=info,tower_http=info".into()),
        )
        .init();

    info!("Starting Sovereign Engine v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = AppConfig::from_env()?;
    info!(listen_addr = %config.listen_addr, "Configuration loaded");

    // Initialize database
    let db = Database::connect(&config.database_url).await?;
    db.migrate().await?;
    info!("Database initialized");

    // Provision internal API token (for Open WebUI → proxy /v1 calls)
    auth::tokens::ensure_internal_token(&config, &db).await?;

    // Backfill GGUF metadata for models missing architecture info
    backfill_gguf_metadata(&db, &config).await;

    // Initialize Docker manager
    let docker = DockerManager::new(&config).await?;
    info!("Docker manager initialized");

    // Pull backend images in the background (non-blocking)
    docker.pull_backend_images().await;

    // Initialize scheduler and load settings from DB
    let scheduler = Scheduler::new();
    if let Err(e) = scheduler.reload_settings(&db).await {
        warn!("Failed to load scheduler settings from DB: {e}");
    }

    // Recover concurrency gate state from DB for any containers still running
    recover_gate_state(&scheduler, &db).await;

    // NOTE: active reservation recovery happens after Arc<AppState> is built (below)

    // Initialize metrics broadcaster
    let metrics = MetricsBroadcaster::new();

    // Initialize reservation change broadcaster
    let reservations_broadcaster = ReservationBroadcaster::new();

    // Build shared state
    let state = Arc::new(AppState {
        config: config.clone(),
        db,
        docker,
        scheduler,
        metrics,
        reservations: reservations_broadcaster,
    });

    // Start background metrics collection (broadcasts every 2s)
    state.metrics.spawn_collector(
        state.docker.clone(),
        state.scheduler.clone(),
        state.config.model_path.clone(),
    );

    // Recover active reservation from DB (if proxy restarted during a reservation)
    scheduler::reservation::recover_active_reservation(&state.db.pool, &state.scheduler).await;

    // Spawn reservation tick task (every 30s)
    {
        let pool = state.db.pool.clone();
        let sched = state.scheduler.clone();
        let res_broadcaster = state.reservations.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            interval.tick().await; // first tick is immediate — skip it
            loop {
                interval.tick().await;
                scheduler::reservation::tick_reservations(&pool, &sched, &res_broadcaster).await;
            }
        });
    }

    // Warn about insecure bootstrap credential defaults
    if config.break_glass {
        if config.bootstrap_user.as_deref() == Some("admin")
            && config.bootstrap_password.as_deref() == Some("changeme")
        {
            warn!(
                "BREAK_GLASS is enabled with default credentials (admin/changeme). \
                   Change BOOTSTRAP_USER and BOOTSTRAP_PASSWORD for any non-local deployment."
            );
        } else {
            warn!(
                "BREAK_GLASS is enabled — bootstrap credentials are active. \
                   Disable after configuring an OIDC identity provider."
            );
        }
    }

    // Encrypt plaintext IdP secrets if encryption key is configured
    if let Some(ref key) = config.db_encryption_key {
        let old_key = config.db_encryption_key_old.as_deref();
        if let Err(e) = db::crypto::migrate_plaintext_secrets(&state.db, key, old_key).await {
            error!(error = %e, "Failed to migrate IdP secrets to encrypted form");
        }
    } else {
        warn!("DB_ENCRYPTION_KEY not set — IdP client secrets stored in plaintext");
    }

    // Spawn hourly session/state cleanup
    {
        let db = state.db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
            interval.tick().await; // first tick is immediate — skip it
            loop {
                interval.tick().await;
                if let Ok(n) = auth::sessions::cleanup_expired(&db).await {
                    if n > 0 {
                        info!(deleted = n, "Cleaned up expired sessions");
                    }
                }
                // Also clean expired OIDC auth state
                let _ =
                    sqlx::query("DELETE FROM oidc_auth_state WHERE expires_at < datetime('now')")
                        .execute(&db.pool)
                        .await;
            }
        });
    }

    // Compute CSP hashes from built index.html (or fall back to hardcoded)
    init_csp_header(&config.ui_path);

    // Build router
    let app = build_router(state.clone());

    // Start server
    let addr = config.listen_addr.parse::<std::net::SocketAddr>()?;

    if let Some(acme) = config.acme_config()? {
        if config.tls_cert_path.is_some() || config.tls_key_path.is_some() {
            warn!("ACME is enabled — ignoring TLS_CERT_PATH/TLS_KEY_PATH");
        }
        info!(
            "Starting HTTPS server on {} with ACME (domains: {:?})",
            addr, acme.domains
        );
        tls::serve_acme(app, addr, &acme.domains, &acme.contact, acme.staging).await?;
    } else if config.tls_cert_path.is_some() && config.tls_key_path.is_some() {
        info!("Starting HTTPS server on {} with manual TLS", addr);
        tls::serve_tls(app, addr, &config).await?;
    } else {
        info!("Starting HTTP server on {} (no TLS configured)", addr);
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
    }

    Ok(())
}

/// Backfill GGUF architecture metadata for models that have NULL metadata columns.
/// Scans GGUF files on disk and updates the DB.
async fn backfill_gguf_metadata(db: &Database, config: &AppConfig) {
    let rows: Vec<(String, String, Option<String>)> = match sqlx::query_as(
        "SELECT id, hf_repo, filename FROM models WHERE n_layers IS NULL AND filename IS NOT NULL",
    )
    .fetch_all(&db.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to query models for GGUF backfill: {e}");
            return;
        }
    };

    if rows.is_empty() {
        return;
    }

    info!(count = rows.len(), "Backfilling GGUF metadata for models");

    for (model_id, hf_repo, filename) in &rows {
        let filename = match filename {
            Some(f) if f.ends_with(".gguf") => f,
            _ => continue,
        };

        let safe_repo = hf_repo.replace('/', "--");
        let gguf_path = format!("{}/{}/{}", config.model_path, safe_repo, filename);

        match api::hf::read_gguf_metadata(&gguf_path).await {
            Ok(meta) => {
                if let Err(e) = sqlx::query(
                    "UPDATE models SET context_length = COALESCE(context_length, ?), n_layers = ?, n_heads = ?, n_kv_heads = ?, embedding_length = ? WHERE id = ?",
                )
                .bind(meta.context_length.map(|v| v as i64))
                .bind(meta.block_count.map(|v| v as i64))
                .bind(meta.head_count.map(|v| v as i64))
                .bind(meta.head_count_kv.map(|v| v as i64))
                .bind(meta.embedding_length.map(|v| v as i64))
                .bind(model_id)
                .execute(&db.pool)
                .await
                {
                    error!(model = %model_id, error = %e, "Failed to update GGUF metadata");
                } else {
                    info!(model = %model_id, "Backfilled GGUF metadata");
                }
            }
            Err(e) => {
                warn!(model = %model_id, path = %gguf_path, error = %e, "Failed to read GGUF for backfill");
            }
        }
    }
}

/// Re-register concurrency gates for containers that survived a proxy restart.
async fn recover_gate_state(scheduler: &Scheduler, db: &Database) {
    let rows: Vec<(String, i64)> = match sqlx::query_as(
        "SELECT cs.model_id, cs.parallel_slots FROM container_secrets cs JOIN models m ON m.id = cs.model_id WHERE m.loaded = 1",
    )
    .fetch_all(&db.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to query container_secrets for gate recovery: {e}");
            return;
        }
    };

    if rows.is_empty() {
        return;
    }

    for (model_id, parallel_slots) in &rows {
        let slots = (*parallel_slots).max(1) as u32;
        scheduler.gate().register(model_id, slots).await;
        info!(model = %model_id, slots, "Recovered gate state");
    }

    info!(count = rows.len(), "Gate state recovered from DB");
}

fn build_router(state: Arc<AppState>) -> Router {
    // OIDC auth routes (no auth required)
    let auth_routes = auth::oidc::routes(state.clone());

    // Portal API routes (session auth required)
    let api_routes = api::routes(state.clone()).layer(middleware::from_fn_with_state(
        state.clone(),
        auth::session_auth_middleware,
    ));

    // OpenAI-compatible routes (bearer token auth required)
    let openai_routes = api::openai::routes(state.clone()).layer(middleware::from_fn_with_state(
        state.clone(),
        auth::bearer_auth_middleware,
    ));

    let ui_path = state.config.ui_path.clone();

    // Open WebUI reverse proxy (session auth with redirect for browsers).
    let webui_fallback = Router::new()
        .fallback(proxy::webui::webui_proxy_handler)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::session_auth_redirect_middleware,
        ))
        .with_state(state.clone());

    let shared_layers = |router: Router| -> Router {
        router
            .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10 MB
            .layer(middleware::from_fn(security_headers))
            .layer(TraceLayer::new_for_http())
            .layer(CompressionLayer::new())
            .layer(build_cors_layer(&state.config))
    };

    // When both hostnames are the same (dev mode / unconfigured), build a combined
    // router that preserves the pre-subdomain layout: API routes + Open WebUI fallback.
    if state.config.api_hostname == state.config.chat_hostname {
        return shared_layers(
            Router::new()
                .nest("/auth", auth_routes)
                .nest("/api", api_routes)
                .nest("/v1", openai_routes)
                .nest_service(
                    "/portal",
                    tower_http::services::ServeDir::new(&ui_path).fallback(
                        tower_http::services::ServeFile::new(format!("{}/index.html", ui_path)),
                    ),
                )
                .fallback_service(webui_fallback),
        );
    }

    // Subdomain mode: separate API and Chat routers dispatched by Host header.
    let api_router = Router::new()
        .route(
            "/",
            axum::routing::get(|| async { axum::response::Redirect::permanent("/portal/") }),
        )
        .nest("/auth", auth_routes)
        .nest("/api", api_routes)
        .nest("/v1", openai_routes)
        .nest_service(
            "/portal",
            tower_http::services::ServeDir::new(&ui_path).fallback(
                tower_http::services::ServeFile::new(format!("{}/index.html", ui_path)),
            ),
        );

    let api_hostname = state.config.api_hostname.clone();
    let chat_hostname = state.config.chat_hostname.clone();

    shared_layers(
        Router::new()
            .fallback(move |req: axum::extract::Request| {
                let api_router = api_router.clone();
                let chat_router = webui_fallback.clone();
                let api_host = api_hostname.clone();
                let chat_host = chat_hostname.clone();
                async move {
                    let host = req
                        .headers()
                        .get("host")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .split(':')
                        .next()
                        .unwrap_or("");

                    if host == chat_host {
                        chat_router.oneshot(req).await.into_response()
                    } else if host == api_host {
                        api_router.oneshot(req).await.into_response()
                    } else {
                        (StatusCode::MISDIRECTED_REQUEST, "421 Misdirected Request").into_response()
                    }
                }
            })
            .with_state(state.clone()),
    )
}

fn build_cors_layer(config: &AppConfig) -> CorsLayer {
    let api_origin = config
        .api_external_url()
        .parse::<HeaderValue>()
        .unwrap_or_else(|_| HeaderValue::from_static("http://localhost:3000"));

    let chat_origin = config
        .chat_external_url()
        .parse::<HeaderValue>()
        .unwrap_or_else(|_| HeaderValue::from_static("http://localhost:3000"));

    use tower_http::cors::AllowOrigin;

    CorsLayer::new()
        .allow_origin(AllowOrigin::list([api_origin, chat_origin]))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
            axum::http::header::ACCEPT,
        ])
        .allow_credentials(true)
}

/// Extract SHA-256 hashes of inline `<script>` blocks from the built index.html
/// and construct the full CSP header value. Falls back to a hardcoded hash if
/// the file is missing (e.g. dev mode without a built UI).
fn init_csp_header(ui_path: &str) {
    let index_path = format!("{}/index.html", ui_path);

    let hashes = match std::fs::read_to_string(&index_path) {
        Ok(html) => extract_inline_script_hashes(&html),
        Err(_) => {
            warn!(
                path = %index_path,
                "index.html not found — using hardcoded CSP hash (dev mode)"
            );
            vec!["sha256-CNK91oXKaUIpki3MXfrcGislo8qcATLtfVWO7y4j0rM=".to_string()]
        }
    };

    let hash_directives: Vec<String> = hashes.iter().map(|h| format!("'{h}'")).collect();
    let csp = format!(
        "default-src 'self'; script-src 'self' {}; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'",
        hash_directives.join(" ")
    );

    info!(hashes = ?hashes, "CSP script-src hashes computed");
    CSP_HEADER.set(csp).ok();
}

/// Parse HTML and return base64-encoded SHA-256 hashes for each inline script block.
fn extract_inline_script_hashes(html: &str) -> Vec<String> {
    let mut hashes = Vec::new();
    let engine = base64::engine::general_purpose::STANDARD;

    // Simple non-greedy extraction of <script>...</script> blocks (no src attribute).
    // This is intentionally simple — we only need to handle the known Vite output.
    let mut search_from = 0;
    while let Some(open_start) = html[search_from..].find("<script>") {
        let abs_open = search_from + open_start;
        let content_start = abs_open + "<script>".len();
        if let Some(close_offset) = html[content_start..].find("</script>") {
            let content = &html[content_start..content_start + close_offset];
            let digest = Sha256::digest(content.as_bytes());
            let b64 = engine.encode(digest);
            hashes.push(format!("sha256-{b64}"));
            search_from = content_start + close_offset + "</script>".len();
        } else {
            break;
        }
    }

    if hashes.is_empty() {
        warn!("No inline <script> blocks found in index.html — CSP may block scripts");
    }

    hashes
}

async fn security_headers(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let is_portal = req.uri().path().starts_with("/portal");
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert(
        "strict-transport-security",
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );
    headers.insert(
        "referrer-policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
    );
    // Only apply restrictive CSP to our own portal UI; proxied apps (e.g.
    // Open WebUI) set their own CSP and need inline scripts to function.
    if is_portal {
        if let Some(csp) = CSP_HEADER.get() {
            if let Ok(val) = HeaderValue::from_str(csp) {
                headers.insert("content-security-policy", val);
            }
        }
    }
    response
}

#[cfg(test)]
mod csp_tests {
    use super::*;

    #[test]
    fn no_scripts_returns_empty_vec() {
        let html = "<html><body><p>Hello</p></body></html>";
        let hashes = extract_inline_script_hashes(html);
        assert!(hashes.is_empty());
    }

    #[test]
    fn single_inline_script_returns_one_hash() {
        let html = r#"<html><head><script>console.log("hi")</script></head></html>"#;
        let hashes = extract_inline_script_hashes(html);
        assert_eq!(hashes.len(), 1);
        assert!(hashes[0].starts_with("sha256-"));
    }

    #[test]
    fn single_inline_script_hash_is_deterministic() {
        let html = "<script>var x = 1;</script>";
        let h1 = extract_inline_script_hashes(html);
        let h2 = extract_inline_script_hashes(html);
        assert_eq!(h1, h2);
    }

    #[test]
    fn multiple_inline_scripts_returns_all_hashes() {
        let html = r#"
            <script>var a = 1;</script>
            <script>var b = 2;</script>
            <script>var c = 3;</script>
        "#;
        let hashes = extract_inline_script_hashes(html);
        assert_eq!(hashes.len(), 3);
        // All hashes should be distinct (different content)
        assert_ne!(hashes[0], hashes[1]);
        assert_ne!(hashes[1], hashes[2]);
        assert_ne!(hashes[0], hashes[2]);
    }

    #[test]
    fn script_with_src_attribute_is_not_matched() {
        // The parser looks for "<script>" exactly — a <script src="..."> tag won't match
        let html = r#"<script src="app.js"></script><script>inline()</script>"#;
        let hashes = extract_inline_script_hashes(html);
        assert_eq!(hashes.len(), 1);
        // The hash should be for "inline()" not the src tag
    }

    #[test]
    fn empty_inline_script_still_hashed() {
        let html = "<script></script>";
        let hashes = extract_inline_script_hashes(html);
        assert_eq!(hashes.len(), 1);
        assert!(hashes[0].starts_with("sha256-"));
    }

    #[test]
    fn hash_matches_known_value() {
        // SHA-256 of empty string = 47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=
        let html = "<script></script>";
        let hashes = extract_inline_script_hashes(html);
        assert_eq!(
            hashes[0],
            "sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="
        );
    }

    #[test]
    fn unclosed_script_tag_stops_parsing() {
        let html = "<script>var x = 1;";
        let hashes = extract_inline_script_hashes(html);
        assert!(hashes.is_empty());
    }
}
