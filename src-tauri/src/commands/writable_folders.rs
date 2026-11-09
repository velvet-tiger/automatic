//! Commands for the folders Claude Code may edit outside a project. The
//! logic is in `core::claude_dirs`.

use std::collections::HashSet;

use crate::core;

use super::project_sweeps::{queue_sweep, SweepProgress};

/// Bring the writable folders of each project in `idents` in step, read-only
/// projects included: their entries go to user settings and their folders are
/// never written. Unlike the sync sweeps, this writes nothing else.
pub(crate) fn refresh_writable_folders(idents: &[String], progress: &SweepProgress) {
    let mut seen = HashSet::new();
    let targets: Vec<core::Project> = idents
        .iter()
        .filter(|ident| seen.insert(ident.as_str()))
        .filter_map(|ident| match read_parsed(ident) {
            Ok(project) if core::writable_folders_apply(&project) => Some(project),
            Ok(_) => None,
            Err(e) => {
                eprintln!("Skipping the writable folders of '{}': {}", ident, e);
                None
            }
        })
        .collect();
    progress.add_total(targets.len());
    for project in targets {
        let outcome = core::sync_writable_folders(&project).and_then(|sync| {
            if sync.errors.is_empty() {
                Ok(())
            } else {
                Err(sync.errors.join("; "))
            }
        });
        progress.project_finished(&project.name, outcome);
    }
}

/// [`refresh_writable_folders`] for every project, queued.
fn queue_refresh_all(label: &str) {
    queue_sweep(None, label.to_string(), |progress| {
        match core::list_project_idents() {
            Ok(idents) => refresh_writable_folders(&idents, progress),
            Err(e) => progress.report_failure("Project list", format!("Could not list projects: {}", e)),
        }
    });
}

fn read_parsed(ident: &str) -> Result<core::Project, String> {
    let raw = core::read_project(ident)?;
    serde_json::from_str(&raw).map_err(|e| format!("Invalid project data: {}", e))
}

/// Turn "Let agents edit across this group" on or off, then bring every
/// member's folders in step.
#[tauri::command(async)]
pub fn set_group_agents_edit_across_group(group_name: &str, enabled: bool) -> Result<(), String> {
    if core::set_group_agents_edit(group_name, enabled)? {
        let members = core::group_member_idents(group_name);
        queue_sweep(None, format!("group {group_name}"), move |progress| {
            refresh_writable_folders(&members, progress)
        });
    }
    Ok(())
}

/// Replace a project's "Other folders Claude can edit", then bring its
/// settings in step. Each folder must be an absolute path.
#[tauri::command(async)]
pub fn set_project_writable_folders(
    name: &str,
    folders: Vec<String>,
) -> Result<core::WritableFoldersSync, String> {
    let ident = core::canonical_project_ident(name)?;
    core::set_project_writable_folders(&ident, &folders)?;
    core::sync_writable_folders(&read_parsed(&ident)?)
}

/// "Not now" on the notice that asks whether user settings may hold this
/// project's folders. Hides it for this checkout only.
#[tauri::command]
pub fn set_writable_folders_notice_hidden(name: &str, hidden: bool) -> Result<(), String> {
    let ident = core::canonical_project_ident(name)?;
    core::set_writable_folders_notice_hidden(&ident, hidden)
}

/// Choose where every project's folders are saved, then bring them in step.
/// Choosing user settings also records the user's agreement to them.
#[tauri::command(async)]
pub fn set_writable_folders_target(target: core::WritableFoldersTarget) -> Result<(), String> {
    core::set_writable_folders_target(target)?;
    queue_refresh_all("folders Claude Code may edit");
    Ok(())
}

/// Record whether Claude Code's user settings may hold project folders
/// ("Save for all my sessions"), then bring every project in step.
#[tauri::command(async)]
pub fn set_writable_folders_user_consent(consent: bool) -> Result<(), String> {
    core::set_writable_folders_user_consent(consent)?;
    queue_refresh_all("folders Claude Code may edit");
    Ok(())
}

/// Bring one project's folders in step now. Works for a read-only project,
/// whose folder is never written.
#[tauri::command(async)]
pub fn sync_project_writable_folders(name: &str) -> Result<core::WritableFoldersSync, String> {
    let ident = core::canonical_project_ident(name)?;
    core::sync_writable_folders(&read_parsed(&ident)?)
}

/// The folders a project wants, where each comes from, and where they are
/// saved. `None` for a project that does not use Claude Code. Reads only.
#[tauri::command]
pub fn read_project_writable_folders(
    name: &str,
) -> Result<Option<core::WritableFoldersFindings>, String> {
    let ident = core::canonical_project_ident(name)?;
    core::writable_folders_findings(&read_parsed(&ident)?)
}
