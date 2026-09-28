//! `automatic projects ...` — list, show, sync.

use super::output::{emit, emit_raw_json, emit_status, OutputOptions};
use super::{CliError, ProjectsAction};
use crate::core;
use crate::sync;

pub fn dispatch(action: ProjectsAction, opts: OutputOptions) -> Result<(), CliError> {
    match action {
        ProjectsAction::List => list(opts),
        // `name` may be a project name or a `local_key`; the handlers work
        // with the project's unique identifier from here on. A name two
        // projects share is an error listing both.
        ProjectsAction::Show { name } => {
            show(&core::canonical_project_ident(&name).map_err(CliError::from)?, opts)
        }
        ProjectsAction::Sync { name } => {
            sync_project(&core::canonical_project_ident(&name).map_err(CliError::from)?, opts)
        }
    }
}

/// One name per registered checkout. A name that repeats is shown with its
/// folder and `local_key`, so the user can pass the key to `show` or `sync`.
fn list(opts: OutputOptions) -> Result<(), CliError> {
    let summaries = core::get_project_summaries().map_err(CliError::from)?;
    let names: Vec<String> = summaries.iter().map(|s| s.name.clone()).collect();
    emit(opts, &names, || {
        if summaries.is_empty() {
            return "No projects.".to_string();
        }
        summaries
            .iter()
            .map(|s| {
                let repeated = summaries
                    .iter()
                    .filter(|o| o.name.to_lowercase() == s.name.to_lowercase())
                    .count()
                    > 1;
                if repeated {
                    format!("{} — {} ({})", s.name, s.directory, s.local_key)
                } else {
                    s.name.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
    .map_err(CliError::Io)
}

fn show(name: &str, opts: OutputOptions) -> Result<(), CliError> {
    // `core::read_project` already returns a JSON string of a
    // `Project` struct. In `--json` mode we hand that through verbatim; in
    // human mode we parse it back to extract a short summary.
    let raw = core::read_project(name).map_err(CliError::from)?;
    let human = || {
        match serde_json::from_str::<core::Project>(&raw) {
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
    }
    };
    emit_raw_json(opts, &raw, &human()).map_err(CliError::Io)
}

fn sync_project(name: &str, opts: OutputOptions) -> Result<(), CliError> {
    let raw = core::read_project(name).map_err(CliError::from)?;
    let project: core::Project = serde_json::from_str(&raw)
        .map_err(|e| CliError::Io(format!("Invalid project data: {}", e)))?;
    let written = sync::sync_project(&project).map_err(CliError::from)?;

    let count = written.len();
    let message = if project.agents.is_empty() {
        format!("No agents configured for project '{}', nothing synced", project.name)
    } else {
        format!(
            "Synced {} file{} for project '{}'",
            count,
            if count == 1 { "" } else { "s" },
            project.name
        )
    };
    if opts.json {
        emit(opts, &written, || message.clone()).map_err(CliError::Io)
    } else {
        emit_status(opts, "ok", &message).map_err(CliError::Io)
    }
}
