use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::agents::known_agents;
use super::secret_store;
use super::settings::{read_settings, write_settings};

// ── API Keys ─────────────────────────────────────────────────────────────────

/// Remembers, per provider, whether a key is present in the keychain.
///
/// Existence can only be established by reading the secret, and callers ask
/// repeatedly: every provider is probed whenever the agent-features toggle is
/// evaluated. On macOS each read is a separate access check that can raise its
/// own password dialog, so the answer is kept for the life of the process and
/// invalidated whenever this process writes or deletes a key.
fn presence_cache() -> &'static Mutex<HashMap<String, bool>> {
    static CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn set_cached_presence(provider: &str, present: bool) {
    if let Ok(mut cache) = presence_cache().lock() {
        cache.insert(provider.to_string(), present);
    }
}

/// Returns `true` if at least one known provider has a key in the keychain.
fn any_known_api_key_stored() -> bool {
    known_agents().iter().any(|id| has_api_key(id.as_str()))
}

/// Best-effort write of `agent_features_enabled`. Errors reading or writing
/// settings are swallowed — failing to update the toggle should never block
/// the keychain operation that triggered it.
fn set_agent_features_enabled(value: bool) {
    if let Ok(mut settings) = read_settings() {
        if settings.agent_features_enabled != Some(value) {
            settings.agent_features_enabled = Some(value);
            let _ = write_settings(&settings);
        }
    }
}

pub fn save_api_key(provider: &str, key: &str) -> Result<(), String> {
    let had_keys_before = any_known_api_key_stored();
    secret_store::store_secret(provider, key)?;
    set_cached_presence(provider, true);

    // Auto-enable agent features the first time a key is added.
    if !had_keys_before && known_agents().iter().any(|id| id.as_str() == provider) {
        set_agent_features_enabled(true);
    }
    Ok(())
}

pub fn get_api_key(provider: &str) -> Result<String, String> {
    secret_store::load_secret(provider)
}

/// Check whether an API key exists in the keyring for this provider without
/// revealing the value.
///
/// The result is cached for the life of the process. A key added or removed by
/// another process is therefore not observed until restart, which is the same
/// staleness the settings snapshot already carries.
pub fn has_api_key(provider: &str) -> bool {
    if let Ok(cache) = presence_cache().lock() {
        if let Some(present) = cache.get(provider) {
            return *present;
        }
    }

    let present = secret_store::load_secret(provider).is_ok();
    set_cached_presence(provider, present);
    present
}

/// Remove a stored API key from the keyring.
pub fn delete_api_key(provider: &str) -> Result<(), String> {
    secret_store::delete_secret(provider)?;
    set_cached_presence(provider, false);

    // Auto-disable agent features once no recognised keys remain.
    if known_agents().iter().any(|id| id.as_str() == provider) && !any_known_api_key_stored() {
        set_agent_features_enabled(false);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Not one of `known_agents()`, so saving it never changes the
    // agent-features setting.
    const PROVIDER: &str = "credentials-test-provider";

    #[test]
    fn api_key_presence_follows_save_and_delete() {
        assert!(!has_api_key(PROVIDER));

        save_api_key(PROVIDER, "key").unwrap();
        assert!(has_api_key(PROVIDER));
        assert_eq!(get_api_key(PROVIDER).unwrap(), "key");

        delete_api_key(PROVIDER).unwrap();
        assert!(!has_api_key(PROVIDER));
        assert!(get_api_key(PROVIDER).is_err());
    }
}
