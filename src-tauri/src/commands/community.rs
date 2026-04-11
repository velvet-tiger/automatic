use crate::core;

/// Return the featured community JSON, refreshing from the remote endpoint
/// if the local cache is older than one hour.
#[tauri::command]
pub async fn get_featured_community() -> Result<String, String> {
    core::get_featured_community().await
}
