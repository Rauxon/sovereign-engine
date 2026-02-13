use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit, Nonce};
use anyhow::{Context, Result};
use base64::Engine as _;
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use tracing::info;

use super::Database;

/// Derive a 256-bit AES key using HKDF-SHA256.
///
/// The salt and info strings are fixed per-application, so the same
/// `key_str` always produces the same derived key. HKDF is the correct
/// construction for extracting a uniform key from a potentially non-uniform
/// input (even though it doesn't add stretching — use a high-entropy key).
fn derive_key(key_str: &str) -> Key<Aes256Gcm> {
    let hkdf = Hkdf::<Sha256>::new(Some(b"sovereign-engine-db-encryption"), key_str.as_bytes());
    let mut okm = [0u8; 32];
    hkdf.expand(b"aes-256-gcm-key", &mut okm)
        .expect("HKDF-SHA256 expand to 32 bytes cannot fail");
    #[allow(deprecated)]
    *Key::<Aes256Gcm>::from_slice(&okm)
}

/// Legacy key derivation (bare SHA-256). Used only during migration from
/// pre-HKDF encrypted data.
fn derive_key_legacy(key_str: &str) -> Key<Aes256Gcm> {
    let hash = Sha256::digest(key_str.as_bytes());
    #[allow(deprecated)]
    *Key::<Aes256Gcm>::from_slice(&hash)
}

/// Encrypt plaintext with AES-256-GCM. Returns base64(nonce || ciphertext).
pub fn encrypt(plaintext: &str, key_str: &str) -> Result<String> {
    let key = derive_key(key_str);
    let cipher = Aes256Gcm::new(&key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;

    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(base64::engine::general_purpose::STANDARD.encode(&combined))
}

/// Decrypt base64(nonce || ciphertext) with AES-256-GCM.
pub fn decrypt(encrypted: &str, key_str: &str) -> Result<String> {
    decrypt_with_key(&derive_key(key_str), encrypted)
}

/// Decrypt with a pre-derived key.
fn decrypt_with_key(key: &Key<Aes256Gcm>, encrypted: &str) -> Result<String> {
    let cipher = Aes256Gcm::new(key);
    let combined = base64::engine::general_purpose::STANDARD
        .decode(encrypted)
        .context("invalid base64")?;

    if combined.len() < 12 {
        anyhow::bail!("ciphertext too short");
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    #[allow(deprecated)]
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("decryption failed — wrong key or corrupted data"))?;

    String::from_utf8(plaintext).context("decrypted value is not valid UTF-8")
}

/// Migrate IdP client secrets on startup.
///
/// Handles three cases for each `client_secret_enc` value:
/// 1. Already encrypted with the current (HKDF) key → no action.
/// 2. Encrypted with the legacy (SHA-256) key → decrypt and re-encrypt with HKDF.
/// 3. Plaintext → encrypt with HKDF.
pub async fn migrate_plaintext_secrets(db: &Database, key: &str) -> Result<()> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, client_secret_enc FROM idp_configs")
            .fetch_all(&db.pool)
            .await
            .context("Failed to query IdP configs for encryption migration")?;

    let current_key = derive_key(key);
    let legacy_key = derive_key_legacy(key);

    let mut migrated_legacy = 0u32;
    let mut migrated_plaintext = 0u32;

    for (id, secret_enc) in &rows {
        // Case 1: already encrypted with current HKDF key
        if decrypt_with_key(&current_key, secret_enc).is_ok() {
            continue;
        }

        // Case 2: encrypted with legacy SHA-256 key — re-encrypt with HKDF
        if let Ok(plaintext) = decrypt_with_key(&legacy_key, secret_enc) {
            let re_encrypted = encrypt(&plaintext, key)
                .context("Failed to re-encrypt client secret with HKDF key")?;
            sqlx::query("UPDATE idp_configs SET client_secret_enc = ? WHERE id = ?")
                .bind(&re_encrypted)
                .bind(id)
                .execute(&db.pool)
                .await
                .context("Failed to update re-encrypted client secret")?;
            migrated_legacy += 1;
            continue;
        }

        // Case 3: plaintext — encrypt with HKDF key
        let encrypted = encrypt(secret_enc, key).context("Failed to encrypt client secret")?;
        sqlx::query("UPDATE idp_configs SET client_secret_enc = ? WHERE id = ?")
            .bind(&encrypted)
            .bind(id)
            .execute(&db.pool)
            .await
            .context("Failed to update encrypted client secret")?;
        migrated_plaintext += 1;
    }

    if migrated_legacy > 0 {
        info!(
            count = migrated_legacy,
            "Re-encrypted IdP secrets from legacy SHA-256 to HKDF key derivation"
        );
    }
    if migrated_plaintext > 0 {
        info!(
            count = migrated_plaintext,
            "Encrypted plaintext IdP secrets"
        );
    }

    Ok(())
}
