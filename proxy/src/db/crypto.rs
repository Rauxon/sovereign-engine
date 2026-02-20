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
/// Handles five cases for each `client_secret_enc` value (tried in order):
/// 1. Already encrypted with the current HKDF key → no action.
/// 2. Encrypted with old HKDF key (key rotation via `DB_ENCRYPTION_KEY_OLD`) → re-encrypt.
/// 3. Encrypted with legacy SHA-256 of current key → re-encrypt.
/// 4. Encrypted with legacy SHA-256 of old key → re-encrypt.
/// 5. Encrypted with HKDF("") (empty-key bug recovery) → re-encrypt.
/// 6. Plaintext → encrypt.
pub async fn migrate_plaintext_secrets(
    db: &Database,
    key: &str,
    old_key: Option<&str>,
) -> Result<()> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, client_secret_enc FROM idp_configs")
            .fetch_all(&db.pool)
            .await
            .context("Failed to query IdP configs for encryption migration")?;

    let current_key = derive_key(key);

    let mut migrated_old_key = 0u32;
    let mut migrated_legacy = 0u32;
    let mut migrated_empty_key = 0u32;
    let mut migrated_plaintext = 0u32;

    for (id, secret_enc) in &rows {
        // Case 1: already encrypted with current HKDF key
        if decrypt_with_key(&current_key, secret_enc).is_ok() {
            continue;
        }

        // Case 2: encrypted with old HKDF key (key rotation)
        if let Some(old) = old_key {
            let old_hkdf = derive_key(old);
            if let Ok(plaintext) = decrypt_with_key(&old_hkdf, secret_enc) {
                re_encrypt_row(db, id, &plaintext, key).await?;
                migrated_old_key += 1;
                continue;
            }
        }

        // Case 3: encrypted with legacy SHA-256 of current key
        let legacy_current = derive_key_legacy(key);
        if let Ok(plaintext) = decrypt_with_key(&legacy_current, secret_enc) {
            re_encrypt_row(db, id, &plaintext, key).await?;
            migrated_legacy += 1;
            continue;
        }

        // Case 4: encrypted with legacy SHA-256 of old key
        if let Some(old) = old_key {
            let legacy_old = derive_key_legacy(old);
            if let Ok(plaintext) = decrypt_with_key(&legacy_old, secret_enc) {
                re_encrypt_row(db, id, &plaintext, key).await?;
                migrated_legacy += 1;
                continue;
            }
        }

        // Case 5: encrypted with HKDF("") — empty-key bug recovery.
        // A previous version of the docker-compose defaulted DB_ENCRYPTION_KEY to ""
        // which silently encrypted secrets with a key derived from empty string.
        if !key.is_empty() {
            let empty_hkdf = derive_key("");
            if let Ok(plaintext) = decrypt_with_key(&empty_hkdf, secret_enc) {
                re_encrypt_row(db, id, &plaintext, key).await?;
                migrated_empty_key += 1;
                continue;
            }
        }

        // Case 6: plaintext — encrypt with current HKDF key
        let encrypted = encrypt(secret_enc, key).context("Failed to encrypt client secret")?;
        sqlx::query("UPDATE idp_configs SET client_secret_enc = ? WHERE id = ?")
            .bind(&encrypted)
            .bind(id)
            .execute(&db.pool)
            .await
            .context("Failed to update encrypted client secret")?;
        migrated_plaintext += 1;
    }

    if migrated_old_key > 0 {
        info!(
            count = migrated_old_key,
            "Re-encrypted IdP secrets from old key to new key"
        );
    }
    if migrated_legacy > 0 {
        info!(
            count = migrated_legacy,
            "Re-encrypted IdP secrets from legacy SHA-256 key derivation"
        );
    }
    if migrated_empty_key > 0 {
        info!(
            count = migrated_empty_key,
            "Re-encrypted IdP secrets from empty-key bug (HKDF(\"\"))"
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

/// Helper: decrypt a secret and re-encrypt with the current key.
async fn re_encrypt_row(db: &Database, id: &str, plaintext: &str, key: &str) -> Result<()> {
    let re_encrypted = encrypt(plaintext, key).context("Failed to re-encrypt client secret")?;
    sqlx::query("UPDATE idp_configs SET client_secret_enc = ? WHERE id = ?")
        .bind(&re_encrypted)
        .bind(id)
        .execute(&db.pool)
        .await
        .context("Failed to update re-encrypted client secret")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &str = "test-encryption-key-with-enough-entropy";
    const NEW_KEY: &str = "new-encryption-key-for-rotation-test";

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let plaintext = "my-secret-client-secret";
        let encrypted = encrypt(plaintext, TEST_KEY).unwrap();
        let decrypted = decrypt(&encrypted, TEST_KEY).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let encrypted = encrypt("secret", TEST_KEY).unwrap();
        let result = decrypt(&encrypted, "wrong-key");
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_with_invalid_base64_fails() {
        let result = decrypt("not-valid-base64!!!", TEST_KEY);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_with_truncated_ciphertext_fails() {
        let encrypted = encrypt("secret", TEST_KEY).unwrap();
        // Decode, truncate to less than 12 bytes (nonce size), re-encode
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&encrypted)
            .unwrap();
        let truncated = base64::engine::general_purpose::STANDARD.encode(&bytes[..8]);
        let result = decrypt(&truncated, TEST_KEY);
        assert!(result.is_err());
    }

    #[test]
    fn same_plaintext_same_key_produces_different_ciphertexts() {
        let a = encrypt("same-input", TEST_KEY).unwrap();
        let b = encrypt("same-input", TEST_KEY).unwrap();
        assert_ne!(a, b, "random nonce should produce different ciphertexts");
    }

    #[test]
    fn derive_key_is_deterministic() {
        let k1 = derive_key("my-key");
        let k2 = derive_key("my-key");
        assert_eq!(k1, k2);
    }

    // --- Migration tests ---

    /// Insert a test IdP row with the given client_secret_enc value.
    async fn insert_test_idp(db: &Database, id: &str, secret_enc: &str) {
        sqlx::query(
            "INSERT INTO idp_configs (id, name, issuer, client_id, client_secret_enc, scopes, enabled)
             VALUES (?, 'Test', 'https://issuer', 'client-id', ?, 'openid', 1)",
        )
        .bind(id)
        .bind(secret_enc)
        .execute(&db.pool)
        .await
        .unwrap();
    }

    /// Read back the raw client_secret_enc from the DB.
    async fn read_secret_enc(db: &Database, id: &str) -> String {
        let (enc,): (String,) =
            sqlx::query_as("SELECT client_secret_enc FROM idp_configs WHERE id = ?")
                .bind(id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        enc
    }

    #[tokio::test]
    async fn migrate_plaintext_encrypts_and_roundtrips() {
        let db = Database::test_db().await;
        let secret = "my-azure-client-secret-value";
        insert_test_idp(&db, "idp1", secret).await;

        migrate_plaintext_secrets(&db, TEST_KEY, None)
            .await
            .unwrap();

        let enc = read_secret_enc(&db, "idp1").await;
        assert_ne!(enc, secret, "should be encrypted, not plaintext");
        let decrypted = decrypt(&enc, TEST_KEY).unwrap();
        assert_eq!(decrypted, secret);
    }

    #[tokio::test]
    async fn migrate_already_encrypted_is_noop() {
        let db = Database::test_db().await;
        let secret = "my-secret";
        let encrypted = encrypt(secret, TEST_KEY).unwrap();
        insert_test_idp(&db, "idp1", &encrypted).await;

        migrate_plaintext_secrets(&db, TEST_KEY, None)
            .await
            .unwrap();

        let enc = read_secret_enc(&db, "idp1").await;
        // Should be unchanged (same ciphertext, not re-encrypted)
        assert_eq!(enc, encrypted);
    }

    #[tokio::test]
    async fn migrate_key_rotation() {
        let db = Database::test_db().await;
        let secret = "rotatable-secret";
        let encrypted_old = encrypt(secret, TEST_KEY).unwrap();
        insert_test_idp(&db, "idp1", &encrypted_old).await;

        // Rotate: new key = NEW_KEY, old key = TEST_KEY
        migrate_plaintext_secrets(&db, NEW_KEY, Some(TEST_KEY))
            .await
            .unwrap();

        let enc = read_secret_enc(&db, "idp1").await;
        assert_ne!(enc, encrypted_old, "should be re-encrypted with new key");
        let decrypted = decrypt(&enc, NEW_KEY).unwrap();
        assert_eq!(decrypted, secret);
    }

    #[tokio::test]
    async fn migrate_empty_key_bug_recovery() {
        let db = Database::test_db().await;
        let secret = "secret-encrypted-with-empty-key";
        // Simulate the empty-key bug: encrypted with HKDF("")
        let encrypted_empty = encrypt(secret, "").unwrap();
        insert_test_idp(&db, "idp1", &encrypted_empty).await;

        // Now migrate with a real key — should detect and re-encrypt
        migrate_plaintext_secrets(&db, TEST_KEY, None)
            .await
            .unwrap();

        let enc = read_secret_enc(&db, "idp1").await;
        assert_ne!(enc, encrypted_empty, "should be re-encrypted");
        let decrypted = decrypt(&enc, TEST_KEY).unwrap();
        assert_eq!(decrypted, secret);
    }

    #[tokio::test]
    async fn migrate_legacy_sha256_key() {
        let db = Database::test_db().await;
        let secret = "legacy-encrypted-secret";
        // Encrypt with legacy SHA-256 key derivation
        let legacy_key = derive_key_legacy(TEST_KEY);
        let cipher = Aes256Gcm::new(&legacy_key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher.encrypt(&nonce, secret.as_bytes()).unwrap();
        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&ciphertext);
        let encrypted_legacy = base64::engine::general_purpose::STANDARD.encode(&combined);
        insert_test_idp(&db, "idp1", &encrypted_legacy).await;

        migrate_plaintext_secrets(&db, TEST_KEY, None)
            .await
            .unwrap();

        let enc = read_secret_enc(&db, "idp1").await;
        let decrypted = decrypt(&enc, TEST_KEY).unwrap();
        assert_eq!(decrypted, secret);
    }

    #[tokio::test]
    async fn migrate_idempotent() {
        let db = Database::test_db().await;
        let secret = "idempotent-secret";
        insert_test_idp(&db, "idp1", secret).await;

        // Run migration twice
        migrate_plaintext_secrets(&db, TEST_KEY, None)
            .await
            .unwrap();
        migrate_plaintext_secrets(&db, TEST_KEY, None)
            .await
            .unwrap();

        let enc = read_secret_enc(&db, "idp1").await;
        let decrypted = decrypt(&enc, TEST_KEY).unwrap();
        assert_eq!(decrypted, secret);
    }

    #[tokio::test]
    async fn migrate_multiple_rows_mixed_states() {
        let db = Database::test_db().await;

        // Row 1: plaintext
        insert_test_idp(&db, "idp1", "plaintext-secret").await;

        // Row 2: already encrypted with current key
        let enc2 = encrypt("already-encrypted", TEST_KEY).unwrap();
        insert_test_idp(&db, "idp2", &enc2).await;

        // Row 3: encrypted with empty key (bug)
        let enc3 = encrypt("empty-key-secret", "").unwrap();
        insert_test_idp(&db, "idp3", &enc3).await;

        migrate_plaintext_secrets(&db, TEST_KEY, None)
            .await
            .unwrap();

        assert_eq!(
            decrypt(&read_secret_enc(&db, "idp1").await, TEST_KEY).unwrap(),
            "plaintext-secret"
        );
        assert_eq!(
            decrypt(&read_secret_enc(&db, "idp2").await, TEST_KEY).unwrap(),
            "already-encrypted"
        );
        assert_eq!(
            decrypt(&read_secret_enc(&db, "idp3").await, TEST_KEY).unwrap(),
            "empty-key-secret"
        );
    }
}
