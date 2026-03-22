mod activity;
mod agents;
mod ai;
mod app_plugins;
mod credentials;
mod features;
mod flags;
mod groups;
mod mcp_servers;
mod memory;
mod misc;
mod oauth;
mod profile;
mod project_files;
mod projects;
mod recommendations;
mod rules;
mod settings;
mod skill_store;
mod skills;
mod task_log;
mod templates;
mod tokens;
mod tools;
mod user_agents;

pub use activity::*;
pub use agents::*;
pub use ai::*;
pub use app_plugins::*;
pub use credentials::*;
pub use features::*;
pub use flags::*;
pub use groups::*;
pub use mcp_servers::*;
pub use memory::*;
pub use misc::*;
pub use oauth::*;
pub use profile::*;
pub use project_files::*;
pub use projects::*;
pub use recommendations::*;
pub use rules::*;
pub use settings::*;
pub use skill_store::*;
pub use skills::*;
pub use task_log::*;
pub use templates::*;
pub use tokens::*;
pub use tools::*;
pub use user_agents::*;

// ── Plugin dispatch ───────────────────────────────────────────────────────────
// All plugin commands flow through the single `invoke_tool_command` dispatcher
// defined in tools.rs.  No individual plugin command name appears here or in
// lib.rs.  The dispatch table in tools.rs maps tool names to plugin dispatch
// functions; plugin folders are entirely self-contained.
