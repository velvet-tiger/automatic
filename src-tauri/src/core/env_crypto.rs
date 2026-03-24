//! Encryption at rest for MCP server environment variable values.
//!
//! # Design
//!
//! A single AES-256-GCM key is generated on first use and stored in the OS
//! keychain (macOS Keychain, Linux SecretService, Windows Credential Manager)
//! under `automatic_desktop` / `env_encryption_key`.  It is never written to
//! disk in any other form.
//!
//! Encrypted values are stored as self-describing sentinel strings:
//!
//! ```text
//! enc:v1:<base64url(12-byte nonce ‖ ciphertext ‖ 16-byte GCM tag)>
//! ```
//!
//! Any value that does not begin with `enc:v1:` is treated as plaintext
//! (backward-compatible with configs written before this feature).
//!
//! # Scope
//!
//! Only the `env` field of stdio-type MCP server configs is encrypted.
//! HTTP header values and all other config fields are left as-is.

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
#[cfg(not(test))]
use keyring::Entry;
use serde_json::Value;

#[cfg(not(test))]
use crate::core::KEYCHAIN_SERVICE;
#[cfg(not(test))]
const KEYCHAIN_USER: &str = "env_encryption_key";
const SENTINEL: &str = "enc:v1:";

#[cfg(test)]
static TEST_KEY: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();

// ── Key management ────────────────────────────────────────────────────────────

/// Retrieve the encryption key from the keychain, creating and storing a new
/// random key if one does not yet exist.
fn get_or_create_key() -> Result<[u8; 32], String> {
    #[cfg(test)]
    {
        return Ok(*TEST_KEY.get_or_init(|| Aes256Gcm::generate_key(OsRng).into()));
    }

    #[cfg(not(test))]
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_USER).map_err(|e| e.to_string())?;

    #[cfg(not(test))]
    match entry.get_password() {
        Ok(hex) => {
            // Key already exists — decode it.
            let bytes = hex::decode(&hex)
                .map_err(|e| format!("Failed to decode encryption key from keychain: {}", e))?;
            bytes
                .try_into()
                .map_err(|_| "Encryption key in keychain has wrong length".into())
        }
        Err(_) => {
            // No key yet — generate one and persist it.
            let key_bytes: [u8; 32] = Aes256Gcm::generate_key(OsRng).into();
            let hex = hex::encode(key_bytes);
            entry
                .set_password(&hex)
                .map_err(|e| format!("Failed to store encryption key in keychain: {}", e))?;
            Ok(key_bytes)
        }
    }
}

// ── Encrypt / Decrypt primitives ──────────────────────────────────────────────

/// Encrypt `plaintext` and return an `enc:v1:<base64url>` sentinel string.
pub fn encrypt_value(plaintext: &str) -> Result<String, String> {
    let key_bytes = get_or_create_key()?;
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // 12 bytes
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {}", e))?;

    // Encode as nonce ‖ ciphertext (which already contains the GCM tag).
    let mut payload = nonce.to_vec();
    payload.extend_from_slice(&ciphertext);
    Ok(format!("{}{}", SENTINEL, URL_SAFE_NO_PAD.encode(payload)))
}

/// Decrypt an `enc:v1:<base64url>` sentinel string.  Returns `Err` if the
/// sentinel prefix is missing or decryption fails.
pub fn decrypt_value(ciphertext_str: &str) -> Result<String, String> {
    let encoded = ciphertext_str
        .strip_prefix(SENTINEL)
        .ok_or("Not an encrypted value")?;

    let payload = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|e| format!("Failed to base64-decode encrypted value: {}", e))?;

    if payload.len() < 12 {
        return Err("Encrypted payload too short".into());
    }

    let (nonce_bytes, ciphertext) = payload.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key_bytes = get_or_create_key()?;
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failed: {}", e))?;

    String::from_utf8(plaintext).map_err(|e| format!("Decrypted value is not valid UTF-8: {}", e))
}

// ── JSON helpers ──────────────────────────────────────────────────────────────

/// Return `true` if the string looks like an encrypted sentinel.
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(SENTINEL)
}

/// Encrypt all string values in a JSON `env` object in place.
///
/// Values that are already encrypted (sentinel prefix) are left untouched so
/// that a double-save does not double-encrypt.  Non-string values are ignored.
pub fn encrypt_env_values(env: &mut Value) -> Result<(), String> {
    if let Value::Object(map) = env {
        for (_, v) in map.iter_mut() {
            if let Value::String(s) = v {
                if !is_encrypted(s) {
                    *s = encrypt_value(s)?;
                }
            }
        }
    }
    Ok(())
}

