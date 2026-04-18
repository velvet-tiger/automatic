mod account;
mod activity;
mod agents;
mod ai;
mod app_plugins;
mod cloud_sync;
mod community;
mod credentials;
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
mod user_commands;
mod remote_sources;
mod whats_new;

pub use crate::plugins::build::commands::*;
pub use account::*;
pub use activity::*;
pub use community::*;
pub use agents::*;
pub use ai::*;
pub use app_plugins::*;
pub use cloud_sync::*;
pub use credentials::*;
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
pub use user_commands::*;
pub use remote_sources::*;
pub use whats_new::*;

// ── Plugin dispatch ───────────────────────────────────────────────────────────
// All plugin commands flow through the single `invoke_tool_command` dispatcher
// defined in tools.rs.  No individual plugin command name appears here or in
// lib.rs.  The dispatch table in tools.rs maps tool names to plugin dispatch
// functions; plugin folders are entirely self-contained.
