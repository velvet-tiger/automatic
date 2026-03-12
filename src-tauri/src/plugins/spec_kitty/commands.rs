use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

// ── Types returned to the frontend ───────────────────────────────────────────

/// Minimal metadata about a Spec Kitty feature, read from
/// `kitty-specs/<slug>/meta.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecKittyFeatureMeta {
    pub feature_number: String,
    pub slug: String,
    pub friendly_name: String,
    pub mission: String,
    pub source_description: String,
    pub created_at: String,
}

/// A single work package as returned by
/// `spec-kitty agent tasks status --json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecKittyWorkPackage {
    pub id: String,
    pub title: String,
    pub lane: String,
    pub phase: String,
    pub file: String,
    #[serde(default)]
    pub agent: Option<String>,
}

/// Full status payload for one feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecKittyFeatureStatus {
    pub feature: String,
    pub total_wps: u32,
    pub progress_percentage: f64,
    pub stale_wps: u32,
    pub work_packages: Vec<SpecKittyWorkPackage>,
}

// ── Commands ─────────────────────────────────────────────────────────────────

/// Scan `<project_dir>/kitty-specs/` and return the `meta.json` contents for
/// every subdirectory that contains one.  Returns an empty list when the
/// directory does not exist or no features are found.
#[tauri::command]
pub fn list_spec_kitty_features(project_dir: String) -> Result<Vec<SpecKittyFeatureMeta>, String> {
    let specs_dir = Path::new(&project_dir).join("kitty-specs");
    if !specs_dir.exists() {
        return Ok(Vec::new());
    }

    let mut features = Vec::new();

    let entries = fs::read_dir(&specs_dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let meta_path = path.join("meta.json");
        if !meta_path.exists() {
            continue;
        }
        let raw = fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
        let meta: SpecKittyFeatureMeta =
            serde_json::from_str(&raw).map_err(|e| format!("Bad meta.json: {}", e))?;
        features.push(meta);
    }

    // Sort by feature_number so the list is stable.
    features.sort_by(|a, b| a.feature_number.cmp(&b.feature_number));
    Ok(features)
}

/// Shell out to `spec-kitty agent tasks status --feature <slug> --json` in
/// `project_dir` and return the parsed status.
///
/// Returns an error string if the binary is not found, the command fails, or
/// the output cannot be parsed as JSON.
#[tauri::command]
pub fn get_spec_kitty_status(
    project_dir: String,
    feature_slug: String,
) -> Result<SpecKittyFeatureStatus, String> {
    let output = std::process::Command::new("spec-kitty")
        .args([
            "agent",
            "tasks",
            "status",
            "--feature",
            &feature_slug,
            "--json",
        ])
        .current_dir(&project_dir)
        .output()
        .map_err(|e| format!("Failed to run spec-kitty: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("spec-kitty exited with error: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<SpecKittyFeatureStatus>(&stdout)
        .map_err(|e| format!("Failed to parse spec-kitty output: {}", e))
}
