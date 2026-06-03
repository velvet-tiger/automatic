//! `automatic rules ...` — list and show rules.

use super::output::{emit, emit_raw_json, OutputOptions};
use super::{CliError, RulesAction};
use crate::core;

pub fn dispatch(action: RulesAction, opts: OutputOptions) -> Result<(), CliError> {
    match action {
        RulesAction::List => list(opts),
        RulesAction::Show { machine_name } => show(&machine_name, opts),
    }
}

fn list(opts: OutputOptions) -> Result<(), CliError> {
    let entries = core::list_rules().map_err(CliError::from)?;
    emit(opts, &entries, || {
        if entries.is_empty() {
            "No rules.".to_string()
        } else {
            entries
                .iter()
                .map(|r| format!("{:<32}  {}", r.id, r.name))
                .collect::<Vec<_>>()
                .join("\n")
        }
    })
    .map_err(CliError::Io)
}

fn show(machine_name: &str, opts: OutputOptions) -> Result<(), CliError> {
    let raw = core::read_rule(machine_name).map_err(CliError::from)?;
    emit_raw_json(opts, &raw, &raw).map_err(CliError::Io)
}
