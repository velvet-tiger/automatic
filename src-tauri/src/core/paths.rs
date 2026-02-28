use std::path::PathBuf;

// ── Path Helpers ─────────────────────────────────────────────────────────────

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
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".automatic/projects"))
}

pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && name != "." && name != ".."
}
