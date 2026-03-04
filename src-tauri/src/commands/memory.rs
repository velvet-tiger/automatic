use crate::core;
use crate::memory;

// ── Memory ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_project_memories(project: &str) -> Result<memory::MemoryDb, String> {
    memory::get_all_memories(project)
}

#[tauri::command]
pub fn store_memory(
    project: &str,
    key: &str,
    value: &str,
    source: Option<&str>,
) -> Result<String, String> {
    memory::store_memory(project, key, value, source)
}

#[tauri::command]
pub fn get_memory(project: &str, key: &str) -> Result<String, String> {
    memory::get_memory(project, key)
}

#[tauri::command]
pub fn list_memories(project: &str, pattern: Option<&str>) -> Result<String, String> {
    memory::list_memories(project, pattern)
}

#[tauri::command]
pub fn search_memories(project: &str, query: &str) -> Result<String, String> {
    memory::search_memories(project, query)
}

#[tauri::command]
pub fn delete_memory(project: &str, key: &str) -> Result<String, String> {
    memory::delete_memory(project, key)
}

#[tauri::command]
pub fn clear_memories(
    project: &str,
    pattern: Option<&str>,
    confirm: bool,
) -> Result<String, String> {
    memory::clear_memories(project, pattern, confirm)
}

/// Returns Claude Code's auto-memory content for the given project.
///
/// Looks up the project's directory from the Automatic registry, then derives
/// the Claude memory path (`~/.claude/projects/<encoded>/memory/`) and reads
/// `MEMORY.md` plus any topic files present.
#[tauri::command]
pub fn get_claude_memory(project: &str) -> Result<memory::ClaudeMemoryContent, String> {
    let project_json = core::read_project(project)?;
    let p: crate::core::Project =
        serde_json::from_str(&project_json).map_err(|e| format!("Invalid project data: {}", e))?;
    memory::read_claude_memory(&p.directory)
}
