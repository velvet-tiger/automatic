use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, Agent};

/// Cline agent — stores project skills under
/// `<project>/.cline/skills/<name>/SKILL.md`, sub-agents under
/// `<project>/.cline/agents/<slug>.md`, and file hooks under
/// `<project>/.cline/hooks/<Event>.<ext>`.
///
/// Cline's current project-level rules live under `.clinerules/` as Markdown
/// files. Automatic manages a single file inside that directory.
/// MCP settings for the CLI live in global CLI state under
/// `~/.cline/data/settings/` (or `$CLINE_DIR/data/settings/` when overridden),
/// so Automatic can discover them but does not sync them per project.
pub struct Cline;

/// File-hook events Cline recognises today. Filenames placed under
/// `.cline/hooks/` must be exactly one of these plus a script extension
/// (e.g. `PreToolUse.sh`, `TaskStart.py`). Only one file per event slot is
/// loaded, so `sync_hooks` refuses duplicates and keeps the first.
///
/// Source: https://github.com/cline/cline/tree/main/sdk/examples/hooks
const CLINE_SUPPORTED_EVENTS: &[&str] = &[
    "TaskStart",
    "TaskResume",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "TaskComplete",
    "TaskError",
    "TaskCancel",
    "SessionShutdown",
];

impl Agent for Cline {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "cline"
    }

    fn label(&self) -> &'static str {
        "Cline"
    }

    fn config_description(&self) -> &'static str {
        "Global CLI settings (~/.cline/data/settings/cline_mcp_settings.json)"
    }

    fn project_file_name(&self) -> &'static str {
        ".clinerules/automatic.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join(".clinerules").exists()
            || dir.join(".cline").join("skills").exists()
            || dir.join(".cline").join("agents").exists()
            || dir.join(".cline").join("hooks").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".cline").join("skills")]
    }

    // ── Capabilities ────────────────────────────────────────────────────

    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            mcp_servers: false,
            global_mcp_servers: true,
            hooks: true,
            ..Default::default()
        }
    }

    fn mcp_note(&self) -> Option<&'static str> {
        Some(
            "Cline reads MCP servers from a global CLI settings file, not per project. Assign servers in Providers > Cline > MCP; Automatic writes to ~/.cline/data/settings/cline_mcp_settings.json (or $CLINE_DIR).",
        )
    }

    // ── Sub-agents ──────────────────────────────────────────────────────

    fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".cline").join("agents"))
    }

    // ── Hooks ───────────────────────────────────────────────────────────

    fn hook_events(&self) -> &'static [&'static str] {
        CLINE_SUPPORTED_EVENTS
    }

    fn sync_hooks(
        &self,
        project_dir: &Path,
        hooks: &[crate::core::Hook],
    ) -> Result<Vec<String>, String> {
        sync_cline_hooks(project_dir, hooks)
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        let _ = dir;
        vec![]
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(
        &self,
        _dir: &Path,
        _servers: &Map<String, Value>,
    ) -> Result<String, String> {
        // Cline stores MCP settings in global CLI state, not per-project.
        // Skip silently — the user is informed via `mcp_note()` in the UI.
        Ok(String::new())
    }

    fn global_mcp_target(&self) -> Option<super::GlobalMcpTarget> {
        Some(super::GlobalMcpTarget {
            path: self.global_mcp_settings_path()?,
            reload_note: Some("Cline usually hot-reloads on file change."),
        })
    }

    fn write_global_mcp_config(
        &self,
        desired: &Map<String, Value>,
        previously_managed: &[String],
    ) -> Result<super::GlobalMcpWriteReport, String> {
        let Some(target) = self.global_mcp_target() else {
            return Err("Home directory not available for Cline global MCP write".to_string());
        };

        // Cline's settings file follows the Claude-compatible mcpServers shape
        // — strip type/enabled/timeout on stdio entries; pass http/sse through.
        let mut rendered = Map::new();
        for (name, config) in desired {
            let transport = config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("stdio");

            let mut server = config.clone();
            if let Some(obj) = server.as_object_mut() {
                if transport == "stdio" {
                    obj.remove("type");
                    obj.remove("enabled");
                    obj.remove("timeout");
                }
            }
            rendered.insert(name.clone(), server);
        }

        super::merge_global_mcp_entries_json(&target.path, "mcpServers", &rendered, previously_managed)
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let _ = dir;
        Map::new()
    }

    fn extra_global_skill_dirs(&self) -> Vec<std::path::PathBuf> {
        // Cline stores skills in ~/.cline/skills/ at user level —
        // not covered by the standard ~/.agents/skills/ or ~/.claude/skills/ scan.
        match super::home_dir() {
            Some(home) => vec![home.join(".cline").join("skills")],
            None => vec![],
        }
    }

    fn detect_global_install(&self) -> bool {
        super::cli_available("cline")
            || self
                .global_mcp_settings_path()
                .is_some_and(|path| path.exists())
            || std::env::var_os("CLINE_DIR").is_some_and(|dir| PathBuf::from(dir).exists())
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        match self.global_mcp_settings_path() {
            Some(path) => discover_mcp_servers_from_json(&path, "mcpServers", identity),
            None => Map::new(),
        }
    }

    fn discover_global_mcp_entry_names(&self) -> std::collections::HashSet<String> {
        match self.global_mcp_settings_path() {
            Some(path) => super::read_global_mcp_entry_names_json(&path, "mcpServers"),
            None => std::collections::HashSet::new(),
        }
    }
}

