use crate::core;

// ── Plugin command dispatcher ─────────────────────────────────────────────────

/// Generic entry point for all plugin-contributed commands.
///
/// The frontend calls this single command instead of per-plugin commands:
///
/// ```ts
/// invoke("invoke_tool_command", {
///   tool:    "spec-kitty",
///   command: "list_features",
///   payload: { projectDir: "/path/to/project" },
/// })
/// ```
///
/// The backend looks up the plugin by `tool`, forwards `command` and `payload`
/// to its `dispatch` function, and returns whatever the plugin returns.
///
/// This means `lib.rs` never needs to be updated when a new plugin is added.
/// Only this file (the dispatch table) and the plugin itself change.
#[tauri::command]
pub fn invoke_tool_command(
    tool: String,
    command: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match tool.as_str() {
        // ── Registered plugins ────────────────────────────────────────────────
        // To add a new plugin: add one line here mapping its tool name to its
        // dispatch function.  Nothing else in lib.rs or commands/mod.rs needs
        // to change.
        "spec-kitty" => crate::plugins::spec_kitty::dispatch(&command, payload),
        other => Err(format!("Unknown tool: '{}'", other)),
    }
}

// ── Tool registry commands ────────────────────────────────────────────────────

/// Return all registered tool names.
#[tauri::command]
pub fn list_tools() -> Result<Vec<String>, String> {
    core::list_tools()
}

/// Read a single tool definition as a JSON string.
#[tauri::command]
pub fn read_tool(name: String) -> Result<String, String> {
    core::read_tool(&name)
}

/// Persist a tool definition.  `data` is the raw JSON from the frontend.
#[tauri::command]
pub fn save_tool(name: String, data: String) -> Result<(), String> {
    core::save_tool(&name, &data)
}

/// Delete a tool definition by name.
#[tauri::command]
pub fn delete_tool(name: String) -> Result<(), String> {
    core::delete_tool(&name)
}

// ── Detection commands ────────────────────────────────────────────────────────

/// Return all registered tools annotated with binary detection state.
/// Each entry has `detected: true | false | null` depending on whether
/// a `detect_binary` is configured.
#[tauri::command]
pub fn list_tools_with_detection() -> Result<Vec<core::tools::ToolEntry>, String> {
    core::list_tools_with_detection()
}

/// Detect which registered tools are present for a given project directory.
/// Returns the names of tools whose binaries were found on `$PATH`.
#[tauri::command]
pub fn autodetect_tools_for_project(project_dir: String) -> Result<Vec<String>, String> {
    core::autodetect_tools_for_project(&project_dir)
}
