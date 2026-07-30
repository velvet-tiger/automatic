use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, Agent};

/// Cursor agent — writes `.cursor/mcp.json` and stores skills under
/// `<project>/.agents/skills/<name>/SKILL.md`.
pub struct Cursor;

impl Agent for Cursor {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "cursor"
    }

    fn label(&self) -> &'static str {
        "Cursor"
    }

    fn config_description(&self) -> &'static str {
        ".cursor/mcp.json"
    }

    fn project_file_name(&self) -> &'static str {
        // Cursor reads AGENTS.md natively; `.cursorrules` is its legacy
        // format and is migrated by the sync engine.
        "AGENTS.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        // AGENTS.md is deliberately excluded: it is shared with Codex and
        // friends and would misattribute those projects to Cursor.
        dir.join(".cursor").join("mcp.json").exists()
            || dir.join(".cursorrules").exists()
            || dir.join(".cursor").join("rules").exists()
            || dir.join(".cursor").join("agents").exists()
            || dir.join(".cursor").join("commands").exists()
            || dir.join(".cursor").join("hooks.json").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".agents").join("skills")]
    }

    fn sync_instruction_rules(
        &self,
        project: &crate::core::Project,
        filename: &str,
        rule_names: &[String],
        custom_contents: &[String],
    ) -> Result<Option<Vec<String>>, String> {
        // Only takes over when the .cursor/rules option is on AND Cursor is
        // the sole user of AGENTS.md (the gate checks both).
        if !crate::core::project_uses_cursor_mdc_rules(project, filename) {
            return Ok(None);
        }

        let mut touched = Vec::new();
        if !rule_names.is_empty() {
            touched.extend(crate::core::sync_rules_to_cursor_mdc_rules(
                &project.directory,
                rule_names,
            )?);
        }

        // Custom rules are always injected inline — they have no machine name
        // to use as an .mdc filename.
        if crate::core::inject_rules_into_project_file_with_custom(
            &project.directory,
            filename,
            &[],
            custom_contents,
        )? {
            touched.push(
                Path::new(&project.directory)
                    .join(filename)
                    .display()
                    .to_string(),
            );
        }

        Ok(Some(touched))
    }

    // ── Capabilities ────────────────────────────────────────────────────

    fn capabilities(&self) -> super::AgentCapabilities {
        super::AgentCapabilities {
            commands: true,
            hooks: true,
            ..Default::default()
        }
    }

    fn sync_hooks(
        &self,
        project_dir: &Path,
        hooks: &[crate::core::Hook],
    ) -> Result<Vec<String>, String> {
        sync_cursor_hooks(project_dir, hooks)
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".cursor").join("mcp.json")]
    }

    // ── Config writing ──────────────────────────────────────────────────

    /// Cursor resolves `${env:NAME}`; the bare `${NAME}` form Claude Code uses
    /// is passed through as a literal string.
    fn rewrite_inherited_env(&self, server: &mut Map<String, Value>, keys: &[String]) {
        super::substitute_inherited_env(server, keys, |key| format!("${{env:{}}}", key));
    }

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        // Cursor's `mcpServers` map looks like Claude Code's but diverges on
        // two points: OAuth client details live under `auth` in SCREAMING_CASE,
        // and `enabled`/`timeout` are not part of the schema on any transport.
        let mut cursor_servers = Map::new();

        for (name, config) in servers {
            let transport = config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("stdio");

            let mut server = config.clone();
            if let Some(obj) = server.as_object_mut() {
                obj.remove("enabled");
                obj.remove("timeout");

                if transport == "stdio" {
                    // Cursor's reference table lists `type` as required.
                    obj.insert("type".to_string(), Value::String("stdio".to_string()));
                } else if let Some(auth) = cursor_auth_block(obj.remove("oauth")) {
                    obj.insert("auth".to_string(), auth);
                }
            }
            cursor_servers.insert(name.clone(), server);
        }

        let output = json!({ "mcpServers": Value::Object(cursor_servers) });

        let cursor_dir = dir.join(".cursor");
        if !cursor_dir.exists() {
            fs::create_dir_all(&cursor_dir)
                .map_err(|e| format!("Failed to create .cursor/: {}", e))?;
        }

        let path = cursor_dir.join("mcp.json");
        let content =
            serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write .cursor/mcp.json: {}", e))?;

        Ok(path.display().to_string())
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = dir.join(".cursor").join("mcp.json");
        if !path.exists() {
            return Map::new();
        }
        // Cursor's format matches Claude's — no normalisation needed.
        discover_mcp_servers_from_json(&path, "mcpServers", identity)
    }

    fn detect_global_install(&self) -> bool {
        // Cursor ships as an app bundle (macOS) and optionally a CLI.
        std::path::Path::new("/Applications/Cursor.app").exists()
            || super::cli_available("cursor")
            || super::home_dir()
                .map(|h| h.join(".cursor").exists())
                .unwrap_or(false)
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };
        // ~/.cursor/mcp.json — user-level Cursor MCP config
        let path = home.join(".cursor").join("mcp.json");
        discover_mcp_servers_from_json(&path, "mcpServers", identity)
    }

    fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".cursor").join("agents"))
    }

    fn commands_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".cursor").join("commands"))
    }
}