/// Decrypt all string values in a JSON `env` object in place.
///
/// Values without the sentinel prefix are left untouched (plain-text
/// backward-compatibility).  Non-string values are ignored.
pub fn decrypt_env_values(env: &mut Value) -> Result<(), String> {
    if let Value::Object(map) = env {
        for (_, v) in map.iter_mut() {
            if let Value::String(s) = v {
                if is_encrypted(s) {
                    *s = decrypt_value(s)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── is_encrypted ─────────────────────────────────────────────────────────

    #[test]
    fn is_encrypted_recognises_sentinel_prefix() {
        assert!(is_encrypted("enc:v1:somepayload"));
    }

    #[test]
    fn is_encrypted_returns_false_for_plaintext() {
        assert!(!is_encrypted("my-api-key"));
        assert!(!is_encrypted(""));
        assert!(!is_encrypted("enc:v2:something")); // wrong version
    }

    // ── encrypt_value / decrypt_value roundtrip ───────────────────────────────

    #[test]
    fn encrypt_produces_sentinel_prefixed_string() {
        let encrypted = encrypt_value("secret").expect("encrypt");
        assert!(encrypted.starts_with("enc:v1:"));
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let plaintext = "my-secret-api-key-12345";
        let encrypted = encrypt_value(plaintext).expect("encrypt");
        let decrypted = decrypt_value(&encrypted).expect("decrypt");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_decrypt_empty_string() {
        let encrypted = encrypt_value("").expect("encrypt empty");
        let decrypted = decrypt_value(&encrypted).expect("decrypt empty");
        assert_eq!(decrypted, "");
    }

    #[test]
    fn encrypt_decrypt_unicode_string() {
        let plaintext = "日本語テスト 🔑 secret";
        let encrypted = encrypt_value(plaintext).expect("encrypt unicode");
        let decrypted = decrypt_value(&encrypted).expect("decrypt unicode");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn two_encryptions_of_same_plaintext_produce_different_ciphertexts() {
        // Nonces are random — ciphertexts must differ even for the same input.
        let a = encrypt_value("same").expect("encrypt a");
        let b = encrypt_value("same").expect("encrypt b");
        assert_ne!(a, b, "identical nonces would be a security flaw");
    }

    #[test]
    fn decrypt_rejects_non_sentinel_string() {
        let result = decrypt_value("plaintext");
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_rejects_truncated_payload() {
        // "enc:v1:" followed by base64 of fewer than 12 bytes (the nonce minimum).
        let short = format!(
            "enc:v1:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"short")
        );
        let result = decrypt_value(&short);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_rejects_corrupted_ciphertext() {
        let encrypted = encrypt_value("original").expect("encrypt");
        // Flip the last character to corrupt the GCM tag.
        let mut corrupted = encrypted.clone();
        let last = corrupted.pop().unwrap();
        corrupted.push(if last == 'A' { 'B' } else { 'A' });
        let result = decrypt_value(&corrupted);
        assert!(result.is_err());
    }

    // ── encrypt_env_values / decrypt_env_values (JSON object helpers) ─────────

    #[test]
    fn encrypt_env_values_encrypts_all_strings() {
        let mut env = json!({ "KEY_ONE": "secret1", "KEY_TWO": "secret2" });
        encrypt_env_values(&mut env).expect("encrypt env");

        let obj = env.as_object().unwrap();
        assert!(obj["KEY_ONE"].as_str().unwrap().starts_with("enc:v1:"));
        assert!(obj["KEY_TWO"].as_str().unwrap().starts_with("enc:v1:"));
    }

    #[test]
    fn encrypt_env_values_is_idempotent() {
        let mut env = json!({ "KEY": "value" });
        encrypt_env_values(&mut env).expect("first encrypt");
        let after_first = env["KEY"].as_str().unwrap().to_string();

        encrypt_env_values(&mut env).expect("second encrypt");
        let after_second = env["KEY"].as_str().unwrap().to_string();

        // Already-encrypted values must not be double-encrypted.
        assert_eq!(after_first, after_second);
    }

    #[test]
    fn decrypt_env_values_restores_plaintext() {
        let mut env = json!({ "KEY": "my-api-key" });
        encrypt_env_values(&mut env).expect("encrypt");
        decrypt_env_values(&mut env).expect("decrypt");
        assert_eq!(env["KEY"].as_str().unwrap(), "my-api-key");
    }

    #[test]
    fn decrypt_env_values_leaves_plaintext_values_untouched() {
        // Backward-compat: values without the sentinel are left as-is.
        let mut env = json!({ "KEY": "already-plaintext" });
        decrypt_env_values(&mut env).expect("decrypt");
        assert_eq!(env["KEY"].as_str().unwrap(), "already-plaintext");
    }

    #[test]
    fn encrypt_env_values_ignores_non_string_values() {
        let mut env = json!({ "NUM": 42, "FLAG": true, "NULL": null });
        encrypt_env_values(&mut env).expect("encrypt non-strings");
        // Values should be unchanged.
        assert_eq!(env["NUM"], json!(42));
        assert_eq!(env["FLAG"], json!(true));
        assert_eq!(env["NULL"], json!(null));
    }

    #[test]
    fn encrypt_env_values_on_non_object_is_noop() {
        let mut env = json!("not an object");
        encrypt_env_values(&mut env).expect("no error");
        assert_eq!(env, json!("not an object"));
    }

    #[test]
    fn full_roundtrip_via_env_helpers() {
        let mut env = json!({
            "ANTHROPIC_API_KEY": "sk-ant-test-1234",
            "OPENAI_KEY": "sk-openai-5678"
        });
        encrypt_env_values(&mut env).expect("encrypt");
        decrypt_env_values(&mut env).expect("decrypt");

        assert_eq!(
            env["ANTHROPIC_API_KEY"].as_str().unwrap(),
            "sk-ant-test-1234"
        );
        assert_eq!(env["OPENAI_KEY"].as_str().unwrap(), "sk-openai-5678");
    }
}
