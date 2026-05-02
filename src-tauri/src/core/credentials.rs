use keyring::Entry;

use super::agents::known_agents;
use super::settings::{read_settings, write_settings};

// ── API Keys ─────────────────────────────────────────────────────────────────

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
    let entry = Entry::new(crate::core::KEYCHAIN_SERVICE, provider).map_err(|e| e.to_string())?;
    entry.set_password(key).map_err(|e| e.to_string())?;

    // Auto-enable agent features the first time a key is added.
    if !had_keys_before && known_agents().iter().any(|id| id.as_str() == provider) {
        set_agent_features_enabled(true);
    }
    Ok(())
}

pub fn get_api_key(provider: &str) -> Result<String, String> {
    let entry = Entry::new(crate::core::KEYCHAIN_SERVICE, provider).map_err(|e| e.to_string())?;
    entry.get_password().map_err(|e| e.to_string())
}

/// Check whether an API key exists in the keyring for this provider without
/// revealing the value.
pub fn has_api_key(provider: &str) -> bool {
    let Ok(entry) = Entry::new(crate::core::KEYCHAIN_SERVICE, provider) else {
        return false;
    };
    entry.get_password().is_ok()
}

/// Remove a stored API key from the keyring.
pub fn delete_api_key(provider: &str) -> Result<(), String> {
    let entry = Entry::new(crate::core::KEYCHAIN_SERVICE, provider).map_err(|e| e.to_string())?;
    entry.delete_credential().map_err(|e| e.to_string())?;

    // Auto-disable agent features once no recognised keys remain.
    if known_agents().iter().any(|id| id.as_str() == provider) && !any_known_api_key_stored() {
        set_agent_features_enabled(false);
    }
    Ok(())
}
