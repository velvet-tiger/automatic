use std::path::PathBuf;

#[cfg(test)]
use std::cell::RefCell;

// ── Path Helpers ─────────────────────────────────────────────────────────────

fn home_dir() -> Result<PathBuf, String> {
    #[cfg(test)]
    if let Some(path) = TEST_HOME_OVERRIDE.with(|override_path| override_path.borrow().clone()) {
        return Ok(path);
    }

    dirs::home_dir().ok_or("Could not find home directory".to_string())
}

/// Returns the root Automatic data directory.
///
/// - **Debug builds** (`cargo tauri dev`, `cargo test`, etc.): `~/.automatic-dev`
/// - **Release builds**: `~/.automatic`
///
/// All other path helpers call this function so that dev and production data
/// are always kept separate.
pub fn get_automatic_dir() -> Result<PathBuf, String> {
    let home = home_dir()?;
    #[cfg(debug_assertions)]
    let dir = home.join(".automatic-dev");
    #[cfg(not(debug_assertions))]
    let dir = home.join(".automatic");
    Ok(dir)
}

/// Primary skills directory — Automatic's managed library.
///
/// This is the only location Automatic writes to. Per-project sync copies
/// skills from here into each project's agent-specific skill directory on
/// demand; nothing is auto-loaded globally.
pub fn get_library_skills_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("library").join("skills"))
}

/// External skill directory — the agentskills.io standard location.
///
/// Read-only for Automatic. Scanned so that skills installed here by other
/// tools (e.g. `npx skills add`) are visible in the UI, but Automatic does
/// not write to this path. Some agents (notably OpenCode) auto-load from
/// this directory globally — anything here applies to every project that
/// uses such agents, which is why Automatic's managed library lives
/// elsewhere.
pub fn get_agents_skills_dir() -> Result<PathBuf, String> {
    let home = home_dir()?;
    Ok(home.join(".agents/skills"))
}

/// External skill directory — Claude Code's location. Read-only for
/// Automatic; same rationale as [`get_agents_skills_dir`].
pub fn get_claude_skills_dir() -> Result<PathBuf, String> {
    let home = home_dir()?;
    Ok(home.join(".claude/skills"))
}

pub fn get_projects_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("projects"))
}

pub fn get_commands_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("commands"))
}

pub fn get_groups_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("groups"))
}

pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && name != "." && name != ".."
}

#[cfg(test)]
thread_local! {
    static TEST_HOME_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn with_test_home<T>(path: PathBuf, test: impl FnOnce() -> T) -> T {
    struct RestoreHome(Option<PathBuf>);

    impl Drop for RestoreHome {
        fn drop(&mut self) {
            TEST_HOME_OVERRIDE.with(|override_path| {
                *override_path.borrow_mut() = self.0.take();
            });
        }
    }

    let previous = TEST_HOME_OVERRIDE.with(|override_path| override_path.replace(Some(path)));
    let _restore = RestoreHome(previous);
    test()
}
