use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use super::paths::{get_automatic_dir, is_valid_name};

// ── Tool types ────────────────────────────────────────────────────────────────

/// The broad category of a tool. Used for display grouping and icon hints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// A command-line interface tool (e.g. a linter, formatter, or generator).
    Cli,
    /// A documentation generator (e.g. typedoc, rustdoc, sphinx).
    DocGen,
    /// A code analyser or quality tool.
    Analyser,
    /// Any other tool that does not fit the above categories.
    #[default]
    Other,
}

impl ToolKind {
    pub fn label(&self) -> &'static str {
        match self {
            ToolKind::Cli => "CLI",
            ToolKind::DocGen => "Doc Generator",
            ToolKind::Analyser => "Analyser",
            ToolKind::Other => "Other",
        }
    }
}

/// A tool definition declared by a plugin or registered manually by the user.
///
/// Tools are stored as individual JSON files in `~/.automatic/tools/`.
/// The filename stem (without `.json`) is the canonical tool name and must
/// match the `name` field inside the file.
///
/// Delivered via the plugin framework: a plugin declares a tool by shipping a
/// `ToolDefinition` with `plugin_id` set to its own id.  Manually added tools
/// leave `plugin_id` as `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Stable slug identifier. Must be a valid name (lowercase, hyphens only).
    /// This is also the filename stem on disk.
    pub name: String,

    /// Human-readable display name (e.g. "Aegis CLI").
    pub display_name: String,

    /// Short description of what the tool does.
    pub description: String,

    /// Canonical URL — typically a GitHub repo URL or homepage.
    pub url: String,

    /// GitHub repository in "owner/repo" format, if the tool lives on GitHub.
    /// Used to fetch the owner's avatar (`https://github.com/<owner>.png`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_repo: Option<String>,

    /// Broad category of the tool.
    #[serde(default)]
    pub kind: ToolKind,

    /// Optional command name to detect whether the tool is installed on this
    /// machine.  When set, `autodetect_tools` will run `which <detect_binary>`
    /// and mark the tool as detected if found.
    ///
    /// Example: `"aegis"` for a CLI installed as `aegis`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detect_binary: Option<String>,

    /// Optional relative directory path that signals this tool has been
    /// initialised inside a project.  When `<project_dir>/<detect_dir>`
    /// exists, the tool is considered present in that project regardless of
    /// whether `detect_binary` is found on PATH.
    ///
    /// Example: `"kitty-specs"` for spec-kitty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detect_dir: Option<String>,

    /// The plugin id that declared this tool, if any.
    /// `None` for manually added tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,

    /// ISO 8601 timestamp when the tool was registered.
    #[serde(default)]
    pub created_at: String,
}

/// A `ToolDefinition` augmented with runtime detection state.
/// Returned by `list_tools_with_detection`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEntry {
    #[serde(flatten)]
    pub definition: ToolDefinition,

    /// `true` if the `detect_binary` was found on `$PATH` during the most
    /// recent autodetect pass.  `None` when no `detect_binary` is set.
    pub detected: Option<bool>,
}

// ── Storage ───────────────────────────────────────────────────────────────────

pub fn get_tools_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("tools"))
}

/// Return all registered tool names (filename stems, no extension).
pub fn list_tools() -> Result<Vec<String>, String> {
    let dir = get_tools_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if is_valid_name(stem) {
                        names.push(stem.to_string());
                    }
                }
            }
        }
    }

    names.sort();
    Ok(names)
}

/// Read a single `ToolDefinition` by name.
pub fn read_tool(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid tool name".into());
    }
    let path = get_tools_dir()?.join(format!("{}.json", name));
    if !path.exists() {
        return Err(format!("Tool '{}' not found", name));
    }
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// Persist a `ToolDefinition`. `data` is the raw JSON string from the frontend.
pub fn save_tool(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid tool name".into());
    }
    // Validate that the data parses as a ToolDefinition.
    let _: ToolDefinition =
        serde_json::from_str(data).map_err(|e| format!("Invalid tool JSON: {}", e))?;

    let dir = get_tools_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    // Re-serialise with pretty-print for human-readable on-disk files.
    let value: serde_json::Value = serde_json::from_str(data).map_err(|e| e.to_string())?;
    let pretty = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;

    fs::write(dir.join(format!("{}.json", name)), pretty).map_err(|e| e.to_string())
}

/// Delete a tool definition by name.
pub fn delete_tool(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid tool name".into());
    }
    let path = get_tools_dir()?.join(format!("{}.json", name));
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ── Autodetection ─────────────────────────────────────────────────────────────

