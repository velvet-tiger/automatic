use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use super::*;

// ── Project Templates ─────────────────────────────────────────────────────────
//
// Project Templates capture agents, skills, MCP servers and a description that
// can be applied when creating a new project or merged into an existing one.
// Stored as JSON files in `~/.automatic/project_templates/{name}.json`.

/// A single project file stored inline in a project template.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct TemplateProjectFile {
    pub filename: String,
    #[serde(default)]
    pub content: String,
}

/// A template that captures the shareable parts of a project configuration.
/// Excludes per-project fields like `directory`, `created_at`, `updated_at`.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProjectTemplate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    /// Project files (e.g. CLAUDE.md) stored inline so they can be written
    /// to a project's directory when the template is applied.
    #[serde(default)]
    pub project_files: Vec<TemplateProjectFile>,
    /// Single unified project instruction content (written to all agent
    /// instruction files when the template is applied in unified mode).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unified_instruction: String,
    /// Rule IDs attached to the unified instruction. These are written into
    /// the project's `file_rules["_unified"]` when the template is applied.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unified_rules: Vec<String>,
}

pub fn get_project_templates_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".automatic/project_templates"))
}

pub fn list_project_templates() -> Result<Vec<String>, String> {
    let dir = get_project_templates_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut templates = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;
    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if is_valid_name(stem) {
                        templates.push(stem.to_string());
                    }
                }
            }
        }
    }
    templates.sort();
    Ok(templates)
}

pub fn read_project_template(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid template name".into());
    }
    let dir = get_project_templates_dir()?;
    let path = dir.join(format!("{}.json", name));
    if path.exists() {
        fs::read_to_string(path).map_err(|e| e.to_string())
    } else {
        Err(format!("Project template '{}' not found", name))
    }
}

pub fn save_project_template(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid template name".into());
    }

    // Validate that data is valid JSON for a ProjectTemplate
    let template: ProjectTemplate =
        serde_json::from_str(data).map_err(|e| format!("Invalid template data: {}", e))?;
    let pretty = serde_json::to_string_pretty(&template).map_err(|e| e.to_string())?;

    let dir = get_project_templates_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let path = dir.join(format!("{}.json", name));
    fs::write(path, pretty).map_err(|e| e.to_string())
}

pub fn delete_project_template(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid template name".into());
    }
    let dir = get_project_templates_dir()?;
    let path = dir.join(format!("{}.json", name));
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn rename_project_template(old_name: &str, new_name: &str) -> Result<(), String> {
    if !is_valid_name(old_name) {
        return Err("Invalid current template name".into());
    }
    if !is_valid_name(new_name) {
        return Err("Invalid new template name".into());
    }
    if old_name == new_name {
        return Ok(());
    }

    let dir = get_project_templates_dir()?;
    let old_path = dir.join(format!("{}.json", old_name));
    let new_path = dir.join(format!("{}.json", new_name));

    if !old_path.exists() {
        return Err(format!("Project template '{}' not found", old_name));
    }
    if new_path.exists() {
        return Err(format!(
            "A project template named '{}' already exists",
            new_name
        ));
    }

    // Read, update name field, write to new path, remove old
    let raw = fs::read_to_string(&old_path).map_err(|e| e.to_string())?;
    let mut template: ProjectTemplate =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid template data: {}", e))?;
    template.name = new_name.to_string();
    let pretty = serde_json::to_string_pretty(&template).map_err(|e| e.to_string())?;
    fs::write(&new_path, pretty).map_err(|e| e.to_string())?;
    fs::remove_file(&old_path).map_err(|e| e.to_string())?;

    Ok(())
}

// ── Bundled Project Template Marketplace ─────────────────────────────────────
//
// Templates shipped with the app, compiled in via `include_str!`.
// These are served to the Template Marketplace UI without any network calls.
// Users can import them into `~/.automatic/project_templates/` as editable copies.

/// A bundled project template marketplace entry (richer than ProjectTemplate).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BundledProjectTemplate {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub skills: Vec<String>,
    pub mcp_servers: Vec<String>,
    pub providers: Vec<String>,
    pub agents: Vec<String>,
    pub project_files: Vec<TemplateProjectFile>,
    #[serde(default)]
    pub unified_instruction: String,
    #[serde(default)]
    pub unified_rules: Vec<String>,
    /// Optional icon filename (png or svg) relative to the template-icons asset
    /// directory, e.g. "nextjs.svg". Served at /template-icons/<icon> in the
    /// frontend. When absent the UI falls back to the first letter of the name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// All bundled marketplace templates, compiled in at build time.
const BUNDLED_TEMPLATES: &[(&str, &str)] = &[
    (
        "nextjs-saas-starter",
        include_str!("../../project-templates/nextjs-saas-starter.json"),
    ),
    (
        "laravel-api-backend",
        include_str!("../../project-templates/laravel-api-backend.json"),
    ),
    (
        "python-data-pipeline",
        include_str!("../../project-templates/python-data-pipeline.json"),
    ),
    (
        "tauri-desktop-app",
        include_str!("../../project-templates/tauri-desktop-app.json"),
    ),
    (
        "terraform-aws-infrastructure",
        include_str!("../../project-templates/terraform-aws-infrastructure.json"),
    ),
    (
        "react-component-library",
        include_str!("../../project-templates/react-component-library.json"),
    ),
];

