use crate::core::task_log::{append_task_log_entries, read_task_log, PersistedTaskLogEntry};

#[tauri::command]
pub fn get_task_log() -> Result<Vec<PersistedTaskLogEntry>, String> {
    read_task_log()
}

#[tauri::command]
pub fn append_task_log(entries: Vec<PersistedTaskLogEntry>) -> Result<(), String> {
    append_task_log_entries(entries)
}