/// Pass-through normaliser: Cursor's format is already canonical.
fn identity(v: Value) -> Value {
    v
}

/// Translate Automatic's `oauth` block into Cursor's `auth` block.
///
/// Automatic stores `{clientId, clientSecret, scope, callbackPort}`; Cursor
/// expects `{CLIENT_ID, CLIENT_SECRET, scopes}` and registers its own fixed
/// redirect URLs, so `callbackPort` has no counterpart and is dropped.  A block
/// without a client id gives Cursor nothing to work with, so it is omitted and
/// Cursor falls back to Dynamic Client Registration.
fn cursor_auth_block(oauth: Option<Value>) -> Option<Value> {
    let source = oauth?;
    let source = source.as_object()?;

    let client_id = source
        .get("clientId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;

    let mut auth = Map::new();
    auth.insert(
        "CLIENT_ID".to_string(),
        Value::String(client_id.to_string()),
    );

    if let Some(secret) = source
        .get("clientSecret")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        auth.insert(
            "CLIENT_SECRET".to_string(),
            Value::String(secret.to_string()),
        );
    }

    // Automatic stores the space-separated OAuth `scope` string; Cursor takes
    // an array.  When empty, omitting the key lets Cursor discover
    // `scopes_supported` from the authorization server metadata.
    let scopes: Vec<Value> = source
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .split_whitespace()
        .map(|s| Value::String(s.to_string()))
        .collect();
    if !scopes.is_empty() {
        auth.insert("scopes".to_string(), Value::Array(scopes));
    }

    Some(Value::Object(auth))
}

// ── Hooks ────────────────────────────────────────────────────────────────────
//
// Cursor reads hooks from `.cursor/hooks.json`:
//
//   { "version": 1, "hooks": { "<event>": [ { "command": "...", ... } ] } }
//
// Unlike Claude Code's matcher-group nesting, each event maps to a FLAT array
// of handler objects, with `matcher` on the handler itself.
//
// Ownership: Cursor's hooks.json is a versioned schema and it is not known
// whether unknown keys per handler are tolerated, so — unlike Claude Code's
// `_managedBy` tagging — the file stays 100% schema-clean.  The handlers we
// emitted are recorded in a sidecar manifest at
// `.automatic/state/cursor-hooks.json`; on every sync we remove the first
// handler deep-equal to each manifest entry before merging the fresh set, so
// re-sync is idempotent, detach removes entries, and user-authored handlers
// are preserved.  A user who hand-edits a managed entry breaks the deep-equal
// match and thereby adopts it as their own.

/// Events supported by Cursor's hook system (Tab-completion hooks excluded).
///
/// KEEP IN LOCKSTEP with the `cursor` entry of `EVENTS_BY_AGENT` in
/// `src/pages/workspace/Hooks.tsx` — hooks whose event is not listed here are
/// skipped at sync time with a warning.
const CURSOR_SUPPORTED_EVENTS: &[&str] = &[
    "sessionStart",
    "sessionEnd",
    "beforeSubmitPrompt",
    "preToolUse",
    "postToolUse",
    "postToolUseFailure",
    "beforeShellExecution",
    "afterShellExecution",
    "beforeMCPExecution",
    "afterMCPExecution",
    "beforeReadFile",
    "afterFileEdit",
    "stop",
    "subagentStart",
    "subagentStop",
    "preCompact",
    "afterAgentResponse",
    "afterAgentThought",
    "workspaceOpen",
];

