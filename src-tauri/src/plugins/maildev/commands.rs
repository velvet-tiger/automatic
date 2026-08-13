use super::process;
use super::types::MaildevStatus;

#[tauri::command]
pub fn start_maildev() -> Result<MaildevStatus, String> {
    process::start()
}

#[tauri::command]
pub fn stop_maildev() -> Result<MaildevStatus, String> {
    process::stop()
}

#[tauri::command]
pub fn get_maildev_status() -> Result<MaildevStatus, String> {
    Ok(process::status())
}
