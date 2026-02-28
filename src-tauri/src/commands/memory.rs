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