/// Relative path of the sidecar manifest recording the handler JSON we last
/// wrote into `.cursor/hooks.json`, keyed by event.
const CURSOR_HOOKS_MANIFEST: &str = ".automatic/state/cursor-hooks.json";

fn sync_cursor_hooks(
    project_dir: &Path,
    hooks: &[crate::core::Hook],
) -> Result<Vec<String>, String> {
    let cursor_dir = project_dir.join(".cursor");
    let hooks_file = cursor_dir.join("hooks.json");
    let scripts_dir = cursor_dir.join("hooks");
    let manifest_path = project_dir.join(CURSOR_HOOKS_MANIFEST);
    let mut written = Vec::new();

    let usable_hooks: Vec<&crate::core::Hook> = hooks
        .iter()
        .filter(|h| {
            if CURSOR_SUPPORTED_EVENTS.contains(&h.event.as_str()) {
                true
            } else {
                eprintln!(
                    "[automatic] Cursor does not support hook event '{}' — skipping hook '{}'",
                    h.event, h.name
                );
                false
            }
        })
        .collect();

    let manifest = load_cursor_hooks_manifest(&manifest_path);

    // Nothing to add and nothing previously written — leave the project alone
    // (do not create .cursor/ on projects that never had hooks).
    if usable_hooks.is_empty() && manifest.is_empty() && !hooks_file.exists() {
        super::cleanup_managed_hook_scripts(&scripts_dir, &[])?;
        return Ok(written);
    }

    // Write script-handler bodies first so hooks.json can reference them.
    let mut managed_script_paths: Vec<std::path::PathBuf> = Vec::new();
    if usable_hooks
        .iter()
        .any(|h| matches!(h.handler, crate::core::HookHandler::Script { .. }))
    {
        fs::create_dir_all(&scripts_dir)
            .map_err(|e| format!("Failed to create .cursor/hooks/: {}", e))?;
    }
    for hook in &usable_hooks {
        if let crate::core::HookHandler::Script {
            interpreter,
            script,
        } = &hook.handler
        {
            let path = super::write_managed_hook_script(&scripts_dir, hook, interpreter, script)?;
            managed_script_paths.push(path.clone());
            written.push(path.display().to_string());
        }
    }

    // Load or initialise the hooks.json document.
    let mut document: Value = if hooks_file.exists() {
        let raw = fs::read_to_string(&hooks_file)
            .map_err(|e| format!("Failed to read .cursor/hooks.json: {}", e))?;
        if raw.trim().is_empty() {
            Value::Object(Map::new())
        } else {
            serde_json::from_str(&raw).map_err(|e| {
                format!(
                    "Failed to parse .cursor/hooks.json — fix the syntax or remove the file: {}",
                    e
                )
            })?
        }
    } else {
        Value::Object(Map::new())
    };
    let doc_obj = document
        .as_object_mut()
        .ok_or_else(|| ".cursor/hooks.json must be a JSON object".to_string())?;

    let hooks_value = doc_obj
        .entry("hooks".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let hooks_obj = hooks_value
        .as_object_mut()
        .ok_or_else(|| "`hooks` in .cursor/hooks.json must be an object".to_string())?;

    // Strip: remove the first handler deep-equal to each manifest entry.
    for (event, handler) in &manifest {
        if let Some(arr) = hooks_obj.get_mut(event).and_then(|v| v.as_array_mut()) {
            if let Some(idx) = arr.iter().position(|h| h == handler) {
                arr.remove(idx);
            }
        }
    }

    // Merge the fresh handler set (BTreeMap for stable event ordering) and
    // record what we emitted for the next sync's strip pass.
    use std::collections::BTreeMap;
    let mut grouped: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for hook in &usable_hooks {
        grouped
            .entry(hook.event.clone())
            .or_default()
            .push(cursor_handler_value(hook));
    }
    let mut new_manifest: Vec<(String, Value)> = Vec::new();
    for (event, handlers) in grouped {
        let entry = hooks_obj
            .entry(event.clone())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(arr) = entry.as_array_mut() {
            for handler in handlers {
                new_manifest.push((event.clone(), handler.clone()));
                arr.push(handler);
            }
        }
    }

    // Prune empty event arrays left behind by the strip pass.
    let empty_events: Vec<String> = hooks_obj
        .iter()
        .filter(|(_, v)| v.as_array().map(|a| a.is_empty()).unwrap_or(false))
        .map(|(k, _)| k.clone())
        .collect();
    for event in empty_events {
        hooks_obj.remove(&event);
    }

    if hooks_obj.is_empty() {
        doc_obj.remove("hooks");
        // Delete the file entirely when nothing but our bookkeeping remains;
        // preserve it if the user added other top-level keys.
        let only_version = doc_obj.keys().all(|k| k == "version");
        if only_version {
            if hooks_file.exists() {
                let _ = fs::remove_file(&hooks_file);
            }
        } else {
            write_cursor_hooks_file(&hooks_file, doc_obj, &mut written)?;
        }
    } else {
        doc_obj.insert(
            "version".to_string(),
            doc_obj
                .get("version")
                .cloned()
                .unwrap_or_else(|| Value::Number(serde_json::Number::from(1u32))),
        );
        fs::create_dir_all(&cursor_dir).map_err(|e| format!("Failed to create .cursor/: {}", e))?;
        write_cursor_hooks_file(&hooks_file, doc_obj, &mut written)?;
    }

    write_cursor_hooks_manifest(&manifest_path, &new_manifest)?;
    super::cleanup_managed_hook_scripts(&scripts_dir, &managed_script_paths)?;

    Ok(written)
}

/// Build a Cursor hook handler object:
/// `{"type": "command", "command": ..., "timeout"?: n, "matcher"?: s}`.
fn cursor_handler_value(hook: &crate::core::Hook) -> Value {
    let mut handler = Map::new();
    handler.insert("type".to_string(), Value::String("command".to_string()));

    let command_str = match &hook.handler {
        crate::core::HookHandler::Command { command } => command.clone(),
        crate::core::HookHandler::Script { interpreter, .. } => {
            let ext = super::hook_script_extension(interpreter);
            format!("./.cursor/hooks/{}.{}", super::hook_slug(hook), ext)
        }
        // The user owns the file — pass the path through verbatim.
        crate::core::HookHandler::Path { path, .. } => path.clone(),
    };
    handler.insert("command".to_string(), Value::String(command_str));

    if let Some(timeout) = hook.timeout_sec {
        handler.insert(
            "timeout".to_string(),
            Value::Number(serde_json::Number::from(timeout)),
        );
    }
    if let Some(matcher) = &hook.matcher {
        handler.insert("matcher".to_string(), Value::String(matcher.clone()));
    }

    Value::Object(handler)
}

fn write_cursor_hooks_file(
    hooks_file: &Path,
    doc_obj: &Map<String, Value>,
    written: &mut Vec<String>,
) -> Result<(), String> {
    let pretty = serde_json::to_string_pretty(&Value::Object(doc_obj.clone()))
        .map_err(|e| format!("JSON error: {}", e))?;
    fs::write(hooks_file, format!("{}\n", pretty))
        .map_err(|e| format!("Failed to write .cursor/hooks.json: {}", e))?;
    written.push(hooks_file.display().to_string());
    Ok(())
}

/// Load the sidecar manifest.  Unreadable or malformed manifests degrade to
/// empty — the worst case is a duplicated managed entry that the next detach
/// or manual edit resolves, never data loss.
fn load_cursor_hooks_manifest(manifest_path: &Path) -> Vec<(String, Value)> {
    let Ok(raw) = fs::read_to_string(manifest_path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return Vec::new();
    };
    let Some(entries) = value.as_array() else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let event = entry.get("event")?.as_str()?.to_string();
            let handler = entry.get("handler")?.clone();
            Some((event, handler))
        })
        .collect()
}

