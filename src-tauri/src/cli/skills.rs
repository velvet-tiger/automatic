//! `automatic skills ...` — list, show, search.

use super::output::{emit, emit_raw_json, OutputOptions};
use super::{CliError, SkillsAction};
use crate::core;

pub fn dispatch(action: SkillsAction, opts: OutputOptions) -> Result<(), CliError> {
    match action {
        SkillsAction::List => list(opts),
        SkillsAction::Show { name } => show(&name, opts),
        SkillsAction::Search { query } => search(&query, opts),
    }
}

fn list(opts: OutputOptions) -> Result<(), CliError> {
    let entries = core::list_skills().map_err(CliError::from)?;
    emit(opts, &entries, || human_list(&entries)).map_err(CliError::Io)
}

fn show(name: &str, opts: OutputOptions) -> Result<(), CliError> {
    // `read_skill` returns the full SKILL.md content as plain text. In
    // `--json` mode we wrap it as `{ "name", "content" }` so downstream
    // tools get structured output without parsing markdown.
    let content = core::read_skill(name).map_err(CliError::from)?;
    if opts.json {
        let body = serde_json::json!({ "name": name, "content": content });
        let rendered = serde_json::to_string_pretty(&body)
            .map_err(|e| CliError::Io(format!("failed to serialise output: {}", e)))?;
        println!("{}", rendered);
        Ok(())
    } else {
        emit_raw_json(opts, &content, &content).map_err(CliError::Io)
    }
}

fn search(query: &str, opts: OutputOptions) -> Result<(), CliError> {
    let entries = core::list_skills().map_err(CliError::from)?;
    let needle = query.to_lowercase();
    let filtered: Vec<_> = entries
        .into_iter()
        .filter(|entry| entry.name.to_lowercase().contains(&needle))
        .collect();

    if filtered.is_empty() && !opts.json {
        return Err(CliError::NotFound(format!("no skills matched '{}'", query)));
    }
    emit(opts, &filtered, || human_list(&filtered)).map_err(CliError::Io)
}

fn human_list(entries: &[core::SkillEntry]) -> String {
    if entries.is_empty() {
        return "No skills.".to_string();
    }
    let mut out = String::new();
    for entry in entries {
        let sources = if entry.sources.is_empty() {
            "-".to_string()
        } else {
            entry.sources.join(",")
        };
        out.push_str(&format!("{:<32}  [{}]\n", entry.name, sources));
    }
    out.trim_end().to_string()
}
