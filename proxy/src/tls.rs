use std::net::SocketAddr;

use anyhow::{Context, Result};
use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use rustls_acme::caches::DirCache;
use rustls_acme::AcmeConfig;
use tokio_stream::StreamExt;
use tracing::{error, info};

use crate::config::AppConfig;

/// Start the HTTPS server with TLS termination via rustls.
pub async fn serve_tls(app: Router, addr: SocketAddr, config: &AppConfig) -> Result<()> {
    let (cert_path, key_path) = config.tls_paths()?;

    let tls_config = RustlsConfig::from_pem_file(cert_path, key_path)
        .await
        .context("Failed to load TLS certificates")?;

    axum_server::bind_rustls(addr, tls_config)
        .serve(app.into_make_service())
        .await
        .context("HTTPS server error")?;

    Ok(())
}

/// Start the HTTPS server with automatic cert provisioning via Let's Encrypt (TLS-ALPN-01).
pub async fn serve_acme(
    app: Router,
    addr: SocketAddr,
    domain: &str,
    contact: &str,
    staging: bool,
) -> Result<()> {
    let mut state = AcmeConfig::new([domain])
        .contact([format!("mailto:{contact}")])
        .cache(DirCache::new("/config/acme"))
        .directory_lets_encrypt(!staging)
        .state();

    let acceptor = state.axum_acceptor(state.default_rustls_config());

    // Drive the ACME state machine — handles cert acquisition and renewal
    tokio::spawn(async move {
        loop {
            match state.next().await {
                Some(Ok(ok)) => info!("ACME event: {:?}", ok),
                Some(Err(err)) => error!("ACME error: {:?}", err),
                None => break,
            }
        }
    });

    axum_server::bind(addr)
        .acceptor(acceptor)
        .serve(app.into_make_service())
        .await
        .context("ACME HTTPS server error")?;

    Ok(())
}
