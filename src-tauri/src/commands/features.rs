use crate::features::{Feature, FeaturePatch, FeatureUpdate, FeatureWithUpdates};

// ── Features ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_features(
    project: &str,
    state: Option<&str>,
    include_archived: Option<bool>,
) -> Result<Vec<Feature>, String> {
    crate::features::list_features(project, state, include_archived.unwrap_or(false))
}

#[tauri::command]
pub fn get_feature(project: &str, feature_id: &str) -> Result<Feature, String> {
    crate::features::get_feature(project, feature_id)
}

#[tauri::command]
pub fn get_feature_with_updates(
    project: &str,
    feature_id: &str,
) -> Result<FeatureWithUpdates, String> {
    crate::features::get_feature_with_updates(project, feature_id)
}

#[tauri::command]
pub fn create_feature(
    project: &str,
    title: &str,
    description: Option<&str>,
    priority: Option<&str>,
    assignee: Option<&str>,
    tags: Option<Vec<String>>,
    linked_files: Option<Vec<String>>,
    effort: Option<&str>,
    created_by: Option<&str>,
) -> Result<Feature, String> {
    crate::features::create_feature(
        project,
        title,
        description.unwrap_or(""),
        priority.unwrap_or("medium"),
        assignee,
        tags.as_deref().unwrap_or(&[]),
        linked_files.as_deref().unwrap_or(&[]),
        effort,
        created_by,
    )
}

#[tauri::command]
pub fn update_feature(
    project: &str,
    feature_id: &str,
    patch: FeaturePatch,
) -> Result<Feature, String> {
    crate::features::update_feature(project, feature_id, patch)
}

#[tauri::command]
pub fn set_feature_state(project: &str, feature_id: &str, state: &str) -> Result<Feature, String> {
    crate::features::set_feature_state(project, feature_id, state)
}

#[tauri::command]
pub fn move_feature(
    project: &str,
    feature_id: &str,
    new_state: &str,
    new_position: i64,
) -> Result<(), String> {
    crate::features::move_feature(project, feature_id, new_state, new_position)
}

#[tauri::command]
pub fn delete_feature(project: &str, feature_id: &str) -> Result<(), String> {
    crate::features::delete_feature(project, feature_id)
}

#[tauri::command]
pub fn archive_feature(project: &str, feature_id: &str) -> Result<Feature, String> {
    crate::features::archive_feature(project, feature_id)
}

#[tauri::command]
pub fn unarchive_feature(project: &str, feature_id: &str) -> Result<Feature, String> {
    crate::features::unarchive_feature(project, feature_id)
}

#[tauri::command]
pub fn add_feature_update(
    project: &str,
    feature_id: &str,
    content: &str,
    author: Option<&str>,
) -> Result<FeatureUpdate, String> {
    crate::features::add_feature_update(project, feature_id, content, author)
}

#[tauri::command]
pub fn get_feature_updates(project: &str, feature_id: &str) -> Result<Vec<FeatureUpdate>, String> {
    crate::features::get_feature_updates(project, feature_id)
}