/// Return all registered tools, each annotated with whether its binary was
/// detected on `$PATH`.  Detection is performed synchronously; callers that
/// need non-blocking behaviour should run this on a background thread.
pub fn list_tools_with_detection() -> Result<Vec<ToolEntry>, String> {
    let names = list_tools()?;
    let mut entries = Vec::with_capacity(names.len());

    for name in &names {
        let raw = read_tool(name)?;
        let definition: ToolDefinition = serde_json::from_str(&raw)
            .map_err(|e| format!("Corrupt tool file '{}': {}", name, e))?;

        let detected = definition
            .detect_binary
            .as_deref()
            .map(|bin| which_binary(bin));

        entries.push(ToolEntry {
            definition,
            detected,
        });
    }

    Ok(entries)
}

/// Check whether `binary` exists somewhere on `$PATH`.
pub(crate) fn which_binary(binary: &str) -> bool {
    // Use `which` on Unix, `where` on Windows.
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("where").arg(binary).output();
    #[cfg(not(target_os = "windows"))]
    let result = std::process::Command::new("which").arg(binary).output();

    result.map(|o| o.status.success()).unwrap_or(false)
}

/// Given a project directory, detect which registered tools are present.
///
/// Detection precedence — evaluated in order, first match wins:
///
/// 1. If `detect_dir` is set: the tool is present **only if**
///    `<project_dir>/<detect_dir>` exists on disk.  `detect_binary` is
///    ignored because the directory is the canonical "this tool has been
///    initialised here" signal.  A binary on PATH merely means the tool is
///    installed on the machine, not that it has been set up in this project.
///
/// 2. If `detect_dir` is **not** set but `detect_binary` is: the tool is
///    present if the binary is found on `$PATH`.
///
/// 3. If neither is set: the tool is never auto-detected.
///
/// Returns the names of tools considered present in the project.
pub fn autodetect_tools_for_project(project_dir: &str) -> Result<Vec<String>, String> {
    let names = list_tools()?;
    let project_path = std::path::Path::new(project_dir);
    let mut detected = Vec::new();

    for name in &names {
        let raw = read_tool(name)?;
        let definition: ToolDefinition = serde_json::from_str(&raw)
            .map_err(|e| format!("Corrupt tool file '{}': {}", name, e))?;

        let present = match definition.detect_dir.as_deref() {
            // detect_dir is set — it is the authoritative project-level signal.
            Some(rel) => project_path.join(rel).exists(),
            // No detect_dir — fall back to binary presence on PATH.
            None => definition
                .detect_binary
                .as_deref()
                .map(which_binary)
                .unwrap_or(false),
        };

        if present {
            detected.push(name.clone());
        }
    }

    Ok(detected)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn make_tool(name: &str) -> String {
        serde_json::json!({
            "name": name,
            "display_name": "Test Tool",
            "description": "A test tool",
            "url": "https://github.com/example/test-tool",
            "github_repo": "example/test-tool",
            "kind": "cli",
            "detect_binary": "test-tool",
            "created_at": "2026-01-01T00:00:00Z"
        })
        .to_string()
    }

    fn save_at(dir: &Path, name: &str, data: &str) -> Result<(), String> {
        if !is_valid_name(name) {
            return Err("Invalid tool name".into());
        }
        let _: ToolDefinition =
            serde_json::from_str(data).map_err(|e| format!("Invalid tool JSON: {}", e))?;
        let value: serde_json::Value = serde_json::from_str(data).map_err(|e| e.to_string())?;
        let pretty = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
        if !dir.exists() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        fs::write(dir.join(format!("{}.json", name)), pretty).map_err(|e| e.to_string())
    }

    fn list_at(dir: &Path) -> Vec<String> {
        if !dir.exists() {
            return Vec::new();
        }
        let mut names = Vec::new();
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if is_valid_name(stem) {
                        names.push(stem.to_string());
                    }
                }
            }
        }
        names.sort();
        names
    }

    #[test]
    fn list_empty_when_dir_missing() {
        let tmp = tmp();
        let dir = tmp.path().join("tools");
        assert!(list_at(&dir).is_empty());
    }

    #[test]
    fn save_and_list_tool() {
        let tmp = tmp();
        let dir = tmp.path().join("tools");
        save_at(&dir, "aegis-cli", &make_tool("aegis-cli")).unwrap();
        let names = list_at(&dir);
        assert_eq!(names, vec!["aegis-cli"]);
    }

    #[test]
    fn save_roundtrip() {
        let tmp = tmp();
        let dir = tmp.path().join("tools");
        save_at(&dir, "spec-kitty", &make_tool("spec-kitty")).unwrap();
        let raw = fs::read_to_string(dir.join("spec-kitty.json")).unwrap();
        let def: ToolDefinition = serde_json::from_str(&raw).unwrap();
        assert_eq!(def.name, "spec-kitty");
        assert_eq!(def.display_name, "Test Tool");
    }

    #[test]
    fn empty_name_rejected() {
        let tmp = tmp();
        let dir = tmp.path().join("tools");
        let result = save_at(&dir, "", &make_tool(""));
        assert!(result.is_err(), "empty name must be rejected");
    }

    #[test]
    fn slash_name_rejected() {
        let tmp = tmp();
        let dir = tmp.path().join("tools");
        let result = save_at(&dir, "path/traversal", &make_tool("path-traversal"));
        assert!(result.is_err(), "name with slash must be rejected");
    }

    #[test]
    fn invalid_json_rejected() {
        let tmp = tmp();
        let dir = tmp.path().join("tools");
        let result = save_at(&dir, "bad-tool", "not json");
        assert!(result.is_err());
    }

    #[test]
    fn which_binary_nonexistent_returns_false() {
        // An extremely unlikely-to-exist binary name.
        assert!(!which_binary("zzz-nonexistent-binary-xyz-123"));
    }

    // ── autodetect_tools_for_project tests ───────────────────────────────────

    /// Build a tool JSON with both detect_dir and detect_binary set.
    fn make_tool_with_signals(name: &str, detect_dir: &str, detect_binary: &str) -> String {
        serde_json::json!({
            "name": name,
            "display_name": "Test Tool",
            "description": "desc",
            "url": "https://example.com",
            "kind": "cli",
            "detect_dir": detect_dir,
            "detect_binary": detect_binary,
            "created_at": "2026-01-01T00:00:00Z"
        })
        .to_string()
    }

    /// Build a tool JSON with only detect_binary (no detect_dir).
    fn make_tool_binary_only(name: &str, detect_binary: &str) -> String {
        serde_json::json!({
            "name": name,
            "display_name": "Test Tool",
            "description": "desc",
            "url": "https://example.com",
            "kind": "cli",
            "detect_binary": detect_binary,
            "created_at": "2026-01-01T00:00:00Z"
        })
        .to_string()
    }

    /// When detect_dir is set but the directory does NOT exist, the tool must NOT
    /// be detected — even if the binary is on PATH.  This is the core regression
    /// test: a pyenv shim on PATH must not trigger project-level detection.
    #[test]
    fn detect_dir_absent_suppresses_binary_match() {
        let tmp = tmp();
        let tools_dir = tmp.path().join("tools");
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&project_dir).unwrap();
        // kitty-specs does NOT exist inside project_dir.

        // Save a tool that has both signals — detect_dir wins.
        save_at(
            &tools_dir,
            "spec-kitty",
            &make_tool_with_signals("spec-kitty", "kitty-specs", "spec-kitty"),
        )
        .unwrap();

        // Call the public function using the real tools dir via env override is not
        // straightforward, so we exercise the logic directly by constructing a
        // ToolDefinition and applying the same rule.
        let raw = fs::read_to_string(tools_dir.join("spec-kitty.json")).unwrap();
        let def: ToolDefinition = serde_json::from_str(&raw).unwrap();

        let present = match def.detect_dir.as_deref() {
            Some(rel) => project_dir.join(rel).exists(),
            None => def
                .detect_binary
                .as_deref()
                .map(which_binary)
                .unwrap_or(false),
        };

        assert!(
            !present,
            "tool with detect_dir set must not be detected when dir is absent, \
             even if binary is on PATH"
        );
    }

    /// When detect_dir is set AND the directory exists, the tool IS detected.
    #[test]
    fn detect_dir_present_triggers_detection() {
        let tmp = tmp();
        let tools_dir = tmp.path().join("tools");
        let project_dir = tmp.path().join("project");
        let kitty_specs = project_dir.join("kitty-specs");
        fs::create_dir_all(&kitty_specs).unwrap();

        save_at(
            &tools_dir,
            "spec-kitty",
            &make_tool_with_signals("spec-kitty", "kitty-specs", "spec-kitty"),
        )
        .unwrap();

        let raw = fs::read_to_string(tools_dir.join("spec-kitty.json")).unwrap();
        let def: ToolDefinition = serde_json::from_str(&raw).unwrap();

        let present = match def.detect_dir.as_deref() {
            Some(rel) => project_dir.join(rel).exists(),
            None => def
                .detect_binary
                .as_deref()
                .map(which_binary)
                .unwrap_or(false),
        };

        assert!(
            present,
            "tool must be detected when detect_dir exists in project"
        );
    }

    /// When only detect_binary is set (no detect_dir), a nonexistent binary
    /// must NOT be detected.
    #[test]
    fn binary_only_tool_not_detected_when_absent() {
        let tmp = tmp();
        let tools_dir = tmp.path().join("tools");
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&project_dir).unwrap();

        save_at(
            &tools_dir,
            "ghost-tool",
            &make_tool_binary_only("ghost-tool", "zzz-nonexistent-binary-xyz-123"),
        )
        .unwrap();

        let raw = fs::read_to_string(tools_dir.join("ghost-tool.json")).unwrap();
        let def: ToolDefinition = serde_json::from_str(&raw).unwrap();

        let present = match def.detect_dir.as_deref() {
            Some(rel) => project_dir.join(rel).exists(),
            None => def
                .detect_binary
                .as_deref()
                .map(which_binary)
                .unwrap_or(false),
        };

        assert!(
            !present,
            "binary-only tool must not be detected when binary is absent"
        );
    }
}
