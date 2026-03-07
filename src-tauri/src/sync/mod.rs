mod autodetect;
mod cleanup;
pub mod drift;
mod engine;
mod helpers;
mod local_skills;

// Re-export the public API so callers can use `sync::function_name` as before.
pub use autodetect::autodetect_project_dependencies;
pub use cleanup::{get_agent_cleanup_preview, remove_agent_from_project};
pub use drift::{
    check_project_drift, collect_instruction_conflicts_pub, AgentDrift, DriftReport, DriftedFile,
    InstructionFileConflict,
};
pub use engine::{discover_new_agent_mcp_configs, sync_project, sync_project_without_autodetect};
pub use local_skills::{
    import_local_skill, read_local_skill, save_local_skill, sync_local_skills_across_agents,
};