impl Cline {
    fn global_mcp_settings_path(&self) -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("CLINE_DIR") {
            return Some(
                PathBuf::from(dir)
                    .join("data")
                    .join("settings")
                    .join("cline_mcp_settings.json"),
            );
        }

        super::home_dir().map(|home| {
            home.join(".cline")
                .join("data")
                .join("settings")
                .join("cline_mcp_settings.json")
        })
    }
}

/// Pass-through normaliser: Cline's format is already canonical.
fn identity(v: Value) -> Value {
    v
}

// ── Hook sync ───────────────────────────────────────────────────────────────
//
// Cline's file-hook mechanism loads one executable per event, named exactly
// after the event (`PreToolUse.sh`, `TaskStart.py`, …). This does not fit the
// merge/owned JSON-file variants of `HookConfigTarget`, so Cline runs its own
// writer — style borrowed from `sync_cursor_hooks`.
//
// Constraints from Cline:
// - One file per event slot. Automatic refuses the second hook targeting the
//   same event slot and keeps the first, with a warning.
// - Payload arrives on stdin as JSON. Command handlers are wrapped in a bash
//   script that pipes the payload back into the user's command.
// - Cline's file-hook layer does not natively filter by matcher, so hooks with
//   a matcher on `PreToolUse` / `PostToolUse` are wrapped in a `jq`-based guard
//   that exits 0 when the payload's `tool_name` doesn't match. When `jq` is
//   absent the guard is skipped (fail-open) rather than blocking every call.
// - Cleanup uses the shared `managed-by-automatic` marker so user-authored
//   scripts sitting in `.cline/hooks/` are left alone.

