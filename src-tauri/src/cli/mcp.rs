//! `automatic mcp ...` — list registered MCP servers.

use super::output::{emit, OutputOptions};
use super::{CliError, McpAction};
use crate::core;

pub fn dispatch(action: McpAction, opts: OutputOptions) -> Result<(), CliError> {
    match action {
        McpAction::List => list(opts),
    }
}

fn list(opts: OutputOptions) -> Result<(), CliError> {
    let names = core::list_mcp_server_configs().map_err(CliError::from)?;
    emit(opts, &names, || {
        if names.is_empty() {
            "No MCP servers registered.".to_string()
        } else {
            names.join("\n")
        }
    })
    .map_err(CliError::Io)
}
