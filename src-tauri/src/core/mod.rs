// Keychain service name — debug builds use a separate entry so "Always Allow"
// only needs to be clicked once per build type, and dev/release entries never
// collide.
#[cfg(debug_assertions)]
pub const KEYCHAIN_SERVICE: &str = "automatic_desktop_dev";
#[cfg(not(debug_assertions))]
pub const KEYCHAIN_SERVICE: &str = "automatic_desktop";

pub mod ai;
mod app_plugins;
mod marketplace;
mod marketplace_data;
mod author;
mod credentials;
mod env_crypto;
mod editors;
mod flags;
mod groups;
mod integrations;
mod mcp_servers;
mod paths;
mod plugins;
mod profile;
mod project_files;
mod project_templates;
mod projects;
mod rules;
mod rules_injection;
mod settings;
mod skill_store;
mod skills;
pub mod task_log;
mod templates;
pub mod tools;
mod types;
mod user_agents;

pub use app_plugins::*;
pub use marketplace::*;
pub use marketplace_data::init_marketplace_files;
pub use author::*;
pub use credentials::*;
pub use flags::*;
pub use editors::*;
pub use groups::*;
pub use integrations::*;
pub use mcp_servers::*;
pub use paths::*;
pub use plugins::*;
pub use profile::*;
pub use project_files::*;
pub use project_templates::*;
pub use projects::*;
pub use rules::*;
pub use rules_injection::*;
pub use settings::*;
pub use skill_store::*;
pub use skills::*;
pub use templates::*;
pub use tools::*;
pub use types::*;
pub use user_agents::*;
