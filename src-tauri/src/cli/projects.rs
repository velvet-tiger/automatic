//! `automatic projects ...` — list, show, sync.

use super::output::{emit, emit_raw_json, emit_status, OutputOptions};
use super::{CliError, ProjectsAction};
use crate::core;
use crate::sync;

pub fn dispatch(action: ProjectsAction, opts: OutputOptions) -> Result<(), CliError> {
    match action {
        ProjectsAction::List => list(opts),
        ProjectsAction::Show { name } => show(&name, opts),
        ProjectsAction::Sync { name } => sync_project(&name, opts),
    }
}

fn list(opts: OutputOptions) -> Result<(), CliError> {
    let names = core::list_projects().map_err(CliError::from)?;
    emit(opts, &names, || {
        if names.is_empty() {
            "No projects.".to_string()
        } else {
            names.join("\n")
        }
    })
    .map_err(CliError::Io)
}

fn show(name: &str, opts: OutputOptions) -> Result<(), CliError> {
    // `core::read_project` already returns a JSON string of a
    // `Project` struct. In `--json` mode we hand that through verbatim; in
    // human mode we parse it back to extract a short summary.
    let raw = core::read_project(name).map_err(CliError::from)?;
    let human = || match serde_json::from_str::<core::Project>(&raw) {
        Ok(project) => format!(
            "Name:       {}\nDirectory:  {}\nSkills:     {}\nMCP:        {}\nAgents:     {}\nUpdated:    {}",
            project.name,
            if project.directory.is_empty() {
                "(unset)".to_string()
            } else {
                project.directory
            },
            project.skills.join(", "),
            project.mcp_servers.join(", "),
            project.agents.join(", "),
            project.updated_at,
        ),
        Err(_) => raw.clone(),
    };
    emit_raw_json(opts, &raw, &human()).map_err(CliError::Io)
}

fn sync_project(name: &str, opts: OutputOptions) -> Result<(), CliError> {
    let raw = core::read_project(name).map_err(CliError::from)?;
    let project: core::Project = serde_json::from_str(&raw)
        .map_err(|e| CliError::Io(format!("Invalid project data: {}", e)))?;
    let written = sync::sync_project(&project).map_err(CliError::from)?;

    let count = written.len();
    let message = format!(
        "Synced {} file{} for project '{}'",
        count,
        if count == 1 { "" } else { "s" },
        name
    );
    if opts.json {
        emit(opts, &written, || message.clone()).map_err(CliError::Io)
    } else {
        emit_status(opts, "ok", &message).map_err(CliError::Io)
    }
}
