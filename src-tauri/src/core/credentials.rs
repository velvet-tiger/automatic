use keyring::Entry;

// ── API Keys ─────────────────────────────────────────────────────────────────

pub fn save_api_key(provider: &str, key: &str) -> Result<(), String> {
    let entry = Entry::new(crate::core::KEYCHAIN_SERVICE, provider).map_err(|e| e.to_string())?;
    entry.set_password(key).map_err(|e| e.to_string())
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
    entry.delete_credential().map_err(|e| e.to_string())
}
