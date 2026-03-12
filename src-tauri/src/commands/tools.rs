use crate::core;

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