/// Return all bundled marketplace templates as JSON array.
pub fn list_bundled_project_templates() -> Result<String, String> {
    let templates: Result<Vec<BundledProjectTemplate>, _> = BUNDLED_TEMPLATES
        .iter()
        .map(|(_, raw)| serde_json::from_str::<BundledProjectTemplate>(raw))
        .collect();

    let templates = templates.map_err(|e| format!("Failed to parse bundled template: {}", e))?;
    serde_json::to_string(&templates).map_err(|e| e.to_string())
}

/// Return a single bundled marketplace template by name as JSON.
pub fn read_bundled_project_template(name: &str) -> Result<String, String> {
    for (slug, raw) in BUNDLED_TEMPLATES {
        if *slug == name {
            return Ok(raw.to_string());
        }
    }
    Err(format!("Bundled template '{}' not found", name))
}

/// Import a bundled marketplace template into the user's local project templates.
/// If a template with the same name already exists it is overwritten.
/// Any skills listed in the template that are bundled with the app are installed
/// to `~/.agents/skills/` at this point (skipping any already present).
pub fn import_bundled_project_template(name: &str) -> Result<(), String> {
    let raw = read_bundled_project_template(name)?;
    let bundled: BundledProjectTemplate =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid template: {}", e))?;

    // Install any bundled skills the template requires (skip already-installed ones).
    install_skills_from_bundle(&bundled.skills)?;

    // Convert to the standard ProjectTemplate structure for storage
    let pt = ProjectTemplate {
        name: bundled.name.clone(),
        description: bundled.description,
        skills: bundled.skills,
        mcp_servers: bundled.mcp_servers,
        providers: bundled.providers,
        agents: bundled.agents,
        project_files: bundled.project_files,
        unified_instruction: bundled.unified_instruction,
        unified_rules: bundled.unified_rules,
    };

    let json = serde_json::to_string_pretty(&pt).map_err(|e| e.to_string())?;
    save_project_template(&bundled.name, &json)
}

/// Search bundled templates by query (matches name, display_name, description, tags, category).
pub fn search_bundled_project_templates(query: &str) -> Result<String, String> {
    let q = query.to_lowercase();
    let templates: Result<Vec<BundledProjectTemplate>, _> = BUNDLED_TEMPLATES
        .iter()
        .map(|(_, raw)| serde_json::from_str::<BundledProjectTemplate>(raw))
        .collect();

    let templates = templates.map_err(|e| format!("Failed to parse bundled template: {}", e))?;

    if q.trim().is_empty() {
        let json = serde_json::to_string(&templates).map_err(|e| e.to_string())?;
        return Ok(json);
    }

    let filtered: Vec<&BundledProjectTemplate> = templates
        .iter()
        .filter(|t| {
            t.name.to_lowercase().contains(&q)
                || t.display_name.to_lowercase().contains(&q)
                || t.description.to_lowercase().contains(&q)
                || t.category.to_lowercase().contains(&q)
                || t.tags.iter().any(|tag| tag.to_lowercase().contains(&q))
        })
        .collect();

    serde_json::to_string(&filtered).map_err(|e| e.to_string())
}

// ── Template Dependency Checking ─────────────────────────────────────────────

/// The status of a single skill dependency for a template.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillDependencyStatus {
    /// Skill name as listed in the template (e.g. "vercel-react-best-practices").
    pub name: String,
    /// Whether the skill is currently installed locally.
    pub installed: bool,
    /// Whether the skill is shipped with the app and can be installed without
    /// a network call.  If `true` and `installed` is `false`, importing the
    /// template will install it automatically.
    pub bundled: bool,
}

/// Dependency check result for a template.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TemplateDependencyReport {
    /// Dependency status for every skill the template requires.
    pub skills: Vec<SkillDependencyStatus>,
    /// MCP server names required by the template that are not configured locally.
    pub missing_mcp_servers: Vec<String>,
}

/// Check which skills and MCP servers a bundled template requires are missing
/// locally.  Bundled skills (shipped with the app) are flagged as installable
/// without a network call — no skills.sh lookup is performed.
pub fn check_template_dependencies(template_name: &str) -> Result<String, String> {
    let raw = read_bundled_project_template(template_name)?;
    let bundled: BundledProjectTemplate =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid template JSON: {}", e))?;

    let installed_names: std::collections::HashSet<String> =
        list_skill_names().unwrap_or_default().into_iter().collect();

    let installed_mcp: std::collections::HashSet<String> = list_mcp_server_configs()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let bundled_names: std::collections::HashSet<&str> =
        bundled_skill_names().into_iter().collect();

    let skill_statuses: Vec<SkillDependencyStatus> = bundled
        .skills
        .iter()
        .map(|skill_name| SkillDependencyStatus {
            name: skill_name.clone(),
            installed: installed_names.contains(skill_name.as_str()),
            bundled: bundled_names.contains(skill_name.as_str()),
        })
        .collect();

    let missing_mcp_servers: Vec<String> = bundled
        .mcp_servers
        .into_iter()
        .filter(|s| !installed_mcp.contains(s.as_str()))
        .collect();

    let report = TemplateDependencyReport {
        skills: skill_statuses,
        missing_mcp_servers,
    };

    serde_json::to_string(&report).map_err(|e| e.to_string())
}
