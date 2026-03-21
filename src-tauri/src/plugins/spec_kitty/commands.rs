use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

fn configured_spec_kitty_binary() -> Result<Option<PathBuf>, String> {
    let definition = match crate::core::tools::read_tool_definition("spec-kitty") {
        Ok(definition) => definition,
        Err(_) => return Ok(None),
    };

    if let Some(path) = definition.binary_path {
        let binary = PathBuf::from(&path);
        if binary.exists() {
            return Ok(Some(binary));
        }
        return Err(format!(
            "Configured Spec Kitty binary does not exist: {}",
            path
        ));
    }

    Ok(None)
}

fn find_spec_kitty_binary() -> Result<PathBuf, String> {
    if let Some(binary) = configured_spec_kitty_binary()? {
        return Ok(binary);
    }

    if let Some(binary) = crate::core::tools::find_binary_on_path("spec-kitty") {
        return Ok(binary);
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let candidates: &[&str] = &[
        "~/.local/bin/spec-kitty",
        "~/.pyenv/shims/spec-kitty",
        "/usr/local/bin/spec-kitty",
        "/opt/homebrew/bin/spec-kitty",
    ];

    for candidate in candidates {
        let path = if candidate.starts_with('~') {
            PathBuf::from(candidate.replacen('~', &home, 1))
        } else {
            PathBuf::from(candidate)
        };

        if path.exists() {
            return Ok(path);
        }
    }

    Err(
        "Spec Kitty binary not found. Set a binary path override in Tools or install `spec-kitty` in a standard location."
            .to_string(),
    )
}

// ── Plugin dispatch ───────────────────────────────────────────────────────────

/// Route an `invoke_tool_command` call to the correct handler within this
/// plugin.  `command` is the string name the frontend passes; `payload` is
/// the raw JSON arguments object.
///
/// This is the only entry point the generic dispatcher in `commands/tools.rs`
/// needs.  No individual command function name from this module is referenced
/// outside the plugin folder.
pub fn dispatch(command: &str, payload: serde_json::Value) -> Result<serde_json::Value, String> {
    match command {
        "list_features" => {
            let project_dir: String = payload
                .get("projectDir")
                .and_then(|v| v.as_str())
                .ok_or("missing field: projectDir")?
                .to_string();
            let result = list_spec_kitty_features(project_dir)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        "get_status" => {
            let project_dir: String = payload
                .get("projectDir")
                .and_then(|v| v.as_str())
                .ok_or("missing field: projectDir")?
                .to_string();
            let feature_slug: String = payload
                .get("featureSlug")
                .and_then(|v| v.as_str())
                .ok_or("missing field: featureSlug")?
                .to_string();
            let result = get_spec_kitty_status(project_dir, feature_slug)?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        other => Err(format!("Unknown spec-kitty command: '{}'", other)),
    }
}

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
    let binary = find_spec_kitty_binary()?;
    let output = std::process::Command::new(&binary)
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
        .map_err(|e| format!("Failed to run Spec Kitty at {}: {}", binary.display(), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("spec-kitty exited with error: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<SpecKittyFeatureStatus>(&stdout)
        .map_err(|e| format!("Failed to parse spec-kitty output: {}", e))
}
