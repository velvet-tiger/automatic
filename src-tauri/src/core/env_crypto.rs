//! Encryption at rest for MCP server environment variable values.
//!
//! # Design
//!
//! A single AES-256-GCM key is generated the first time a value is encrypted
//! and kept in the secret store (`core::secret_store`) under
//! `env_encryption_key`. In a release build that is the OS keychain (macOS
//! Keychain, Windows Credential Manager, Linux Secret Service over D-Bus), and
//! the key is never written to disk in any other form. A debug build keeps the
//! key in the dev secrets file instead.
//!
//! Decrypting never creates a key. A new key cannot read a value that an
//! earlier key encrypted, so a missing key is reported as an error.
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
use serde_json::Value;

#[cfg(not(test))]
use crate::core::secret_store;
#[cfg(not(test))]
const KEYCHAIN_USER: &str = "env_encryption_key";
const SENTINEL: &str = "enc:v1:";

#[cfg(test)]
static TEST_KEY: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();

#[cfg(not(test))]
static CACHED_KEY: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();

// ── Key management ────────────────────────────────────────────────────────────

/// Retrieve the encryption key, creating and storing a new random key if one
/// does not yet exist.
#[cfg(test)]
fn get_or_create_key() -> Result<[u8; 32], String> {
    Ok(*TEST_KEY.get_or_init(|| Aes256Gcm::generate_key(OsRng).into()))
}

/// Retrieve the encryption key for a decrypt. Tests share one key, so a value
/// a test encrypted can be decrypted again.
#[cfg(test)]
fn existing_key() -> Result<[u8; 32], String> {
    get_or_create_key()
}

/// Retrieve the encryption key from the keychain, creating and storing a new
/// random key if one does not yet exist.
///
/// The key is read from the keychain at most once per process. Callers encrypt
/// and decrypt one value at a time, so without this the keychain is hit once
/// per environment variable, and on macOS each of those reads is a separate
/// access check that can raise its own password dialog.
#[cfg(not(test))]
fn get_or_create_key() -> Result<[u8; 32], String> {
    if let Some(key) = CACHED_KEY.get() {
        return Ok(*key);
    }
    let key = read_or_create_keychain_key()?;
    Ok(*CACHED_KEY.get_or_init(|| key))
}

/// Retrieve the encryption key from the keychain for a decrypt. Never creates
/// a key.
///
/// A missing key is not cached. Another process may create the key later, and
/// the next call must find it.
#[cfg(not(test))]
fn existing_key() -> Result<[u8; 32], String> {
    if let Some(key) = CACHED_KEY.get() {
        return Ok(*key);
    }
    let key = key_for_decrypt(secret_store::load_secret_if_present(KEYCHAIN_USER))?;
    Ok(*CACHED_KEY.get_or_init(|| key))
}

#[cfg(not(test))]
fn read_or_create_keychain_key() -> Result<[u8; 32], String> {
    existing_or_new_key(secret_store::load_secret_if_present(KEYCHAIN_USER), |hex| {
        secret_store::store_secret(KEYCHAIN_USER, hex)
    })
}

/// Return the stored key. When `loaded` shows that nothing is stored, create
/// a key and pass it to `store` as hex. `store` is called in no other case.
///
/// The read result and the write are parameters so that the rule "a failed
/// read never creates a key" can be tested without a keychain.
fn existing_or_new_key(
    loaded: Result<Option<String>, String>,
    store: impl FnOnce(&str) -> Result<(), String>,
) -> Result<[u8; 32], String> {
    if let Some(key) = stored_key(loaded)? {
        return Ok(key);
    }

    // No key yet — generate one and persist it.
    let key_bytes: [u8; 32] = Aes256Gcm::generate_key(OsRng).into();
    store(&hex::encode(key_bytes))
        .map_err(|e| format!("Failed to store encryption key in keychain: {}", e))?;
    Ok(key_bytes)
}

/// Return the stored key for a decrypt. No stored key is an error here.
///
/// The value being decrypted was encrypted with a key that is no longer in
/// the store. Before this rule, the first read after the key was lost created
/// a new key, and the old values then failed with a bare "Decryption failed".
fn key_for_decrypt(loaded: Result<Option<String>, String>) -> Result<[u8; 32], String> {
    stored_key(loaded)?.ok_or_else(|| {
        "The encryption key for this value is no longer in the keychain, so the value cannot \
         be decrypted (no new key was created). Enter the value again to store it with a new key."
            .to_string()
    })
}

