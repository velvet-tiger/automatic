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
pub use engine::{
    discover_new_agent_mcp_configs, sync_project, sync_project_without_autodetect,
    sync_to_directory,
};
pub use rebuild::{rebuild_instruction_snapshots, rebuild_project_state};
