use crate::core;

// ── Instructions ─────────────────────────────────────────────────────────────
//
// Reusable markdown documents (e.g. Agent Project Brief, Session Context)
// stored at `~/.automatic/library/instructions/`.

#[tauri::command]
pub fn get_instructions() -> Result<Vec<String>, String> {
    core::list_instructions()
}

#[tauri::command]
pub fn read_instruction(name: &str) -> Result<String, String> {
    core::read_instruction(name)
}

#[tauri::command]
pub fn save_instruction(name: &str, content: &str) -> Result<(), String> {
    core::save_instruction(name, content)
}

#[tauri::command]
pub fn delete_instruction(name: &str) -> Result<(), String> {
    core::delete_instruction(name)
}
