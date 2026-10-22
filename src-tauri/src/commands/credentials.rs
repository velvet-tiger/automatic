use crate::core;

// ── API Keys ─────────────────────────────────────────────────────────────────
//
// The secret store also holds MCP OAuth tokens and the cloud account's
// tokens. These commands take a caller-supplied name, so each one refuses any
// name that is not an LLM provider id before it touches the store.

#[tauri::command]
pub fn save_api_key(provider: &str, key: &str) -> Result<(), String> {
    core::agents::check_credential_provider(provider)?;
    core::save_api_key(provider, key)
}

#[tauri::command]
pub fn get_api_key(provider: &str) -> Result<String, String> {
    core::agents::check_credential_provider(provider)?;
    core::get_api_key(provider)
}

/// Whether a key is stored for this provider. An error means the keychain
/// could not be read, which is not the same as no key.
#[tauri::command]
pub fn has_api_key(provider: &str) -> Result<bool, String> {
    core::agents::check_credential_provider(provider)?;
    core::has_api_key(provider)
}

#[tauri::command]
pub fn delete_api_key(provider: &str) -> Result<(), String> {
    core::agents::check_credential_provider(provider)?;
    core::delete_api_key(provider)
}

/// Effective state of the master "Agent features" toggle. Used by the frontend
/// to gate AI buttons / inputs purely on whether the user has agent features
/// turned on, independent of whether an API key is currently stored.
#[tauri::command]
pub fn agent_features_enabled() -> bool {
    core::ai::agent_features_enabled()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::secret_store;

    #[test]
    fn credential_commands_refuse_names_that_are_not_providers() {
        // Stands in for an OAuth or account entry. No other test uses it.
        let account = "credential-commands-test-account";
        secret_store::store_secret(account, "must-not-leak").expect("store");

        let read = get_api_key(account).expect_err("read refused");
        assert!(!read.contains("must-not-leak"), "{read}");
        assert!(read.contains("Accepted provider ids"), "{read}");
        assert!(has_api_key(account).is_err(), "existence check refused");
        assert!(save_api_key(account, "planted").is_err(), "overwrite refused");
        assert!(delete_api_key(account).is_err(), "delete refused");
        assert_eq!(
            secret_store::load_secret(account).expect("still stored"),
            "must-not-leak",
            "the refused calls left the entry alone"
        );

        secret_store::delete_secret(account).expect("clean up");
    }
}
