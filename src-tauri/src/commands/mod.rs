mod activity;
mod ai;
mod features;
mod app_plugins;
mod credentials;
mod flags;
mod profile;
mod recommendations;
mod settings;
mod skills;
mod skill_store;
mod rules;
mod task_log;
mod templates;
mod projects;
mod project_files;
mod mcp_servers;
mod agents;
mod memory;
mod misc;
mod oauth;
mod tokens;
mod tools;

pub use activity::*;
pub use ai::*;
pub use features::*;
pub use app_plugins::*;
pub use credentials::*;
pub use flags::*;
pub use profile::*;
pub use recommendations::*;
pub use settings::*;
pub use skills::*;
pub use skill_store::*;
pub use rules::*;
pub use task_log::*;
pub use templates::*;
pub use projects::*;
pub use project_files::*;
pub use mcp_servers::*;
pub use agents::*;
pub use memory::*;
pub use misc::*;
pub use oauth::*;
pub use tokens::*;
pub use tools::*;

// ── Plugin dispatch ───────────────────────────────────────────────────────────
// All plugin commands flow through the single `invoke_tool_command` dispatcher
// defined in tools.rs.  No individual plugin command name appears here or in
// lib.rs.  The dispatch table in tools.rs maps tool names to plugin dispatch
// functions; plugin folders are entirely self-contained.
