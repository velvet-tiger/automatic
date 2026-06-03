//! Command-line interface for Automatic.
//!
//! This module exposes a small read-mostly CLI over the same `core::*`
//! business logic the GUI and MCP server use. It is invoked from `main.rs`
//! when the first argv after the program name matches a known CLI verb;
//! otherwise the GUI launches as before.
//!
//! Dispatch lives here (not in `main.rs`) so that argument parsing, output
//! formatting, and exit-code mapping are all in one place.

use clap::{Parser, Subcommand};

pub mod output;

use output::OutputOptions;

/// Top-level `automatic` CLI. `clap` produces `--help` automatically.
#[derive(Debug, Parser)]
#[command(
    name = "automatic",
    bin_name = "automatic",
    about = "Manage Automatic from the terminal",
    disable_help_subcommand = true,
    version
)]
pub struct Cli {
    /// Emit machine-readable JSON. Matches the shapes returned by the MCP
    /// server so the same scripts work against either surface.
    #[arg(long, global = true)]
    pub json: bool,

    /// Suppress non-essential human output. Has no effect when `--json` is
    /// set.
    #[arg(long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Inspect and sync projects managed by Automatic.
    Projects {
        #[command(subcommand)]
        action: ProjectsAction,
    },
    /// List and read skills from the library and external sources.
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },
    /// Inspect registered MCP servers.
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
    /// Read and write project-scoped memory entries.
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
    /// Inspect rules.
    Rules {
        #[command(subcommand)]
        action: RulesAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProjectsAction {
    /// List every project name known to Automatic.
    List,
    /// Print the full project config (registry + on-disk merged form).
    Show { name: String },
    /// Sync the named project to its configured directory.
    Sync { name: String },
}

#[derive(Debug, Subcommand)]
pub enum SkillsAction {
    /// List skills discovered across every skill source.
    List,
    /// Print the raw SKILL.md content for a skill.
    Show { name: String },
    /// Filter skill names by a case-insensitive substring.
    Search { query: String },
}

#[derive(Debug, Subcommand)]
pub enum McpAction {
    /// List MCP server config names registered with Automatic.
    List,
}

#[derive(Debug, Subcommand)]
pub enum MemoryAction {
    /// List memory keys for a project, optionally filtered by a glob pattern.
    List {
        /// Project name whose memories to list.
        project: String,
        /// Optional glob pattern (e.g. `conventions/*`).
        pattern: Option<String>,
    },
    /// Print the value for a single memory key.
    Get {
        project: String,
        key: String,
    },
    /// Write or overwrite a memory entry. The source defaults to `cli`.
    Set {
        project: String,
        key: String,
        value: String,
        /// Origin tag stored alongside the entry.
        #[arg(long, default_value = "cli")]
        source: String,
    },
    /// Substring search across memory keys and values.
    Search {
        project: String,
        query: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum RulesAction {
    /// List rule machine names in the library.
    List,
    /// Print the body of a rule by machine name.
    Show { machine_name: String },
}

/// Parse argv and dispatch to the matching command group.
///
/// Returns the process exit code. Errors are printed to stderr; usage
/// errors are handled by clap directly via `parse_from`.
pub fn run(argv: Vec<String>) -> i32 {
    let cli = match Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(err) => {
            // clap emits its own formatted error / help text and assigns
            // an exit code (2 for usage errors, 0 for --help/--version).
            err.exit();
        }
    };

    let opts = OutputOptions {
        json: cli.json,
        quiet: cli.quiet,
    };

    let result = match cli.command {
        Command::Projects { action } => projects::dispatch(action, opts),
        Command::Skills { action } => skills::dispatch(action, opts),
        Command::Mcp { action } => mcp::dispatch(action, opts),
        Command::Memory { action } => memory::dispatch(action, opts),
        Command::Rules { action } => rules::dispatch(action, opts),
    };

    match result {
        Ok(()) => 0,
        Err(CliError::NotFound(msg)) => {
            eprintln!("not found: {}", msg);
            1
        }
        Err(CliError::Usage(msg)) => {
            eprintln!("usage error: {}", msg);
            2
        }
        Err(CliError::Io(msg)) => {
            eprintln!("error: {}", msg);
            3
        }
    }
}

/// Returns true when `verb` is a CLI subcommand or a clap meta-flag that
/// should be handled by the CLI rather than launching the GUI.
///
/// `main.rs` checks this before falling through to `tauri::Builder` so that
/// `automatic --help`, `automatic -V`, and any of the documented verbs are
/// dispatched to `run`.
pub fn is_cli_verb(verb: &str) -> bool {
    matches!(
        verb,
        "projects"
            | "skills"
            | "mcp"
            | "memory"
            | "rules"
            | "--help"
            | "-h"
            | "help"
            | "--version"
            | "-V"
    )
}

/// Error categories that map onto distinct process exit codes. Command
/// handlers map raw `core::*` `String` errors into one of these variants so
/// that exit codes are consistent across the CLI surface.
#[derive(Debug)]
pub enum CliError {
    NotFound(String),
    Usage(String),
    Io(String),
}

impl From<String> for CliError {
    fn from(value: String) -> Self {
        // Heuristic: many `core::*` errors include "not found" — surface
        // those as exit code 1 so scripts can distinguish missing entries
        // from genuine failures. Everything else falls back to `Io`.
        let lower = value.to_ascii_lowercase();
        if lower.contains("not found") {
            CliError::NotFound(value)
        } else {
            CliError::Io(value)
        }
    }
}

mod memory;
mod mcp;
mod projects;
mod rules;
mod skills;

#[cfg(test)]
mod tests {
    use super::*;

    /// `is_cli_verb` is the gate `main.rs` uses to decide whether to dispatch
    /// to the CLI or launch the GUI. Drift here would silently break the
    /// help / version flags and any documented verb, so pin every known
    /// entry to a test.
    #[test]
    fn is_cli_verb_accepts_known_verbs_and_meta_flags() {
        for verb in [
            "projects",
            "skills",
            "mcp",
            "memory",
            "rules",
            "--help",
            "-h",
            "help",
            "--version",
            "-V",
        ] {
            assert!(is_cli_verb(verb), "expected `{}` to be a CLI verb", verb);
        }
    }

    #[test]
    fn is_cli_verb_rejects_unknown_input() {
        for verb in ["", "mcp-serve", "mcp-proxy", "gui", "open"] {
            assert!(
                !is_cli_verb(verb),
                "expected `{}` to NOT be a CLI verb (would shadow GUI / MCP launch)",
                verb
            );
        }
    }

    #[test]
    fn parses_projects_list_with_global_json_flag() {
        let cli = Cli::try_parse_from(["automatic", "--json", "projects", "list"]).unwrap();
        assert!(cli.json);
        assert!(!cli.quiet);
        assert!(matches!(cli.command, Command::Projects { action: ProjectsAction::List }));
    }

    #[test]
    fn parses_projects_show_with_name() {
        let cli = Cli::try_parse_from(["automatic", "projects", "show", "demo"]).unwrap();
        match cli.command {
            Command::Projects {
                action: ProjectsAction::Show { name },
            } => assert_eq!(name, "demo"),
            other => panic!("unexpected command: {:?}", other),
        }
    }

    #[test]
    fn parses_memory_set_with_default_source() {
        let cli =
            Cli::try_parse_from(["automatic", "memory", "set", "proj", "k", "v"]).unwrap();
        match cli.command {
            Command::Memory {
                action:
                    MemoryAction::Set {
                        project,
                        key,
                        value,
                        source,
                    },
            } => {
                assert_eq!(project, "proj");
                assert_eq!(key, "k");
                assert_eq!(value, "v");
                assert_eq!(source, "cli");
            }
            other => panic!("unexpected command: {:?}", other),
        }
    }

    #[test]
    fn rejects_unknown_subcommand() {
        let err = Cli::try_parse_from(["automatic", "doesnotexist"]).unwrap_err();
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::InvalidSubcommand,
            "expected a usage error for unknown verbs"
        );
    }

    /// `CliError::from(String)` heuristically routes "not found" errors to
    /// exit code 1. Other errors fall through to `Io` (exit 3). This is
    /// what scripts rely on to distinguish missing entries.
    #[test]
    fn error_routing_recognises_not_found() {
        let err: CliError = "Project 'x' not found".to_string().into();
        assert!(matches!(err, CliError::NotFound(_)));

        let err: CliError = "Permission denied writing config".to_string().into();
        assert!(matches!(err, CliError::Io(_)));
    }
}
