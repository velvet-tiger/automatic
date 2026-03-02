use std::collections::HashMap;

/// Resolve feature flags from build-time environment variables.
///
/// The single source for the Rust side is the `VITE_FLAGS` environment variable
/// read at **compile time** via `option_env!()`, which keeps the contract
/// identical to the frontend: both sides see the same value baked into the
/// binary at build time.
///
/// Flag names are normalised to lowercase. A flag present in the comma-separated
/// list is `true`; everything else defaults to `false` (keys are absent rather
/// than stored as `false` to keep the map small and the serialisation clean).
///
/// Future extension point: callers (e.g. an MCP tool) can pass additional
/// remote flag overrides as `Option<HashMap<String, bool>>` and merge them on
/// top.
pub fn resolve_flags(remote: Option<HashMap<String, bool>>) -> HashMap<String, bool> {
    let mut flags: HashMap<String, bool> = HashMap::new();

    // ── Compile-time env ─────────────────────────────────────────────────────
    // VITE_FLAGS is a comma-separated list: "flag_a,flag_b"
    // Using option_env! keeps API keys and flags off the filesystem at runtime.
    let raw: &str = option_env!("VITE_FLAGS").unwrap_or("");
    for token in raw.split(',') {
        let key = token.trim().to_lowercase();
        if !key.is_empty() {
            flags.insert(key, true);
        }
    }

    // ── Remote overrides (future / injected) ─────────────────────────────────
    if let Some(overrides) = remote {
        for (k, v) in overrides {
            flags.insert(k.to_lowercase(), v);
        }
    }

    flags
}

/// Convenience: return the flag map with no remote overrides applied.
pub fn get_feature_flags() -> HashMap<String, bool> {
    resolve_flags(None)
}
