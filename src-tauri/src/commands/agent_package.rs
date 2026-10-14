use std::collections::BTreeMap;

use serde::Serialize;

use crate::core;

use super::projects::{
    sync_projects_referencing_mcp_server, sync_projects_referencing_skill, with_each_project_mut,
};

// ── Agent Plugin packages ────────────────────────────────────────────────────
//
// Each command reads files, so each runs on a blocking thread and never on
// the main thread.

/// What an import would add, shown to the user before they confirm it.
#[derive(Debug, Clone, Serialize)]
pub struct AgentPackagePreview {
    pub package: core::AgentPackage,
    /// For each MCP server name in the package, the projects that already
    /// reference it. Importing the server writes it into those projects.
    pub server_projects: BTreeMap<String, Vec<String>>,
    /// The same for each skill name.
    pub skill_projects: BTreeMap<String, Vec<String>>,
}

/// True when the folder has a `plugin.json` at its root, so the import
/// dialog should treat it as an Agent Plugin package.
#[tauri::command]
pub async fn is_agent_package_dir(path: String) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || core::is_agent_package_dir(&path))
        .await
        .map_err(|e| format!("is_agent_package_dir task join error: {e}"))
}

/// Read a package without changing anything, for the confirm step.
#[tauri::command]
pub async fn preview_agent_package(path: String) -> Result<AgentPackagePreview, String> {
    tokio::task::spawn_blocking(move || {
        let package = core::read_agent_package(&path)?;
        let (skill_projects, server_projects) = referencing_projects(&package);
        Ok(AgentPackagePreview {
            package,
            server_projects,
            skill_projects,
        })
    })
    .await
    .map_err(|e| format!("preview_agent_package task join error: {e}"))?
}

/// Import an Agent Plugin package's skills and MCP servers into the library.
/// Only new names are added. Projects that already referenced one of those
/// names are synced, as for any other library save; the preview lists them
/// so the user agrees to that before importing.
#[tauri::command]
pub async fn import_agent_package(path: String) -> Result<core::AgentPackageImportResult, String> {
    tokio::task::spawn_blocking(move || {
        let result = core::import_agent_package(&path)?;
        for name in &result.imported_skills {
            core::record_recently_added("skills", name);
            sync_projects_referencing_skill(name);
        }
        for name in &result.imported_mcp_servers {
            sync_projects_referencing_mcp_server(name);
        }
        Ok(result)
    })
    .await
    .map_err(|e| format!("import_agent_package task join error: {e}"))?
}

/// List the Agent Plugin packages installed in `~/.agents/plugins`.
#[tauri::command]
pub async fn list_installed_agent_packages() -> Result<core::InstalledAgentPackages, String> {
    tokio::task::spawn_blocking(core::list_installed_agent_packages)
        .await
        .map_err(|e| format!("list_installed_agent_packages task join error: {e}"))?
}

type ProjectsByName = BTreeMap<String, Vec<String>>;

/// The projects naming each of the package's skills and servers, matched
/// exactly as the post-import sync matches them.
fn referencing_projects(package: &core::AgentPackage) -> (ProjectsByName, ProjectsByName) {
    let mut skills = ProjectsByName::new();
    let mut servers = ProjectsByName::new();
    with_each_project_mut(|_, project| {
        for skill in &package.skills {
            if project.skills.iter().any(|name| name == &skill.name) {
                skills
                    .entry(skill.name.clone())
                    .or_default()
                    .push(project.name.clone());
            }
        }
        for server in &package.mcp_servers {
            if project.mcp_servers.iter().any(|name| name == &server.name) {
                servers
                    .entry(server.name.clone())
                    .or_default()
                    .push(project.name.clone());
            }
        }
    });
    (skills, servers)
}
