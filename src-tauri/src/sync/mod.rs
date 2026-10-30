mod autodetect;
mod cleanup;
pub mod drift;
mod engine;
pub mod global_mcp;
mod helpers;
mod rebuild;

// Re-export the public API so callers can use `sync::function_name` as before.
pub use autodetect::autodetect_project_dependencies;
pub use cleanup::{get_agent_cleanup_preview, remove_agent_from_project};
pub use drift::{
    check_project_drift, check_project_problems, collect_instruction_conflicts_pub,
    mcp_config_sync_notes, AgentDrift,
    DriftReport, DriftedFile, InstructionFileConflict, ProjectProblem, ProjectProblemKind,
    ProjectProblemsReport,
};
pub use helpers::{
    extract_agent_machine_name as extract_agent_machine_name_pub, CustomAssetConflict,
    CustomAssetKind, McpConfigSyncNotes, McpServerNote,
};
// Contained project reads and writes, shared with the agent writers and the
// core instruction and rule writers.
pub(crate) use helpers::{
    create_project_dir, escaped_silent_dir, project_write_refusal, read_project_text,
    write_project_file, write_project_file_or_skip,
};
// Adopting a custom agent resolves the conflict the scan reported, and turns
// the agent-native file back into the Markdown the project stores.
pub(crate) use helpers::{scan_custom_agent, CustomAgentFinding};
pub(crate) use rebuild::agent_file_to_markdown;

/// Force-write custom agents (empty skip set). Used by overwrite resolution.
pub fn sync_custom_agents_force(
    root: &std::path::Path,
    agents_dir: &std::path::Path,
    custom_agents: &[crate::core::CustomAgent],
    agent: &dyn crate::agent::Agent,
) -> Result<Vec<String>, String> {
    helpers::sync_custom_agents(
        root,
        agents_dir,
        custom_agents,
        agent,
        &std::collections::HashSet::new(),
    )
}
pub use engine::{
    discover_new_agent_mcp_configs, sync_new_project_from_existing_files, sync_project,
    sync_project_without_autodetect, sync_to_directory,
};
pub use rebuild::{rebuild_instruction_snapshots, rebuild_project_state};
