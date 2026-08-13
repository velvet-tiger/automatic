use serde::{Deserialize, Serialize};

/// Runtime status of the single, machine-wide Maildev process. Returned to
/// the frontend by every command in `commands.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaildevStatus {
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Captured stderr tail, populated only when the process is not running
    /// and exited with output (e.g. "EADDRINUSE"). The sole diagnostic
    /// surface — there is no paginated log view.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