/// Decode the result of reading the stored key. `Ok(None)` means no key has
/// been created yet, and is the only result that may lead to a new key.
///
/// A failed read is an error, never "no key". The key may still be in the
/// store, and replacing it would leave every value encrypted with it
/// unreadable. A stored key that cannot be decoded is an error for the same
/// reason.
fn stored_key(loaded: Result<Option<String>, String>) -> Result<Option<[u8; 32]>, String> {
    let stored = loaded.map_err(|e| {
        format!(
            "Failed to read encryption key from keychain (no new key was created): {}",
            e
        )
    })?;
    let Some(hex) = stored else {
        return Ok(None);
    };

    let bytes = hex::decode(&hex)
        .map_err(|e| format!("Failed to decode encryption key from keychain: {}", e))?;
    let key = bytes
        .try_into()
        .map_err(|_| "Encryption key in keychain has wrong length".to_string())?;
    Ok(Some(key))
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
/// sentinel prefix is missing, no key is stored, or decryption fails.
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

    let key_bytes = existing_key()?;
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

    // ── stored_key ───────────────────────────────────────────────────────────

    #[test]
    fn stored_key_decodes_an_existing_key() {
        let key = [7u8; 32];
        let loaded = Ok(Some(hex::encode(key)));
        assert_eq!(stored_key(loaded), Ok(Some(key)));
    }

    #[test]
    fn stored_key_reports_no_key_only_when_nothing_is_stored() {
        assert_eq!(stored_key(Ok(None)), Ok(None));
    }

    #[test]
    fn stored_key_keeps_a_failed_read_as_an_error() {
        // A denied or locked keychain must never look like "no key yet":
        // that would replace the real key and orphan every encrypted value.
        let error = stored_key(Err("User interaction is not allowed".to_string())).unwrap_err();
        assert!(error.contains("User interaction is not allowed"));
        assert!(error.contains("no new key was created"));
    }

    #[test]
    fn stored_key_rejects_a_key_it_cannot_decode() {
        assert!(stored_key(Ok(Some("not hex".to_string()))).is_err());
        assert!(stored_key(Ok(Some(String::new()))).is_err());
        assert!(stored_key(Ok(Some(hex::encode([7u8; 16])))).is_err());
    }

    // ── key_for_decrypt ──────────────────────────────────────────────────────

    #[test]
    fn a_decrypt_uses_the_stored_key() {
        let key = [5u8; 32];
        assert_eq!(key_for_decrypt(Ok(Some(hex::encode(key)))), Ok(key));
    }

    #[test]
    fn a_decrypt_with_no_stored_key_is_an_error() {
        // A new key could not read the value, and would hide that the old
        // key was lost.
        let error = key_for_decrypt(Ok(None)).unwrap_err();
        assert!(
            error.contains("no new key was created"),
            "error was: {error}"
        );
    }

    #[test]
    fn a_decrypt_keeps_a_failed_read_as_an_error() {
        let error = key_for_decrypt(Err("keychain is locked".to_string())).unwrap_err();
        assert!(error.contains("keychain is locked"), "error was: {error}");
    }

    // ── existing_or_new_key ──────────────────────────────────────────────────

    /// Run `existing_or_new_key` and return every value it passed to `store`.
    fn key_and_stored_values(
        loaded: Result<Option<String>, String>,
    ) -> (Result<[u8; 32], String>, Vec<String>) {
        let mut stored = Vec::new();
        let key = existing_or_new_key(loaded, |hex| {
            stored.push(hex.to_string());
            Ok(())
        });
        (key, stored)
    }

    #[test]
    fn a_failed_read_never_stores_a_new_key() {
        let (key, stored) = key_and_stored_values(Err("keychain is locked".to_string()));
        assert!(key.is_err());
        assert!(stored.is_empty(), "a failed read must not replace the key");
    }

    #[test]
    fn an_undecodable_key_is_never_replaced() {
        let (key, stored) = key_and_stored_values(Ok(Some("not hex".to_string())));
        assert!(key.is_err());
        assert!(stored.is_empty());
    }

    #[test]
    fn an_existing_key_is_returned_without_a_write() {
        let existing = [9u8; 32];
        let (key, stored) = key_and_stored_values(Ok(Some(hex::encode(existing))));
        assert_eq!(key, Ok(existing));
        assert!(stored.is_empty());
    }

    #[test]
    fn a_new_key_is_stored_once_when_nothing_is_stored() {
        let (key, stored) = key_and_stored_values(Ok(None));
        let key = key.expect("a new key");
        assert_eq!(stored, vec![hex::encode(key)]);
    }

    #[test]
    fn a_new_key_that_cannot_be_stored_is_an_error() {
        let key = existing_or_new_key(Ok(None), |_| Err("disk full".to_string()));
        assert!(key.unwrap_err().contains("disk full"));
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
