use serde::{Deserialize, Serialize};

/// Node package manager used to run a project's scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
}

impl PackageManager {
    /// The executable name to resolve on `$PATH`.
    pub fn binary(&self) -> &'static str {
        match self {
            PackageManager::Npm => "npm",
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            PackageManager::Npm => "npm",
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
        }
    }
}

/// A configured dev server for a project: which script to run, with which
/// package manager, from which directory. Persisted independently of the
/// running process — a config can exist (and be edited) whether or not the
/// server is currently running.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Stable identifier, generated on first save.
    pub id: String,
    /// Display name (e.g. "web", "api").
    pub name: String,
    pub package_manager: PackageManager,
    /// The npm script to run (e.g. "dev").
    pub script: String,
    /// Directory the server runs from, relative to the project directory.
    /// Empty string means the project root — used for monorepos where a
    /// server lives in a subpackage.
    #[serde(default)]
    pub subdirectory: String,
    /// Optional port the server is expected to listen on, used only to show
    /// an "Open" link once running. Not passed to the process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default)]
    pub created_at: String,
}

/// A single captured line of process output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLine {
    pub stream: LogStream,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    Stdout,
    Stderr,
}

/// A `ServerConfig` combined with its current runtime state. Returned to the
/// frontend for both the per-project tab and the cross-project Tools view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevServerStatus {
    pub id: String,
    pub project: String,
    pub name: String,
    pub package_manager: PackageManager,
    pub script: String,
    pub subdirectory: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// URLs the server has printed to stdout/stderr that point back at this
    /// machine (e.g. `http://localhost:5173/`), in first-seen order. Detected
    /// from output rather than configured, so it reflects the port the
    /// server actually bound to even if that differs from `port`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<String>,
}

/// A single script found in a project's `package.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpmScriptEntry {
    pub name: String,
    pub command: String,
}
