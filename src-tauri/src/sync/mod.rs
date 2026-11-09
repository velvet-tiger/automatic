mod agent_adopt;
mod autodetect;
mod cleanup;
pub mod drift;
mod engine;
pub mod global_mcp;
mod helpers;
mod rebuild;
mod worktrees;

// Re-export the public API so callers can use `sync::function_name` as before.
pub use autodetect::autodetect_project_dependencies;
pub use cleanup::{get_agent_cleanup_preview, remove_agent_from_project};
pub use drift::{
    check_project_drift, check_project_problems, collect_instruction_conflicts_pub,
    mcp_config_sync_notes, project_sync_warnings, AgentDrift, ProjectSyncWarnings,
    DriftReport, DriftedFile, InstructionFileConflict, ProjectProblem, ProjectProblemKind,
    ProjectProblemsReport, WritableFoldersTargetNotice,
};
pub use helpers::{CustomAssetConflict, CustomAssetKind, McpConfigSyncNotes, McpServerNote};
// Contained project reads and writes, shared with the agent writers and the
// core instruction and rule writers.
pub(crate) use helpers::{
    create_project_dir, escaped_silent_dir, project_write_refusal, read_project_text,
    remove_dropped_agent_files, write_project_file, write_project_file_or_skip,
};
// Adopting a custom agent resolves the conflict the scan reported.
pub(crate) use agent_adopt::{
    adopted_agent_markdown, other_agent_files_matching, rewrite_other_agent_files,
    unwritable_other_agent_files, OtherAgentFile,
};
pub(crate) use helpers::{scan_custom_agent, CustomAgentFinding};

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
pub use worktrees::{sync_linked_worktree, WorktreeSync};
pub(crate) use worktrees::{linked_worktrees, main_checkout_of};
pub use rebuild::{
    rebuild_instruction_snapshots, rebuild_project_state, rebuild_project_with_refusals,
};
