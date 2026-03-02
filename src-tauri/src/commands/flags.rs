use crate::core;
use std::collections::HashMap;

/// Return all feature flags resolved from the build-time environment.
///
/// The frontend uses this to stay in sync with the Rust layer (e.g. MCP tools)
/// without having to parse `import.meta.env` independently for cross-language
/// flag checks.
///
/// Returns a `HashMap<String, bool>` where only explicitly enabled flags are
/// present (value always `true`). Unknown / absent flags are not in the map
/// and should be treated as `false` by callers.
#[tauri::command]
pub fn get_feature_flags() -> HashMap<String, bool> {
    core::get_feature_flags()
}
