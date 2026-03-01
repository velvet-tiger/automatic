use std::path::PathBuf;

// ── Path Helpers ─────────────────────────────────────────────────────────────

/// Returns the root Automatic data directory.
///
/// - **Debug builds** (`cargo tauri dev`, `cargo test`, etc.): `~/.automatic-dev`
/// - **Release builds**: `~/.automatic`
///
/// All other path helpers call this function so that dev and production data
/// are always kept separate.
pub fn get_automatic_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    #[cfg(debug_assertions)]
    let dir = home.join(".automatic-dev");
    #[cfg(not(debug_assertions))]
    let dir = home.join(".automatic");
    Ok(dir)
}

/// Primary skills directory — the agentskills.io standard location.
pub fn get_agents_skills_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".agents/skills"))
}

/// Secondary skills directory — Claude Code's location.
pub fn get_claude_skills_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".claude/skills"))
}

pub fn get_projects_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("projects"))
}

pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && name != "." && name != ".."
}
