use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use super::discover_data::read_templates_json;
use super::*;

// ── Project Templates ─────────────────────────────────────────────────────────
//
// Project Templates capture agents, skills, MCP servers and a description that
// can be applied when creating a new project or merged into an existing one.
// Stored as JSON files in `~/.automatic/library/templates/{name}.json`.

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
    /// Workspace sub-agent IDs (from `~/.automatic/agents/`) to include when
    /// this template is applied to a project.  These map to the project's
    /// `user_agents` field and are written to each agent's sub-agent directory
    /// (e.g. `.claude/agents/`) during sync.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_agents: Vec<String>,
    /// Workspace command names (from `~/.automatic/commands/`) to include when
    /// this template is applied to a project.  These map to the project's
    /// `user_commands` field and are first written to `.agents/commands/`
    /// during sync before any agent-specific linking or conversion happens.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_commands: Vec<String>,
    /// Author/provider metadata.  Mirrors the `_author` convention used by
    /// MCP server configs.  Stored as a raw JSON value so it round-trips
    /// without a dedicated struct — shape: `{ type, name?, url?, repo?, ... }`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _author: Option<serde_json::Value>,
}

pub fn get_templates_dir() -> Result<PathBuf, String> {
    Ok(super::paths::get_library_dir()?.join("templates"))
}

pub fn list_templates() -> Result<Vec<String>, String> {
    let dir = get_templates_dir()?;
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

pub fn read_template(name: &str) -> Result<String, String> {
    if !is_valid_name(name) {
        return Err("Invalid template name".into());
    }
    let dir = get_templates_dir()?;
    let path = dir.join(format!("{}.json", name));
    if path.exists() {
        let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut template: ProjectTemplate =
            serde_json::from_str(&raw).map_err(|e| format!("Invalid template data: {}", e))?;
        if template._author.is_none() {
            template._author = super::remote_sources::get_provenance_author("template", name)?;
        }
        serde_json::to_string(&template).map_err(|e| e.to_string())
    } else {
        Err(format!("Project template '{}' not found", name))
    }
}

pub fn save_template(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid template name".into());
    }

    // Validate that data is valid JSON for a ProjectTemplate
    let template: ProjectTemplate =
        serde_json::from_str(data).map_err(|e| format!("Invalid template data: {}", e))?;
    let pretty = serde_json::to_string_pretty(&template).map_err(|e| e.to_string())?;

    let dir = get_templates_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let path = dir.join(format!("{}.json", name));
    let is_new = !path.exists();
    fs::write(&path, pretty).map_err(|e| e.to_string())?;

    if is_new {
        record_recently_added("templates", name);
    }

    Ok(())
}

pub fn delete_template(name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid template name".into());
    }
    let dir = get_templates_dir()?;
    let path = dir.join(format!("{}.json", name));
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    remove_recently_added("templates", name);
    Ok(())
}

pub fn rename_template(old_name: &str, new_name: &str) -> Result<(), String> {
    if !is_valid_name(old_name) {
        return Err("Invalid current template name".into());
    }
    if !is_valid_name(new_name) {
        return Err("Invalid new template name".into());
    }
    if old_name == new_name {
        return Ok(());
    }

    let dir = get_templates_dir()?;
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

// ── Bundled Project Template Discover ────────────────────────────────────────
//
// Templates shipped with the app, compiled in via `include_str!`.
// These are served to the Discover Templates UI without any network calls.
// Users can import them into `~/.automatic/library/templates/` as editable copies.

/// A bundled project template Discover entry (richer than ProjectTemplate).
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
    /// Workspace command names to include when this template is imported.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_commands: Vec<String>,
    /// Optional icon filename (png or svg) relative to the template-icons asset
    /// directory, e.g. "nextjs.svg". Served at /template-icons/<icon> in the
    /// frontend. When absent the UI falls back to the first letter of the name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Author/provider metadata, same shape as `ProjectTemplate::_author`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _author: Option<serde_json::Value>,
    /// Maps community skill name → GitHub source (e.g. `"wshobson/agents"`).
    /// Used during template import to auto-fetch skills that are not bundled
    /// with the app.  Bundled skills are installed without a network call;
    /// community skills listed here are fetched from raw.githubusercontent.com.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub skill_sources: HashMap<String, String>,
}

