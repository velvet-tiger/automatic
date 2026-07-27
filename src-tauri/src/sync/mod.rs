mod autodetect;
mod cleanup;
pub mod drift;
mod engine;
mod helpers;
mod rebuild;

// Re-export the public API so callers can use `sync::function_name` as before.
pub use autodetect::autodetect_project_dependencies;
pub use cleanup::{get_agent_cleanup_preview, remove_agent_from_project};
pub use drift::{
    check_project_drift, check_project_problems, collect_instruction_conflicts_pub, AgentDrift,
    DriftReport, DriftedFile, InstructionFileConflict, ProjectProblem, ProjectProblemKind,
    ProjectProblemsReport,
};
pub use helpers::{
    extract_agent_machine_name as extract_agent_machine_name_pub, CustomAssetConflict,
    CustomAssetKind,
};

/// Force-write custom agents (empty skip set). Used by overwrite resolution.
pub fn sync_custom_agents_force(
    agents_dir: &std::path::Path,
    custom_agents: &[crate::core::CustomAgent],
    agent: &dyn crate::agent::Agent,
) -> Result<Vec<String>, String> {
    helpers::sync_custom_agents(
        agents_dir,
        custom_agents,
        agent,
        &std::collections::HashSet::new(),
    )
}
pub use engine::{
    discover_new_agent_mcp_configs, sync_project, sync_project_without_autodetect,
    sync_to_directory,
};
pub use rebuild::{rebuild_instruction_snapshots, rebuild_project_state};
