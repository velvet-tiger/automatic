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
use keyring::Entry;
use serde_json::Value;

const KEYCHAIN_SERVICE: &str = "automatic_desktop";
const KEYCHAIN_USER: &str = "env_encryption_key";
const SENTINEL: &str = "enc:v1:";

// ── Key management ────────────────────────────────────────────────────────────

/// Retrieve the encryption key from the keychain, creating and storing a new
/// random key if one does not yet exist.
fn get_or_create_key() -> Result<[u8; 32], String> {
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_USER).map_err(|e| e.to_string())?;

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