/// All bundled Discover templates, compiled in at build time.
/// `pub(super)` so `discover_data` can reference the raw strings for seeding.
pub(super) const BUNDLED_TEMPLATES: &[(&str, &str)] = &[
    (
        "software-defaults",
        include_str!("../../assets/discover/project-templates/software-defaults.json"),
    ),
    (
        "nextjs-saas-starter",
        include_str!("../../assets/discover/project-templates/nextjs-saas-starter.json"),
    ),
    (
        "laravel-api-backend",
        include_str!("../../assets/discover/project-templates/laravel-api-backend.json"),
    ),
    (
        "python-data-pipeline",
        include_str!("../../assets/discover/project-templates/python-data-pipeline.json"),
    ),
    (
        "tauri-desktop-app",
        include_str!("../../assets/discover/project-templates/tauri-desktop-app.json"),
    ),
    (
        "terraform-aws-infrastructure",
        include_str!(
            "../../assets/discover/project-templates/terraform-aws-infrastructure.json"
        ),
    ),
    (
        "react-component-library",
        include_str!("../../assets/discover/project-templates/react-component-library.json"),
    ),
    (
        "django-web-app",
        include_str!("../../assets/discover/project-templates/django-web-app.json"),
    ),
    (
        "fastapi-service",
        include_str!("../../assets/discover/project-templates/fastapi-service.json"),
    ),
    (
        "react-native-app",
        include_str!("../../assets/discover/project-templates/react-native-app.json"),
    ),
    (
        "rust-cli-app",
        include_str!("../../assets/discover/project-templates/rust-cli-app.json"),
    ),
    (
        "supabase-backend",
        include_str!("../../assets/discover/project-templates/supabase-backend.json"),
    ),
    (
        "graphql-api",
        include_str!("../../assets/discover/project-templates/graphql-api.json"),
    ),
    (
        "docker-containerised-service",
        include_str!(
            "../../assets/discover/project-templates/docker-containerised-service.json"
        ),
    ),
    (
        "ruby-on-rails-api",
        include_str!("../../assets/discover/project-templates/ruby-on-rails-api.json"),
    ),
];

/// Return all bundled Discover templates as JSON array.
/// Reads from `~/.automatic/discover/templates.json` (disk is sole source of truth).
pub fn list_bundled_templates() -> Result<String, String> {
    read_templates_json()
}

/// Return a single bundled Discover template by name as JSON.
/// Reads from `~/.automatic/discover/templates.json`.
pub fn read_bundled_template(name: &str) -> Result<String, String> {
    let json = read_templates_json()?;
    let templates: Vec<serde_json::Value> =
        serde_json::from_str(&json).map_err(|e| format!("Failed to parse templates: {}", e))?;

    for tmpl in &templates {
        if tmpl.get("name").and_then(|v| v.as_str()) == Some(name) {
            return serde_json::to_string(tmpl).map_err(|e| e.to_string());
        }
    }

    Err(format!("Bundled template '{}' not found", name))
}

/// Import a bundled Discover template into the user's local project templates.
/// If a template with the same name already exists it is overwritten.
///
/// Install order:
/// 1. Bundled skills — installed synchronously from the compiled-in binary.
/// 2. Community skills — fetched asynchronously from raw.githubusercontent.com
///    using the `skill_sources` map in the template.  Errors are logged but
///    never propagate; a failed community skill fetch does not abort the import.
pub async fn import_bundled_template(name: &str) -> Result<(), String> {
    let raw = read_bundled_template(name)?;
    let bundled: BundledProjectTemplate =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid template: {}", e))?;

    // Step 1: install skills that are bundled with the app (no network).
    super::install_skills_from_bundle(&bundled.skills)?;

    // Step 2: fetch and install community skills (network, best-effort).
    install_community_skills(&bundled).await;

    // Convert to the standard ProjectTemplate structure for storage.
    // _author is preserved so the user-local copy retains provenance.
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
        user_agents: Vec::new(),
        user_commands: bundled.user_commands,
        _author: bundled._author,
    };

    let json = serde_json::to_string_pretty(&pt).map_err(|e| e.to_string())?;
    save_template(&bundled.name, &json)
}