fn sync_cline_hooks(
    project_dir: &Path,
    hooks: &[crate::core::Hook],
) -> Result<Vec<String>, String> {
    let hooks_dir = project_dir.join(".cline").join("hooks");
    let mut written = Vec::new();

    // Drop hooks bound to events Cline doesn't ship a slot for.
    let usable: Vec<&crate::core::Hook> = hooks
        .iter()
        .filter(|h| {
            if CLINE_SUPPORTED_EVENTS.contains(&h.event.as_str()) {
                true
            } else {
                eprintln!(
                    "[automatic] Cline does not support hook event '{}' — skipping hook '{}'",
                    h.event, h.name
                );
                false
            }
        })
        .collect();

    // One file per event slot: first hook wins, warn about the rest.
    use std::collections::BTreeMap;
    let mut by_event: BTreeMap<&str, &crate::core::Hook> = BTreeMap::new();
    for hook in &usable {
        if let Some(existing) = by_event.get(hook.event.as_str()) {
            eprintln!(
                "[automatic] Cline loads only one hook per event slot — dropping '{}' (event '{}'); '{}' won.",
                hook.name, hook.event, existing.name
            );
            continue;
        }
        by_event.insert(hook.event.as_str(), *hook);
    }

    // Nothing to write, nothing on disk — don't create `.cline/hooks/` on
    // projects that never had hooks.
    if by_event.is_empty() && !hooks_dir.exists() {
        return Ok(written);
    }

    if !by_event.is_empty() {
        fs::create_dir_all(&hooks_dir)
            .map_err(|e| format!("Failed to create .cline/hooks/: {}", e))?;
    }

    let mut managed_paths: Vec<PathBuf> = Vec::new();
    for (event, hook) in &by_event {
        let (ext, body) = render_cline_hook_body(hook, event);
        let path = hooks_dir.join(format!("{}.{}", event, ext));
        fs::write(&path, body)
            .map_err(|e| format!("Failed to write '{}': {}", path.display(), e))?;
        set_executable(&path);
        managed_paths.push(path.clone());
        written.push(path.display().to_string());
    }

    super::cleanup_managed_hook_scripts(&hooks_dir, &managed_paths)?;

    Ok(written)
}

/// Render the on-disk body for one Cline hook. Returns `(extension, body)` —
/// the extension is derived from the interpreter for `Script` handlers, and
/// fixed as `sh` for `Command` and `Path` handlers (which are always wrapped
/// in a bash script). The body carries the `automatic-managed` marker and a
/// language-appropriate `cline event: <name>` comment so readers (and drift
/// checks) can see which event slot the file fills without walking the
/// parent directory.
fn render_cline_hook_body(hook: &crate::core::Hook, event: &str) -> (&'static str, String) {
    let (ext, body) = match &hook.handler {
        crate::core::HookHandler::Script {
            interpreter,
            script,
        } => {
            let ext = super::hook_script_extension(interpreter);
            (ext, super::hook_ensure_shebang(script, interpreter))
        }
        crate::core::HookHandler::Command { command } => {
            ("sh", cline_command_wrapper(hook, command))
        }
        crate::core::HookHandler::Path { path, interpreter } => {
            let invocation = match interpreter.as_deref().map(str::trim) {
                Some(interp) if !interp.is_empty() => format!("{} \"{}\"", interp, path),
                _ => format!("\"{}\"", path),
            };
            ("sh", cline_command_wrapper(hook, &invocation))
        }
    };
    let annotated = super::hook_annotate_managed_script(&body);
    (ext, inject_event_marker(&annotated, ext, event))
}

/// Insert a language-appropriate `cline event: <name>` comment right after
/// the shebang so the event slot is discoverable from the file contents.
fn inject_event_marker(body: &str, ext: &str, event: &str) -> String {
    let prefix = if ext == "js" || ext == "ts" { "//" } else { "#" };
    let marker = format!("{} cline event: {}\n", prefix, event);
    if let Some(rest) = body.strip_prefix("#!") {
        if let Some(nl) = rest.find('\n') {
            let (shebang, after) = body.split_at("#!".len() + nl + 1);
            return format!("{}{}{}", shebang, marker, after);
        }
    }
    format!("{}{}", marker, body)
}

/// Bash wrapper that pipes Cline's JSON payload on stdin into `command`.
/// When the hook targets a tool-carrying event and has a matcher, wrap the
/// command in a `jq`-based guard so mismatches exit 0 without invoking the
/// user's code. Missing `jq` fails open (guard skipped, command still runs).
fn cline_command_wrapper(hook: &crate::core::Hook, command: &str) -> String {
    let mut out = String::from("#!/usr/bin/env bash\n");
    out.push_str("set -eu\n");
    out.push_str("payload=\"$(cat)\"\n");

    if let Some(matcher) = hook.matcher.as_deref() {
        if hook.event == "PreToolUse" || hook.event == "PostToolUse" {
            out.push_str("if command -v jq >/dev/null 2>&1; then\n");
            out.push_str(&format!(
                "  if ! printf '%s' \"$payload\" | jq -e --arg re {} 'select(.tool_name // \"\" | test($re))' >/dev/null 2>&1; then\n",
                shell_single_quote(matcher)
            ));
            out.push_str("    exit 0\n");
            out.push_str("  fi\n");
            out.push_str("fi\n");
        } else {
            eprintln!(
                "[automatic] Cline hook '{}' has a matcher but event '{}' provides no tool_name — matcher ignored.",
                hook.name, hook.event
            );
        }
    }

    out.push_str(&format!("printf '%s' \"$payload\" | {}\n", command));
    out
}

