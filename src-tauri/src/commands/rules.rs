use std::path::PathBuf;

use crate::core;

use super::projects::{
    prune_rule_from_projects, sync_project_if_configured, with_each_project_mut,
};

// ── Rules ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_rules() -> Result<Vec<core::RuleEntry>, String> {
    core::list_rules()
}

#[tauri::command]
pub fn read_rule(machine_name: &str) -> Result<String, String> {
    core::read_rule(machine_name)
}

#[tauri::command]
pub fn save_rule(machine_name: &str, name: &str, content: &str) -> Result<(), String> {
    // Only persist the rule to disk.  Do NOT eagerly sync referencing projects
    // here — that writes to project directories, which triggers the Tauri dev
    // watcher and causes a full app reload (losing all frontend state).
    // The user can push updates to projects via the "Update" buttons on the
    // Rules page instead.
    core::save_rule(machine_name, name, content)
}

#[tauri::command]
pub fn delete_rule(machine_name: &str) -> Result<(), String> {
    core::delete_rule(machine_name)?;
    prune_rule_from_projects(machine_name);
    Ok(())
}

/// A project that references a rule, with its current sync status.
#[derive(serde::Serialize)]
pub struct RuleProjectStatus {
    pub name: String,
    /// `true` if the on-disk instruction file(s) already contain the current
    /// rule content; `false` if a re-sync is needed.
    pub synced: bool,
}

/// Return all projects that reference `rule_name` together with whether
/// their on-disk instruction files are already up to date.
#[tauri::command]
pub fn get_projects_referencing_rule(rule_name: &str) -> Result<Vec<RuleProjectStatus>, String> {
    let mut referencing: Vec<RuleProjectStatus> = Vec::new();
    with_each_project_mut(|project_name, project| {
        // Collect the file_rules entries that reference this rule.
        let referencing_entries: Vec<(&String, &Vec<String>)> = project
            .file_rules
            .iter()
            .filter(|(_, rules)| rules.iter().any(|r| r == rule_name))
            .collect();

        if referencing_entries.is_empty() {
            return;
        }

        let dir = PathBuf::from(&project.directory);

        // Resolve each file_rules key to actual on-disk paths.
        // In unified mode, "_unified" maps to every agent's project file.
        let synced = referencing_entries.iter().all(|(key, rules)| {
            let paths: Vec<PathBuf> = if *key == "_unified" {
                let mut seen = std::collections::HashSet::new();
                project
                    .agents
                    .iter()
                    .filter_map(|aid| crate::agent::from_id(aid))
                    .filter(|inst| seen.insert(inst.project_file_name().to_string()))
                    .map(|inst| dir.join(inst.project_file_name()))
                    .collect()
            } else {
                vec![dir.join(key.as_str())]
            };

            // All resolved files must contain the current rules section.
            paths
                .iter()
                .all(|path| core::is_file_rules_current(path, rules).unwrap_or(false))
        });

        referencing.push(RuleProjectStatus {
            name: project_name.to_string(),
            synced,
        });
    });
    Ok(referencing)
}

/// Re-inject the given rule into a single project's instruction files and re-sync
/// its agent configs.  Used by the "Update" button on the Rules page.
#[tauri::command]
pub fn sync_rule_to_project(rule_name: &str, project_name: &str) -> Result<(), String> {
    let raw = core::read_project(project_name)?;
    let project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    let references = project
        .file_rules
        .values()
        .any(|rules| rules.iter().any(|r| r == rule_name));

    if !references {
        return Err(format!(
            "Project '{}' does not reference rule '{}'",
            project_name, rule_name
        ));
    }

    // Re-inject the updated rule content into any files in this project that use it.
    for (filename, rules) in &project.file_rules {
        if rules.iter().any(|r| r == rule_name) {
            core::inject_rules_into_project_file(&project.directory, filename, rules)?;
        }
    }

    sync_project_if_configured(project_name, &project);
    Ok(())
}
