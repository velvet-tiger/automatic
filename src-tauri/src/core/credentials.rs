use keyring::Entry;

// ── API Keys ─────────────────────────────────────────────────────────────────

pub fn save_api_key(provider: &str, key: &str) -> Result<(), String> {
    let entry = Entry::new("automatic_desktop", provider).map_err(|e| e.to_string())?;
    entry.set_password(key).map_err(|e| e.to_string())
}

pub fn get_api_key(provider: &str) -> Result<String, String> {
    let entry = Entry::new("automatic_desktop", provider).map_err(|e| e.to_string())?;
    entry.get_password().map_err(|e| e.to_string())
}
