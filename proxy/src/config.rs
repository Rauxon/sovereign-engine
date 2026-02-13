use anyhow::{Context, Result};
use subtle::ConstantTimeEq;

#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Bind address (e.g. "0.0.0.0:443")
    pub listen_addr: String,

    /// SQLite database URL
    pub database_url: String,

    /// Path to TLS certificate PEM file
    pub tls_cert_path: Option<String>,

    /// Path to TLS private key PEM file
    pub tls_key_path: Option<String>,

    /// Bootstrap admin username (first-run only)
    pub bootstrap_user: Option<String>,

    /// Bootstrap admin password (first-run only)
    pub bootstrap_password: Option<String>,

    /// Re-enable bootstrap creds alongside OIDC
    pub break_glass: bool,

    /// Docker socket path
    pub docker_host: String,

    /// Path to models directory (local to this process, e.g. /models when Dockerized)
    pub model_path: String,

    /// Host-side models path for child container bind mounts.
    /// Defaults to MODEL_PATH (correct when proxy runs directly on host).
    pub model_host_path: String,

    /// Static UI files path
    pub ui_path: String,

    /// External URL for OIDC callbacks (e.g. "https://ai.example.com")
    pub external_url: String,

    /// Docker network name for backend containers (internal, isolated)
    pub backend_network: String,

    /// ACME domain — enables automatic Let's Encrypt cert provisioning when set
    pub acme_domain: Option<String>,

    /// ACME contact email — required when ACME_DOMAIN is set
    pub acme_contact: Option<String>,

    /// Use Let's Encrypt staging environment (default false)
    pub acme_staging: bool,

    /// Open WebUI backend URL (internal, no external access)
    pub webui_backend_url: String,

    /// Pre-shared API key for Open WebUI → proxy /v1 calls (env: WEBUI_API_KEY)
    pub webui_api_key: Option<String>,

    /// Max seconds to hold a queued request before returning 429 (env: QUEUE_TIMEOUT_SECS)
    pub queue_timeout_secs: u64,

    /// Set Secure flag on session cookies (env: SECURE_COOKIES, default: true).
    /// Set to false for HTTP-only dev instances.
    pub secure_cookies: bool,

    /// Encryption key for IdP client secrets at rest (env: DB_ENCRYPTION_KEY).
    /// When set, client_secret_enc is AES-256-GCM encrypted. When absent, stored plaintext.
    pub db_encryption_key: Option<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            listen_addr: std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:443".into()),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:///config/sovereign.db".into()),
            tls_cert_path: std::env::var("TLS_CERT_PATH").ok(),
            tls_key_path: std::env::var("TLS_KEY_PATH").ok(),
            bootstrap_user: std::env::var("BOOTSTRAP_USER").ok(),
            bootstrap_password: std::env::var("BOOTSTRAP_PASSWORD").ok(),
            break_glass: std::env::var("BREAK_GLASS")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            docker_host: std::env::var("DOCKER_HOST")
                .unwrap_or_else(|_| "unix:///var/run/docker.sock".into()),
            model_path: std::env::var("MODEL_PATH").unwrap_or_else(|_| "/models".into()),
            model_host_path: std::env::var("MODEL_HOST_PATH").unwrap_or_else(|_| {
                std::env::var("MODEL_PATH").unwrap_or_else(|_| "/models".into())
            }),
            ui_path: std::env::var("UI_PATH").unwrap_or_else(|_| "/app/ui".into()),
            external_url: std::env::var("EXTERNAL_URL")
                .unwrap_or_else(|_| "http://localhost:3000".into()),
            backend_network: std::env::var("BACKEND_NETWORK")
                .unwrap_or_else(|_| "sovereign-internal".into()),
            acme_domain: std::env::var("ACME_DOMAIN").ok(),
            acme_contact: std::env::var("ACME_CONTACT").ok(),
            acme_staging: std::env::var("ACME_STAGING")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            webui_backend_url: std::env::var("WEBUI_BACKEND_URL")
                .unwrap_or_else(|_| "http://open-webui:8080".into()),
            webui_api_key: std::env::var("WEBUI_API_KEY").ok(),
            queue_timeout_secs: std::env::var("QUEUE_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            secure_cookies: std::env::var("SECURE_COOKIES")
                .map(|v| !v.eq_ignore_ascii_case("false"))
                .unwrap_or(true),
            db_encryption_key: std::env::var("DB_ENCRYPTION_KEY").ok(),
        })
    }

    /// Check if bootstrap credentials are configured.
    pub fn has_bootstrap_creds(&self) -> bool {
        self.bootstrap_user.is_some() && self.bootstrap_password.is_some()
    }

    /// Validate bootstrap credentials against the provided values.
    /// Uses constant-time comparison to prevent timing side-channel attacks.
    pub fn validate_bootstrap_creds(&self, user: &str, password: &str) -> bool {
        match (&self.bootstrap_user, &self.bootstrap_password) {
            (Some(u), Some(p)) => {
                let user_match = u.as_bytes().ct_eq(user.as_bytes());
                let pass_match = p.as_bytes().ct_eq(password.as_bytes());
                (user_match & pass_match).into()
            }
            _ => false,
        }
    }

    /// Load TLS certificate and key paths, returning an error if either is missing.
    pub fn tls_paths(&self) -> Result<(&str, &str)> {
        let cert = self
            .tls_cert_path
            .as_deref()
            .context("TLS_CERT_PATH not set")?;
        let key = self
            .tls_key_path
            .as_deref()
            .context("TLS_KEY_PATH not set")?;
        Ok((cert, key))
    }

    /// Return ACME config tuple if ACME_DOMAIN is set.
    /// Fails fast if domain is set without a contact email.
    pub fn acme_config(&self) -> Result<Option<(&str, &str, bool)>> {
        match (&self.acme_domain, &self.acme_contact) {
            (Some(domain), Some(contact)) => Ok(Some((domain, contact, self.acme_staging))),
            (Some(_), None) => anyhow::bail!("ACME_DOMAIN is set but ACME_CONTACT is missing"),
            _ => Ok(None),
        }
    }
}
