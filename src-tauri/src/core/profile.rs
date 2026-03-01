use serde::{Deserialize, Serialize};
use std::fs;

use super::paths::get_automatic_dir;

// ── User Profile (~/.automatic/profile.json) ─────────────────────────────────
//
// Stores the local user identity.  Authentication is handled externally (a
// future web-service authorisation flow will populate `clerk_id` with a
// real user ID).  Until that happens a stable machine-local UUID is generated
// on first run and used as the `clerk_id` placeholder, so that `created_by`
// relationships on projects are always consistently tagged.

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserProfile {
    /// Stable user ID.  Set to a machine-local UUID by default; replaced by
    /// the web-service user ID once authorisation is implemented.
    pub clerk_id: String,
    /// User email address (empty until set by a web service).
    #[serde(default)]
    pub email: String,
    /// Display name (empty until set by a web service).
    #[serde(default)]
    pub display_name: String,
    /// Avatar URL (none until set by a web service).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// ISO 8601 timestamp of the first time this profile was saved locally.
    #[serde(default)]
    pub created_at: String,
    /// ISO 8601 timestamp of the last profile update.
    #[serde(default)]
    pub updated_at: String,
}

fn get_profile_path() -> Result<std::path::PathBuf, String> {
    Ok(get_automatic_dir()?.join("profile.json"))
}

/// Returns the local user profile, bootstrapping a default one on first run.
///
/// If no profile file exists a stable UUID is generated and persisted as the
/// local user ID.  This ensures `userId` is always non-null in the frontend
/// and `created_by` fields on projects are consistently tagged from the
/// very first launch, ready to be replaced by a real web-service ID later.
pub fn read_profile() -> Result<Option<UserProfile>, String> {
    let path = get_profile_path()?;
    if path.exists() {
        let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let profile: UserProfile =
            serde_json::from_str(&raw).map_err(|e| format!("Invalid profile data: {}", e))?;
        return Ok(Some(profile));
    }

    // No profile on disk — bootstrap a default local profile and persist it.
    let local_id = format!("local_{}", uuid::Uuid::new_v4().simple());
    let now = chrono::Utc::now().to_rfc3339();
    let default_profile = UserProfile {
        clerk_id: local_id,
        email: String::new(),
        display_name: String::new(),
        avatar_url: None,
        created_at: now.clone(),
        updated_at: now,
    };
    save_profile(&default_profile)?;
    Ok(Some(default_profile))
}

/// Save or update the local user profile.  On the first save the `created_at`
/// field is set; subsequent saves only update `updated_at`.
pub fn save_profile(profile: &UserProfile) -> Result<(), String> {
    let path = get_profile_path()?;
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }

    let mut to_save = profile.clone();
    let now = chrono::Utc::now().to_rfc3339();

    // Preserve original created_at if the file already exists.
    // Read the file directly instead of calling read_profile() to avoid
    // mutual recursion (read_profile → save_profile → read_profile → …)
    // which causes a stack overflow on first run when no profile exists.
    if path.exists() {
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(existing) = serde_json::from_str::<UserProfile>(&raw) {
                if !existing.created_at.is_empty() {
                    to_save.created_at = existing.created_at;
                }
            }
        }
    }
    if to_save.created_at.is_empty() {
        to_save.created_at = now.clone();
    }
    to_save.updated_at = now;

    let raw = serde_json::to_string_pretty(&to_save).map_err(|e| e.to_string())?;
    fs::write(&path, raw).map_err(|e| e.to_string())
}