fn write_cursor_hooks_manifest(
    manifest_path: &Path,
    entries: &[(String, Value)],
) -> Result<(), String> {
    if entries.is_empty() {
        if manifest_path.exists() {
            let _ = fs::remove_file(manifest_path);
        }
        return Ok(());
    }
    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
    }
    let value: Vec<Value> = entries
        .iter()
        .map(|(event, handler)| json!({ "event": event, "handler": handler }))
        .collect();
    let pretty = serde_json::to_string_pretty(&Value::Array(value))
        .map_err(|e| format!("JSON error: {}", e))?;
    fs::write(manifest_path, format!("{}\n", pretty))
        .map_err(|e| format!("Failed to write Cursor hooks manifest: {}", e))?;
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn stdio_servers() -> Map<String, Value> {
        let mut s = Map::new();
        s.insert(
            "automatic".to_string(),
            json!({"type":"stdio","command":"/usr/local/bin/automatic","args":["mcp-serve"]}),
        );
        s.insert(
            "github".to_string(),
            json!({"type":"stdio","command":"npx","args":["-y","@modelcontextprotocol/server-github"],"env":{"GITHUB_TOKEN":"ghp_test123"}}),
        );
        s
    }

    fn http_servers() -> Map<String, Value> {
        let mut s = Map::new();
        s.insert(
            "remote-api".to_string(),
            json!({"type":"http","url":"https://api.example.com/mcp","headers":{"Authorization":"Bearer tok_abc123"},"oauth":{"clientId":"client_123","scope":"read"}}),
        );
        s
    }

    #[test]
    fn test_detect() {
        let dir = tempdir().unwrap();
        assert!(!Cursor.detect_in(dir.path()));

        fs::create_dir_all(dir.path().join(".cursor")).unwrap();
        fs::write(dir.path().join(".cursor/mcp.json"), "{}").unwrap();
        assert!(Cursor.detect_in(dir.path()));
    }

    #[test]
    fn test_detect_cursorrules() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".cursorrules"), "").unwrap();
        assert!(Cursor.detect_in(dir.path()));
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        Cursor
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".cursor/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // stdio entries keep an explicit "type": "stdio" (Cursor expects it)
        assert_eq!(
            parsed["mcpServers"]["automatic"]["type"].as_str().unwrap(),
            "stdio"
        );
        assert!(parsed["mcpServers"]["automatic"]["command"]
            .as_str()
            .unwrap()
            .contains("automatic"));
        assert_eq!(
            parsed["mcpServers"]["github"]["command"].as_str().unwrap(),
            "npx"
        );
    }

    #[test]
    fn managed_gitignore_paths_collapse_to_expected_patterns() {
        let dir = tempdir().unwrap();
        let paths = crate::agent::Agent::managed_gitignore_paths(&Cursor, dir.path());
        let patterns = crate::core::gitignore::build_patterns(dir.path(), &paths, false);

        // AGENTS.md is the instruction file; every .cursor/* path collapses to
        // the whole directory; .agents/skills is covered by the universal
        // /.agents/ entry.
        assert!(patterns.contains(&"/AGENTS.md".to_string()), "{patterns:?}");
        assert!(patterns.contains(&"/.cursor/".to_string()), "{patterns:?}");
        assert!(
            !patterns.iter().any(|p| p.contains(".cursorrules")),
            "legacy .cursorrules must no longer be ignored: {patterns:?}"
        );
    }

    // ── Hook sync ───────────────────────────────────────────────────────────

    fn cmd_hook(
        name: &str,
        event: &str,
        matcher: Option<&str>,
        command: &str,
    ) -> crate::core::Hook {
        crate::core::Hook {
            name: name.to_string(),
            agent: "cursor".to_string(),
            event: event.to_string(),
            matcher: matcher.map(|s| s.to_string()),
            handler: crate::core::HookHandler::Command {
                command: command.to_string(),
            },
            timeout_sec: None,
            plugin_id: None,
            _author: None,
        }
    }

    fn path_hook(name: &str, event: &str, path: &str) -> crate::core::Hook {
        crate::core::Hook {
            name: name.to_string(),
            agent: "cursor".to_string(),
            event: event.to_string(),
            matcher: None,
            handler: crate::core::HookHandler::Path {
                path: path.to_string(),
                interpreter: None,
            },
            timeout_sec: None,
            plugin_id: None,
            _author: None,
        }
    }

    fn script_hook(name: &str, event: &str, interpreter: &str, script: &str) -> crate::core::Hook {
        crate::core::Hook {
            name: name.to_string(),
            agent: "cursor".to_string(),
            event: event.to_string(),
            matcher: None,
            handler: crate::core::HookHandler::Script {
                interpreter: interpreter.to_string(),
                script: script.to_string(),
            },
            timeout_sec: Some(60),
            plugin_id: None,
            _author: None,
        }
    }

    fn read_hooks_json(dir: &Path) -> Value {
        let raw = fs::read_to_string(dir.join(".cursor/hooks.json")).expect("read hooks.json");
        serde_json::from_str(&raw).expect("parse hooks.json")
    }

    #[test]
    fn cursor_hook_sync_writes_flat_handler_arrays() {
        let dir = tempdir().unwrap();
        let hooks = vec![cmd_hook(
            "log-session",
            "sessionStart",
            None,
            "echo session started",
        )];

        let written = Cursor.sync_hooks(dir.path(), &hooks).expect("sync");
        assert!(written.iter().any(|p| p.ends_with("hooks.json")));

        let v = read_hooks_json(dir.path());
        assert_eq!(v["version"], 1);
        let handlers = v["hooks"]["sessionStart"].as_array().unwrap();
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0]["type"], "command");
        assert_eq!(handlers[0]["command"], "echo session started");
        // Schema-clean: no ownership tags in the vendor file.
        assert!(handlers[0].get("_managedBy").is_none());

        // Ownership is recorded in the sidecar manifest instead.
        let manifest = fs::read_to_string(dir.path().join(".automatic/state/cursor-hooks.json"))
            .expect("manifest exists");
        assert!(manifest.contains("sessionStart"));
    }

    #[test]
    fn cursor_hook_sync_puts_matcher_on_handler() {
        let dir = tempdir().unwrap();
        let hooks = vec![cmd_hook(
            "guard-shell",
            "preToolUse",
            Some("Shell"),
            "./check.sh",
        )];

        Cursor.sync_hooks(dir.path(), &hooks).expect("sync");

        let v = read_hooks_json(dir.path());
        let handlers = v["hooks"]["preToolUse"].as_array().unwrap();
        assert_eq!(handlers[0]["matcher"], "Shell");
    }

    #[test]
    fn cursor_hook_sync_is_idempotent_and_preserves_user_entries() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".cursor")).unwrap();
        let preexisting = json!({
            "version": 1,
            "customTopLevel": true,
            "hooks": {
                "stop": [ { "type": "command", "command": "echo user-owned" } ]
            }
        });
        fs::write(
            dir.path().join(".cursor/hooks.json"),
            serde_json::to_string_pretty(&preexisting).unwrap(),
        )
        .unwrap();

        let hooks = vec![cmd_hook("bye", "stop", None, "echo managed")];
        Cursor.sync_hooks(dir.path(), &hooks).expect("sync 1");
        Cursor.sync_hooks(dir.path(), &hooks).expect("sync 2");
        Cursor.sync_hooks(dir.path(), &hooks).expect("sync 3");

        let v = read_hooks_json(dir.path());
        assert_eq!(v["customTopLevel"], true, "unknown top-level keys survive");
        let handlers = v["hooks"]["stop"].as_array().unwrap();
        assert_eq!(
            handlers.len(),
            2,
            "one user entry + one managed entry, no duplicates: {:?}",
            handlers
        );
        assert_eq!(handlers[0]["command"], "echo user-owned");
        assert_eq!(handlers[1]["command"], "echo managed");
    }

    #[test]
    fn cursor_hook_detach_removes_entries_and_file() {
        let dir = tempdir().unwrap();
        let hooks = vec![cmd_hook("bye", "stop", None, "echo managed")];
        Cursor.sync_hooks(dir.path(), &hooks).expect("sync");
        assert!(dir.path().join(".cursor/hooks.json").exists());

        Cursor.sync_hooks(dir.path(), &[]).expect("detach");

        assert!(
            !dir.path().join(".cursor/hooks.json").exists(),
            "file should be removed when only managed entries remain"
        );
        assert!(
            !dir.path()
                .join(".automatic/state/cursor-hooks.json")
                .exists(),
            "manifest should be removed when empty"
        );
    }

    #[test]
    fn cursor_hook_detach_keeps_user_entries() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".cursor")).unwrap();
        fs::write(
            dir.path().join(".cursor/hooks.json"),
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "hooks": { "stop": [ { "type": "command", "command": "echo user-owned" } ] }
            }))
            .unwrap(),
        )
        .unwrap();

        let hooks = vec![cmd_hook("bye", "stop", None, "echo managed")];
        Cursor.sync_hooks(dir.path(), &hooks).expect("sync");
        Cursor.sync_hooks(dir.path(), &[]).expect("detach");

        let v = read_hooks_json(dir.path());
        let handlers = v["hooks"]["stop"].as_array().unwrap();
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0]["command"], "echo user-owned");
    }

    #[test]
    fn cursor_hook_script_handler_writes_managed_script() {
        let dir = tempdir().unwrap();
        let hooks = vec![script_hook(
            "Format Check",
            "afterFileEdit",
            "bash",
            "echo checking",
        )];

        Cursor.sync_hooks(dir.path(), &hooks).expect("sync");

        let script_path = dir.path().join(".cursor/hooks/format-check.sh");
        let body = fs::read_to_string(&script_path).expect("script written");
        assert!(body.starts_with("#!/usr/bin/env bash"));
        assert!(body.contains("managed-by-automatic"));

        let v = read_hooks_json(dir.path());
        let handlers = v["hooks"]["afterFileEdit"].as_array().unwrap();
        assert_eq!(handlers[0]["command"], "./.cursor/hooks/format-check.sh");
        assert_eq!(handlers[0]["timeout"], 60);

        // Detach removes the orphaned managed script.
        Cursor.sync_hooks(dir.path(), &[]).expect("detach");
        assert!(!script_path.exists());
    }

    #[test]
    fn cursor_hook_path_handler_passes_through() {
        let dir = tempdir().unwrap();
        let hooks = vec![path_hook("mine", "sessionEnd", "./scripts/my-hook.sh")];

        Cursor.sync_hooks(dir.path(), &hooks).expect("sync");

        let v = read_hooks_json(dir.path());
        let handlers = v["hooks"]["sessionEnd"].as_array().unwrap();
        assert_eq!(handlers[0]["command"], "./scripts/my-hook.sh");
        // No script file is created for path handlers.
        assert!(!dir.path().join(".cursor/hooks").exists());
    }

    #[test]
    fn cursor_hook_unsupported_event_is_skipped() {
        let dir = tempdir().unwrap();
        let hooks = vec![cmd_hook("bad", "SessionStart", None, "echo wrong-case")];

        let written = Cursor.sync_hooks(dir.path(), &hooks).expect("sync");
        assert!(written.is_empty());
        assert!(
            !dir.path().join(".cursor/hooks.json").exists(),
            "unsupported-only hook sets should not create hooks.json"
        );
    }

    #[test]
    fn cursor_hook_user_edited_managed_entry_is_adopted() {
        let dir = tempdir().unwrap();
        let hooks = vec![cmd_hook("bye", "stop", None, "echo managed")];
        Cursor.sync_hooks(dir.path(), &hooks).expect("sync");

        // The user edits our managed entry by hand.
        let mut v = read_hooks_json(dir.path());
        v["hooks"]["stop"][0]["command"] = json!("echo edited-by-user");
        fs::write(
            dir.path().join(".cursor/hooks.json"),
            serde_json::to_string_pretty(&v).unwrap(),
        )
        .unwrap();

        // Re-sync: the edited entry no longer matches the manifest, so it is
        // adopted as user-owned and the fresh managed entry is added next to it.
        Cursor.sync_hooks(dir.path(), &hooks).expect("re-sync");
        let v = read_hooks_json(dir.path());
        let handlers = v["hooks"]["stop"].as_array().unwrap();
        assert_eq!(handlers.len(), 2);
        assert_eq!(handlers[0]["command"], "echo edited-by-user");
        assert_eq!(handlers[1]["command"], "echo managed");
    }

    #[test]
    fn test_write_http() {
        let dir = tempdir().unwrap();
        Cursor
            .write_mcp_config(dir.path(), &http_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".cursor/mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            parsed["mcpServers"]["remote-api"]["type"].as_str().unwrap(),
            "http"
        );
        assert_eq!(
            parsed["mcpServers"]["remote-api"]["url"].as_str().unwrap(),
            "https://api.example.com/mcp"
        );
    }
}
