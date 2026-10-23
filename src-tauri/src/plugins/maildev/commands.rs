use super::process;
use super::types::MaildevStatus;

/// `process::start` may re-read `PATH` from the login shell, which can take
/// several seconds. Tauri runs plain commands on the main thread, so the
/// work is moved to a blocking task to keep the window responsive.
#[tauri::command]
pub async fn start_maildev() -> Result<MaildevStatus, String> {
    tokio::task::spawn_blocking(process::start)
        .await
        .map_err(|e| format!("start_maildev task join error: {e}"))?
}

#[tauri::command]
pub fn stop_maildev() -> Result<MaildevStatus, String> {
    process::stop()
}

#[tauri::command]
pub fn get_maildev_status() -> Result<MaildevStatus, String> {
    Ok(process::status())
}