/// Fetch and install community skills listed in a bundled template.
///
/// Only processes skills that are not already in the app bundle and are not
/// already installed locally.  Each skill is fetched using the source from
/// `template.skill_sources`.  Skills without a source entry are silently
/// skipped — they remain listed as "install manually" in the dep panel.
///
/// All errors are logged to stderr; none propagate so a single failed fetch
/// never aborts the overall template import.
async fn install_community_skills(template: &BundledProjectTemplate) {
    if template.skill_sources.is_empty() {
        return;
    }

    let bundled_names: std::collections::HashSet<&str> =
        super::bundled_skill_names().into_iter().collect();

    let installed_names: std::collections::HashSet<String> =
        list_skill_names().unwrap_or_default().into_iter().collect();

    for skill_name in &template.skills {
        // Bundled skills are already handled by install_skills_from_bundle.
        if bundled_names.contains(skill_name.as_str()) {
            continue;
        }
        // Skip skills the user already has installed.
        if installed_names.contains(skill_name) {
            continue;
        }
        let Some(source) = template.skill_sources.get(skill_name) else {
            // No source info — leave for the user to install manually.
            continue;
        };

        match fetch_remote_skill_content(source, skill_name).await {
            Ok(content) => {
                if let Err(e) = save_skill(skill_name, &content) {
                    eprintln!(
                        "[automatic] template import: failed to save community skill '{}': {}",
                        skill_name, e
                    );
                } else {
                    let id = format!("{}/{}", source, skill_name);
                    if let Err(e) = record_skill_source(skill_name, source, &id, "github") {
                        eprintln!(
                            "[automatic] template import: failed to record source for '{}': {}",
                            skill_name, e
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "[automatic] template import: failed to fetch community skill '{}' from '{}': {}",
                    skill_name, source, e
                );
            }
        }
    }
}

// ── Apply Templates to Project ──────────────────────────────────────────────

/// Result of applying one or more templates to a project.
/// Returned to the frontend so it can update its local state.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApplyTemplatesResult {
    /// The updated project JSON (same shape the frontend already works with).
    pub project: Project,
    /// Unified instruction entries to apply (content + rule IDs).
    /// Non-empty when at least one template has unified_instruction or
    /// unified_rules set — the frontend uses these to populate the
    /// instruction editor.
    pub pending_unified: Vec<PendingUnifiedEntry>,
}

/// A single unified instruction entry from a template.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PendingUnifiedEntry {
    pub content: String,
    pub rules: Vec<String>,
}

/// Merge one or more templates into an existing project.
///
/// Each template's assets are unioned into the project. Unified instructions
/// are collected and returned (not written to disk here — the frontend feeds
/// them into the instruction editor so the user can review before saving).
///
/// `project_files` from templates are written to the project directory
/// immediately via `save_project_file`.
pub fn apply_templates_to_project(
    project_name: &str,
    template_names: &[String],
) -> Result<ApplyTemplatesResult, String> {
    if template_names.is_empty() {
        return Err("No templates specified".into());
    }

    let raw = read_project(project_name)?;
    let mut project: Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    let mut templates = Vec::new();
    for tmpl_name in template_names {
        let tmpl_raw = read_template(tmpl_name)?;
        let tmpl: ProjectTemplate = serde_json::from_str(&tmpl_raw)
            .map_err(|e| format!("Invalid template '{}': {}", tmpl_name, e))?;
        templates.push(tmpl);
    }

    let result = merge_templates_into_project(&mut project, &templates)?;

    project.updated_at = chrono::Utc::now().to_rfc3339();

    let pretty = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    save_project(project_name, &pretty)?;

    Ok(result)
}

/// Pure merge logic: applies template assets to a project in place.
///
/// This is the single source of truth for which fields get merged and how.
/// Extracted from `apply_templates_to_project` so the merge can be tested
/// without filesystem I/O.
pub(crate) fn merge_templates_into_project(
    project: &mut Project,
    templates: &[ProjectTemplate],
) -> Result<ApplyTemplatesResult, String> {
    let mut pending_unified: Vec<PendingUnifiedEntry> = Vec::new();
    let mut any_unified = false;

    for tmpl in templates {
        union_vec(&mut project.agents, &tmpl.agents);
        union_vec(&mut project.skills, &tmpl.skills);
        union_vec(&mut project.mcp_servers, &tmpl.mcp_servers);
        union_vec(&mut project.providers, &tmpl.providers);
        union_vec(&mut project.user_agents, &tmpl.user_agents);
        union_vec(&mut project.user_commands, &tmpl.user_commands);

        if project.description.is_empty() && !tmpl.description.is_empty() {
            project.description.clone_from(&tmpl.description);
        }

        let has_content = !tmpl.unified_instruction.trim().is_empty();
        let has_rules = !tmpl.unified_rules.is_empty();
        // Collect pending entry whenever there is content OR rules to apply.
        // Rules alone are still returned so the frontend can persist them to
        // file_rules._project — but they do NOT trigger a mode switch to
        // "unified", because writing empty content in unified mode would
        // overwrite existing per-agent instruction files.
        if has_content || has_rules {
            pending_unified.push(PendingUnifiedEntry {
                content: tmpl.unified_instruction.clone(),
                rules: tmpl.unified_rules.clone(),
            });
        }
        if has_content {
            any_unified = true;
        }

        // Write template project files to the project directory.
        if !project.directory.is_empty() {
            for pf in &tmpl.project_files {
                if !pf.filename.is_empty() && !pf.content.is_empty() {
                    save_project_file(&project.directory, &pf.filename, &pf.content)?;
                }
            }
        }
    }

    if any_unified {
        project.instruction_mode = "unified".to_string();
    }

    Ok(ApplyTemplatesResult {
        project: project.clone(),
        pending_unified,
    })
}

