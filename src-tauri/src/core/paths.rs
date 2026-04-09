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

/// Primary skills directory — the agentskills.io standard location.
pub fn get_agents_skills_dir() -> Result<PathBuf, String> {
    let home = home_dir()?;
    Ok(home.join(".agents/skills"))
}

/// Secondary skills directory — Claude Code's location.
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
