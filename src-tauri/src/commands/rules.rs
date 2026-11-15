
use crate::core;

use super::project_sweeps::{
    ensure_not_being_deleted, hold_library_writes, queue_sweep, run_delete_and_wait, SweepProgress,
};
use super::projects::{
    prune_rule_from_projects, prune_rule_from_templates, sync_project_if_configured,
    sync_projects_referencing_rule, with_each_project_mut,
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
    ensure_not_being_deleted(&format!("rule:{machine_name}"), name)?;
    core::user_writes::save_rule(machine_name, name, content)?;
    // Propagate to every project that references this rule so on-disk
    // instruction files stay in sync with the library content. Sync output
    // directories are excluded from the Vite dev watcher so this does not
    // trigger a frontend reload during `make dev`.
    let rule = machine_name.to_string();
    queue_sweep(
        Some(format!("rule:{rule}")),
        format!("rule {rule}"),
        move |progress| sync_projects_referencing_rule(&rule, progress),
    );
    Ok(())
}

#[tauri::command(async)]
pub fn delete_rule(machine_name: &str) -> Result<(), String> {
    let rule = machine_name.to_string();
    run_delete_and_wait(
        format!("rule:{rule}"),
        format!("deleted rule {rule}"),
        move |progress| {
            core::user_writes::delete_rule(&rule)?;
            prune_rule_references(&rule, progress);
            Ok(())
        },
    )
}

/// Take a deleted rule out of every profile, project and template, and
/// re-sync the projects that changed. Run after the rule is deleted.
pub(crate) fn prune_rule_references(machine_name: &str, progress: &SweepProgress) {
    core::prune_asset_from_profiles(core::ProfileResourceKind::Rule, machine_name);
    prune_rule_from_projects(machine_name, progress);
    prune_rule_from_templates(machine_name);
}

/// A project that references a rule, with its current sync status.
#[derive(serde::Serialize)]
pub struct RuleProjectStatus {
    pub name: String,
    /// The project's `local_key`; `None` while it has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_key: Option<String>,
    /// `true` if the on-disk instruction file(s) already contain the current
    /// rule content; `false` if a re-sync is needed.
    pub synced: bool,
    /// `true` when the project is read-only on this machine, so it cannot
    /// be updated whatever `synced` says.
    #[serde(default)]
    pub read_only: bool,
}

/// Whether `project` uses `rule_name`: listed in its `file_rules`, or added
/// by sync (the mandatory rules, and the gitignore rule while the project
/// manages its `.gitignore`).
fn project_uses_rule(project: &core::Project, rule_name: &str) -> bool {
    project
        .file_rules
        .values()
        .any(|rules| rules.iter().any(|r| r == rule_name))
        || crate::sync::project_rule_names(project)
            .iter()
            .any(|r| r == rule_name)
}

/// Return all projects that use `rule_name` together with whether their
/// on-disk rules already match what sync would write.
///
/// `synced` covers all of the project's rules, not only this one.  The
/// Update button runs a full project sync, so a row stays orange until every
/// rule is in place.
#[tauri::command(async)]
pub fn get_projects_referencing_rule(rule_name: &str) -> Result<Vec<RuleProjectStatus>, String> {
    let mut referencing: Vec<RuleProjectStatus> = Vec::new();
    with_each_project_mut(|_, project| {
        if !project_uses_rule(project, rule_name) {
            return;
        }
        referencing.push(RuleProjectStatus {
            name: project.name.clone(),
            local_key: core::local_key_of(project),
            synced: crate::sync::project_rules_current(project),
            read_only: project.is_read_only_here(),
        });
    });
    Ok(referencing)
}

/// Push updated rules to a single project and re-sync its agent configs.
/// Used by the "Update" button on the Rules page.
///
/// The full project sync (which correctly routes rules to `.claude/rules/`
/// or inline depending on the project's agent options) handles the actual
/// write — no manual injection is needed here.
#[tauri::command(async)]
pub fn sync_rule_to_project(rule_name: &str, project_name: &str) -> Result<(), String> {
    let _writes = hold_library_writes();
    let project_name = &crate::core::canonical_project_ident(project_name)?;
    super::projects::ensure_writable(project_name)?;
    let raw = core::read_project(project_name)?;
    let mut project: core::Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))?;

    if !project_uses_rule(&project, rule_name) {
        return Err(format!(
            "Project '{}' does not reference rule '{}'",
            project.name, rule_name
        ));
    }

    sync_project_if_configured(project_name, &mut project);
    Ok(())
}