/// Append items from `source` to `target`, skipping duplicates.
fn union_vec(target: &mut Vec<String>, source: &[String]) {
    for item in source {
        if !target.contains(item) {
            target.push(item.clone());
        }
    }
}

/// Search bundled templates by query (matches name, display_name, description, tags, category).
/// Reads from `~/.automatic/discover/templates.json`.
pub fn search_bundled_templates(query: &str) -> Result<String, String> {
    let json = read_templates_json()?;
    let templates: Vec<BundledProjectTemplate> =
        serde_json::from_str(&json).map_err(|e| format!("Failed to parse templates: {}", e))?;

    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return serde_json::to_string(&templates).map_err(|e| e.to_string());
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
    let raw = read_bundled_template(template_name)?;
    let bundled: BundledProjectTemplate =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid template JSON: {}", e))?;

    let installed_names: std::collections::HashSet<String> =
        list_skill_names().unwrap_or_default().into_iter().collect();

    let installed_mcp: std::collections::HashSet<String> = list_mcp_server_configs()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let bundled_names: std::collections::HashSet<&str> =
        super::bundled_skill_names().into_iter().collect();

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::with_test_home;
    use std::fs;

    fn empty_project() -> Project {
        Project {
            name: "test-project".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            ..Default::default()
        }
    }

    fn make_template(name: &str) -> ProjectTemplate {
        ProjectTemplate {
            name: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn read_template_hydrates_author_from_remote_provenance() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            let dir = get_templates_dir().expect("templates dir");
            fs::create_dir_all(&dir).expect("create templates dir");
            let template = ProjectTemplate {
                name: "remote-template".into(),
                description: "Remote template".into(),
                ..Default::default()
            };
            fs::write(
                dir.join("remote-template.json"),
                serde_json::to_string_pretty(&template).expect("serialize template"),
            )
            .expect("write template");
            super::super::remote_sources::record_provenance(
                "template",
                "remote-template",
                "octocat/remote-templates",
            )
            .expect("record provenance");

            let raw = read_template("remote-template").expect("read template");
            let hydrated: ProjectTemplate = serde_json::from_str(&raw).expect("parse template");
            let author = hydrated._author.expect("author metadata");

            assert_eq!(author["type"].as_str(), Some("github"));
            assert_eq!(author["repo"].as_str(), Some("octocat/remote-templates"));
        });
    }

    // ── All asset types are merged ──────────────────────────────────────────

    #[test]
    fn merge_unions_all_asset_types() {
        let mut project = empty_project();
        project.skills = vec!["existing-skill".into()];

        let tmpl = ProjectTemplate {
            name: "full-template".into(),
            description: "A template".into(),
            skills: vec!["new-skill".into(), "existing-skill".into()],
            mcp_servers: vec!["github-mcp".into()],
            providers: vec!["openai".into()],
            agents: vec!["claude".into()],
            user_agents: vec!["researcher".into()],
            user_commands: vec!["lint".into()],
            ..Default::default()
        };

        let result =
            merge_templates_into_project(&mut project, &[tmpl]).expect("merge should succeed");

        assert_eq!(
            result.project.skills,
            vec!["existing-skill", "new-skill"],
            "skills should be unioned with existing first"
        );
        assert_eq!(result.project.mcp_servers, vec!["github-mcp"]);
        assert_eq!(result.project.providers, vec!["openai"]);
        assert_eq!(result.project.agents, vec!["claude"]);
        assert_eq!(result.project.user_agents, vec!["researcher"]);
        assert_eq!(result.project.user_commands, vec!["lint"]);
        assert_eq!(result.project.description, "A template");
    }

    #[test]
    fn merge_deduplicates_across_templates() {
        let mut project = empty_project();

        let tmpl1 = ProjectTemplate {
            name: "tmpl1".into(),
            skills: vec!["skill-a".into(), "skill-b".into()],
            agents: vec!["claude".into()],
            user_agents: vec!["agent-x".into()],
            ..Default::default()
        };
        let tmpl2 = ProjectTemplate {
            name: "tmpl2".into(),
            skills: vec!["skill-b".into(), "skill-c".into()],
            agents: vec!["claude".into(), "opencode".into()],
            user_agents: vec!["agent-x".into(), "agent-y".into()],
            ..Default::default()
        };

        let result = merge_templates_into_project(&mut project, &[tmpl1, tmpl2])
            .expect("merge should succeed");

        assert_eq!(result.project.skills, vec!["skill-a", "skill-b", "skill-c"]);
        assert_eq!(result.project.agents, vec!["claude", "opencode"]);
        assert_eq!(result.project.user_agents, vec!["agent-x", "agent-y"]);
    }

    #[test]
    fn merge_preserves_existing_project_description() {
        let mut project = empty_project();
        project.description = "Existing description".into();

        let tmpl = ProjectTemplate {
            name: "tmpl".into(),
            description: "Template description".into(),
            ..Default::default()
        };

        let result =
            merge_templates_into_project(&mut project, &[tmpl]).expect("merge should succeed");

        assert_eq!(
            result.project.description, "Existing description",
            "existing description should not be overwritten"
        );
    }

    #[test]
    fn merge_sets_description_when_empty() {
        let mut project = empty_project();

        let tmpl = ProjectTemplate {
            name: "tmpl".into(),
            description: "Template description".into(),
            ..Default::default()
        };

        let result =
            merge_templates_into_project(&mut project, &[tmpl]).expect("merge should succeed");

        assert_eq!(result.project.description, "Template description");
    }

    // ── Unified instructions ────────────────────────────────────────────────

    #[test]
    fn merge_collects_unified_instructions() {
        let mut project = empty_project();

        let tmpl = ProjectTemplate {
            name: "tmpl".into(),
            unified_instruction: "Do the thing.".into(),
            unified_rules: vec!["rule-a".into()],
            ..Default::default()
        };

        let result =
            merge_templates_into_project(&mut project, &[tmpl]).expect("merge should succeed");

        assert_eq!(result.project.instruction_mode, "unified");
        assert_eq!(result.pending_unified.len(), 1);
        assert_eq!(result.pending_unified[0].content, "Do the thing.");
        assert_eq!(result.pending_unified[0].rules, vec!["rule-a"]);
    }

    #[test]
    fn merge_skips_empty_unified_instructions() {
        let mut project = empty_project();

        let tmpl = ProjectTemplate {
            name: "tmpl".into(),
            unified_instruction: "   ".into(),
            unified_rules: vec![],
            ..Default::default()
        };

        let result =
            merge_templates_into_project(&mut project, &[tmpl]).expect("merge should succeed");

        // instruction_mode should NOT be changed to "unified"
        assert_ne!(result.project.instruction_mode, "unified");
        assert!(result.pending_unified.is_empty());
    }

    #[test]
    fn merge_does_not_switch_to_unified_for_rules_only_template() {
        // Regression: a template with unified_rules but no unified_instruction was
        // previously switching instruction_mode to "unified", which caused the sync
        // engine to overwrite existing per-agent instruction files with empty content.
        let mut project = empty_project();
        project.instruction_mode = "per-agent".to_string();

        let tmpl = ProjectTemplate {
            name: "tmpl".into(),
            unified_rules: vec!["automatic-service".into()],
            ..Default::default()
        };

        let result =
            merge_templates_into_project(&mut project, &[tmpl]).expect("merge should succeed");

        assert_ne!(
            result.project.instruction_mode, "unified",
            "rules-only template must not switch instruction_mode to unified"
        );
        // Rules are still returned in pending_unified so the frontend can persist them.
        assert_eq!(result.pending_unified.len(), 1);
        assert_eq!(result.pending_unified[0].rules, vec!["automatic-service"]);
        assert_eq!(result.pending_unified[0].content, "");
    }

    #[test]
    fn merge_collects_unified_from_multiple_templates() {
        let mut project = empty_project();

        let tmpl1 = ProjectTemplate {
            name: "tmpl1".into(),
            unified_instruction: "First instruction.".into(),
            ..Default::default()
        };
        let tmpl2 = ProjectTemplate {
            name: "tmpl2".into(),
            unified_rules: vec!["rule-b".into()],
            ..Default::default()
        };

        let result = merge_templates_into_project(&mut project, &[tmpl1, tmpl2])
            .expect("merge should succeed");

        assert_eq!(result.pending_unified.len(), 2);
        assert_eq!(result.pending_unified[0].content, "First instruction.");
        assert_eq!(result.pending_unified[1].rules, vec!["rule-b"]);
    }

    // ── Project files ───────────────────────────────────────────────────────

    #[test]
    fn merge_writes_project_files_to_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut project = empty_project();
        project.directory = tmp.path().to_str().unwrap().into();

        let tmpl = ProjectTemplate {
            name: "tmpl".into(),
            project_files: vec![
                TemplateProjectFile {
                    filename: "README.md".into(),
                    content: "# Hello".into(),
                },
                TemplateProjectFile {
                    filename: "notes.txt".into(),
                    content: "Some notes".into(),
                },
            ],
            ..Default::default()
        };

        merge_templates_into_project(&mut project, &[tmpl]).expect("merge should succeed");

        let readme = fs::read_to_string(tmp.path().join("README.md")).expect("read README");
        assert_eq!(readme, "# Hello");

        let notes = fs::read_to_string(tmp.path().join("notes.txt")).expect("read notes");
        assert_eq!(notes, "Some notes");
    }

    #[test]
    fn merge_skips_project_files_when_no_directory() {
        let mut project = empty_project();
        // directory is empty — project files should be silently skipped

        let tmpl = ProjectTemplate {
            name: "tmpl".into(),
            project_files: vec![TemplateProjectFile {
                filename: "README.md".into(),
                content: "# Hello".into(),
            }],
            ..Default::default()
        };

        // Should not error even though there's no directory to write to
        merge_templates_into_project(&mut project, &[tmpl]).expect("merge should succeed");
    }

    #[test]
    fn merge_skips_empty_project_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut project = empty_project();
        project.directory = tmp.path().to_str().unwrap().into();

        let tmpl = ProjectTemplate {
            name: "tmpl".into(),
            project_files: vec![
                TemplateProjectFile {
                    filename: "".into(),
                    content: "should not write".into(),
                },
                TemplateProjectFile {
                    filename: "empty.txt".into(),
                    content: "".into(),
                },
            ],
            ..Default::default()
        };

        merge_templates_into_project(&mut project, &[tmpl]).expect("merge should succeed");

        // Neither file should have been written
        assert!(!tmp.path().join("empty.txt").exists());
    }

    // ── union_vec ───────────────────────────────────────────────────────────

    #[test]
    fn union_vec_appends_new_items() {
        let mut target = vec!["a".into(), "b".into()];
        union_vec(&mut target, &["c".into(), "d".into()]);
        assert_eq!(target, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn union_vec_skips_duplicates() {
        let mut target = vec!["a".into(), "b".into()];
        union_vec(&mut target, &["b".into(), "c".into(), "a".into()]);
        assert_eq!(target, vec!["a", "b", "c"]);
    }

    #[test]
    fn union_vec_handles_empty_source() {
        let mut target = vec!["a".into()];
        union_vec(&mut target, &[]);
        assert_eq!(target, vec!["a"]);
    }

    #[test]
    fn union_vec_handles_empty_target() {
        let mut target: Vec<String> = vec![];
        union_vec(&mut target, &["a".into(), "b".into()]);
        assert_eq!(target, vec!["a", "b"]);
    }

    // ── Edge cases ──────────────────────────────────────────────────────────

    #[test]
    fn merge_with_no_templates_returns_unchanged_project() {
        let mut project = empty_project();
        project.skills = vec!["skill-a".into()];

        let result = merge_templates_into_project(&mut project, &[]).expect("merge should succeed");

        assert_eq!(result.project.skills, vec!["skill-a"]);
        assert!(result.pending_unified.is_empty());
    }

    #[test]
    fn merge_with_empty_template_is_noop() {
        let mut project = empty_project();
        project.skills = vec!["existing".into()];
        project.agents = vec!["claude".into()];

        let tmpl = make_template("empty");

        let result =
            merge_templates_into_project(&mut project, &[tmpl]).expect("merge should succeed");

        assert_eq!(result.project.skills, vec!["existing"]);
        assert_eq!(result.project.agents, vec!["claude"]);
    }
}