/// Quote `s` for safe substitution into a single-quoted bash string. Handles
/// embedded single quotes by closing the quote, escaping, and reopening.
fn shell_single_quote(s: &str) -> String {
    let escaped = s.replace('\'', "'\\''");
    format!("'{}'", escaped)
}

fn set_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(path, perms);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Hook, HookHandler};
    use std::fs;
    use tempfile::tempdir;

    fn hook(name: &str, event: &str, command: &str) -> Hook {
        Hook {
            name: name.to_string(),
            agent: "cline".to_string(),
            event: event.to_string(),
            matcher: None,
            handler: HookHandler::Command {
                command: command.to_string(),
            },
            timeout_sec: None,
            plugin_id: None,
            _author: None,
        }
    }

    #[test]
    fn test_detect() {
        let dir = tempdir().unwrap();
        assert!(!Cline.detect_in(dir.path()));

        fs::create_dir_all(dir.path().join(".cline").join("skills")).unwrap();
        assert!(Cline.detect_in(dir.path()));
    }

    #[test]
    fn test_detect_clinerules() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".clinerules")).unwrap();
        assert!(Cline.detect_in(dir.path()));
    }

    #[test]
    fn test_detect_agents_and_hooks_dirs() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".cline").join("agents")).unwrap();
        assert!(Cline.detect_in(dir.path()));

        let other = tempdir().unwrap();
        fs::create_dir_all(other.path().join(".cline").join("hooks")).unwrap();
        assert!(Cline.detect_in(other.path()));
    }

    #[test]
    fn test_mcp_capability_disabled() {
        assert!(!Cline.capabilities().mcp_servers);
        assert!(Cline.mcp_note().is_some());
    }

    #[test]
    fn test_capabilities_enable_agents_and_hooks() {
        let caps = Cline.capabilities();
        assert!(caps.agents, "Cline supports sub-agents under .cline/agents/");
        assert!(caps.hooks, "Cline supports file hooks under .cline/hooks/");
        assert!(caps.global_mcp_servers);
    }

    #[test]
    fn test_agents_dir_points_at_dot_cline() {
        let dir = tempdir().unwrap();
        let agents = Cline.agents_dir(dir.path()).unwrap();
        assert_eq!(agents, dir.path().join(".cline").join("agents"));
    }

    #[test]
    fn test_hook_events_matches_cline_slots() {
        let events = Cline.hook_events();
        assert_eq!(events.len(), CLINE_SUPPORTED_EVENTS.len());
        for name in [
            "TaskStart",
            "PreToolUse",
            "PostToolUse",
            "TaskComplete",
            "SessionShutdown",
        ] {
            assert!(events.contains(&name), "missing event {}", name);
        }
    }

    #[test]
    fn test_project_file_name_uses_managed_rule_file() {
        assert_eq!(Cline.project_file_name(), ".clinerules/automatic.md");
    }

    #[test]
    fn test_sync_hooks_writes_command_wrapper() {
        let dir = tempdir().unwrap();
        let written = Cline
            .sync_hooks(dir.path(), &[hook("audit", "PreToolUse", "logger tool")])
            .unwrap();
        assert_eq!(written.len(), 1);

        let path = dir.path().join(".cline").join("hooks").join("PreToolUse.sh");
        assert!(path.exists());
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.starts_with("#!/usr/bin/env bash"));
        assert!(body.contains("managed-by-automatic"));
        assert!(body.contains("# cline event: PreToolUse"));
        assert!(body.contains("logger tool"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o755);
        }
    }

    #[test]
    fn test_sync_hooks_script_handler_uses_interpreter_extension() {
        let dir = tempdir().unwrap();
        let h = Hook {
            name: "py-hook".to_string(),
            agent: "cline".to_string(),
            event: "TaskStart".to_string(),
            matcher: None,
            handler: HookHandler::Script {
                interpreter: "python3".to_string(),
                script: "print('hi')\n".to_string(),
            },
            timeout_sec: None,
            plugin_id: None,
            _author: None,
        };
        Cline.sync_hooks(dir.path(), &[h]).unwrap();

        let path = dir.path().join(".cline").join("hooks").join("TaskStart.py");
        assert!(path.exists());
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.starts_with("#!/usr/bin/env python3"));
        assert!(body.contains("managed-by-automatic"));
        assert!(body.contains("print('hi')"));
    }

    #[test]
    fn test_sync_hooks_refuses_duplicate_event() {
        let dir = tempdir().unwrap();
        let written = Cline
            .sync_hooks(
                dir.path(),
                &[
                    hook("first", "PreToolUse", "first-cmd"),
                    hook("second", "PreToolUse", "second-cmd"),
                ],
            )
            .unwrap();
        assert_eq!(written.len(), 1, "only one file per event slot");

        let body = fs::read_to_string(
            dir.path().join(".cline").join("hooks").join("PreToolUse.sh"),
        )
        .unwrap();
        assert!(body.contains("first-cmd"));
        assert!(!body.contains("second-cmd"));
    }

    #[test]
    fn test_sync_hooks_skips_unsupported_event() {
        let dir = tempdir().unwrap();
        let written = Cline
            .sync_hooks(
                dir.path(),
                &[hook("nope", "SessionStart", "cmd")],
            )
            .unwrap();
        assert!(written.is_empty());
        assert!(!dir.path().join(".cline").join("hooks").exists());
    }

    #[test]
    fn test_sync_hooks_wraps_matcher_with_jq_guard() {
        let dir = tempdir().unwrap();
        let mut h = hook("guarded", "PreToolUse", "cmd");
        h.matcher = Some("write.*".to_string());
        Cline.sync_hooks(dir.path(), &[h]).unwrap();

        let body = fs::read_to_string(
            dir.path().join(".cline").join("hooks").join("PreToolUse.sh"),
        )
        .unwrap();
        assert!(body.contains("command -v jq"));
        assert!(body.contains("test($re)"));
        assert!(body.contains("'write.*'"));
    }

    #[test]
    fn test_sync_hooks_cleanup_removes_managed_only() {
        let dir = tempdir().unwrap();
        Cline
            .sync_hooks(dir.path(), &[hook("audit", "PreToolUse", "cmd")])
            .unwrap();

        // Drop a user-authored script alongside the managed one.
        let hooks_dir = dir.path().join(".cline").join("hooks");
        let user_script = hooks_dir.join("PostToolUse.sh");
        fs::write(&user_script, "#!/bin/sh\necho user\n").unwrap();

        // Re-sync with an empty hook set — managed file should go, user file stays.
        Cline.sync_hooks(dir.path(), &[]).unwrap();
        assert!(!hooks_dir.join("PreToolUse.sh").exists());
        assert!(user_script.exists());
    }

    #[test]
    fn test_sync_hooks_leaves_project_alone_when_no_hooks_and_no_dir() {
        let dir = tempdir().unwrap();
        let written = Cline.sync_hooks(dir.path(), &[]).unwrap();
        assert!(written.is_empty());
        assert!(!dir.path().join(".cline").exists());
    }

    #[test]
    fn test_discover_global_mcp_servers_uses_cline_dir_override() {
        let cline_dir = tempdir().unwrap();
        std::env::set_var("CLINE_DIR", cline_dir.path());
        let settings_dir = cline_dir.path().join("data").join("settings");
        fs::create_dir_all(&settings_dir).unwrap();
        fs::write(
            settings_dir.join("cline_mcp_settings.json"),
            r#"{"mcpServers":{"github":{"command":"npx","args":["-y","@modelcontextprotocol/server-github"]}}}"#,
        )
        .unwrap();

        let servers = Cline.discover_global_mcp_servers();
        std::env::remove_var("CLINE_DIR");

        assert!(servers.contains_key("github"));
    }
}
