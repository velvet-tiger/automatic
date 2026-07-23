use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::{discover_mcp_servers_from_json, sync_individual_skills, Agent};

/// Claude Code agent — writes `.mcp.json` and stores skills under
/// `<project>/.claude/skills/<name>/SKILL.md`.
pub struct ClaudeCode;

impl Agent for ClaudeCode {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "claude"
    }

    fn label(&self) -> &'static str {
        "Claude Code"
    }

    fn config_description(&self) -> &'static str {
        ".mcp.json"
    }

    fn project_file_name(&self) -> &'static str {
        "CLAUDE.md"
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_in(&self, dir: &Path) -> bool {
        dir.join("CLAUDE.md").exists()
            || dir.join(".mcp.json").exists()
            || dir.join(".claude").join("settings.json").exists()
            || dir.join(".claude").join("skills").exists()
            || dir.join(".claude").join("commands").exists()
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".claude").join("skills")]
    }

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        // Claude Code uses Automatic's JSON format directly, with one tweak:
        // strip "type" from stdio entries for Claude Desktop backward-compat.
        let mut claude_servers = Map::new();

        for (name, config) in servers {
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
            claude_servers.insert(name.clone(), server);
        }

        let output = json!({ "mcpServers": Value::Object(claude_servers) });
        let path = dir.join(".mcp.json");
        let content =
            serde_json::to_string_pretty(&output).map_err(|e| format!("JSON error: {}", e))?;
        write_file_atomic(&path, content.as_bytes())
            .map_err(|e| format!("Failed to write .mcp.json: {}", e))?;

        Ok(path.display().to_string())
    }

    fn sync_skills(
        &self,
        dir: &Path,
        skill_contents: &[(String, String)],
        selected_names: &[String],
        local_skill_names: &[String],
    ) -> Result<Vec<String>, String> {
        let mut written = Vec::new();
        let skills_dir = dir.join(".claude").join("skills");
        sync_individual_skills(
            &skills_dir,
            skill_contents,
            selected_names,
            local_skill_names,
            &mut written,
        )?;
        Ok(written)
    }

    fn sync_instruction_rules(
        &self,
        project: &crate::core::Project,
        filename: &str,
        rule_names: &[String],
        custom_contents: &[String],
    ) -> Result<Option<Vec<String>>, String> {
        let opts = project
            .agent_options
            .get(self.id())
            .cloned()
            .unwrap_or_default();
        if !opts.claude_rules_in_dot_claude {
            return Ok(None);
        }

        let mut touched = Vec::new();
        if !rule_names.is_empty() {
            touched.extend(crate::core::sync_rules_to_dot_claude_rules(
                &project.directory,
                rule_names,
            )?);
        }

        if crate::core::inject_rules_into_project_file_with_custom(
            &project.directory,
            filename,
            &[],
            custom_contents,
        )? {
            touched.push(
                PathBuf::from(&project.directory)
                    .join(filename)
                    .display()
                    .to_string(),
            );
        }

        Ok(Some(touched))
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

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
        sync_claude_code_hooks(project_dir, hooks)
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        let path = dir.join(".mcp.json");
        if !path.exists() {
            return Map::new();
        }
        // Claude's format is already canonical — no normalisation needed.
        discover_mcp_servers_from_json(&path, "mcpServers", identity)
    }

    fn detect_global_install(&self) -> bool {
        // The `claude` binary on PATH, or the ~/.claude config directory.
        super::cli_available("claude")
            || super::home_dir()
                .map(|h| h.join(".claude").exists())
                .unwrap_or(false)
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        let Some(home) = super::home_dir() else {
            return Map::new();
        };

        // ~/.claude.json is the single source of truth for user-scoped MCP
        // servers in Claude Code.  The top-level `mcpServers` object holds
        // servers added with `claude mcp add --scope user`.
        //
        // Note: local-scoped servers (default scope / `--scope local`) live
        // under `projects["<abs-path>"]["mcpServers"]` in the same file,
        // keyed by absolute project path.  We don't read those here because
        // this method has no project-path context — they are project-specific
        // and would pollute every other project's import list if surfaced
        // globally.
        discover_claude_global_config(&home.join(".claude.json"))
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".claude").join("agents"))
    }

    fn commands_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join(".claude").join("commands"))
    }

    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        vec![dir.join(".mcp.json")]
    }
}

/// Read user-scoped MCP servers from Claude Code's `~/.claude.json`.
///
/// Claude Code stores MCP server configs at three scopes inside this file:
/// - User scope (`--scope user`): top-level `mcpServers` object
/// - Local scope (default / `--scope local`): `projects["<abs-path>"]["mcpServers"]`
///
/// This function reads only the top-level `mcpServers` — the user-scoped
/// entries that apply across all projects.  Per-project local-scope entries
/// are intentionally excluded because they are project-specific and we have
/// no project path context here.
fn discover_claude_global_config(path: &Path) -> Map<String, Value> {
    discover_mcp_servers_from_json(path, "mcpServers", identity)
}

/// Pass-through normaliser: Claude's format is already canonical.
fn identity(v: Value) -> Value {
    v
}

// ── Hooks ────────────────────────────────────────────────────────────────────
//
// Claude Code reads hooks from `.claude/settings.json` under the top-level
// `hooks` key. Each event maps to an array of matcher groups, and each
// matcher group has an array of handlers. We need to merge our entries into
// any pre-existing settings without disturbing keys the user owns (model,
// permissions, env, …) or hook entries the user wrote by hand.
//
// Ownership tagging: every handler we emit carries `_managedBy: "automatic"`
// and `_hookId: "<machine-name>"`. On every sync we drop existing handlers
// whose `_managedBy == "automatic"` before merging our fresh set in, so
// detach actually removes entries and resync never accumulates duplicates.
// Claude Code ignores unknown JSON fields, so the tags are inert at runtime.

const HOOK_MANAGED_KEY: &str = "_managedBy";
const HOOK_MANAGED_VALUE: &str = "automatic";
const HOOK_ID_KEY: &str = "_hookId";

fn sync_claude_code_hooks(
    project_dir: &Path,
    hooks: &[crate::core::Hook],
) -> Result<Vec<String>, String> {
    let claude_dir = project_dir.join(".claude");
    let settings_path = claude_dir.join("settings.json");
    let hooks_scripts_dir = claude_dir.join("hooks");

    // Always ensure .claude/ exists so we can write the settings file even on
    // a project that has never been touched by Claude Code.
    fs::create_dir_all(&claude_dir)
        .map_err(|e| format!("Failed to create .claude/: {}", e))?;

    let mut written = Vec::new();

    // Write any script-handler bodies first so the settings file can reference
    // them by path. The script directory is owned by Automatic in the sense
    // that we manage the files we put in it — but we don't blow it away here,
    // so a user-authored script next to ours stays intact.
    let mut hooks_dir_created = false;
    let mut managed_script_paths: Vec<PathBuf> = Vec::new();
    for hook in hooks {
        if let crate::core::HookHandler::Script {
            interpreter,
            script,
        } = &hook.handler
        {
            if !hooks_dir_created {
                fs::create_dir_all(&hooks_scripts_dir)
                    .map_err(|e| format!("Failed to create .claude/hooks/: {}", e))?;
                hooks_dir_created = true;
            }
            let script_path =
                super::write_managed_hook_script(&hooks_scripts_dir, hook, interpreter, script)?;
            managed_script_paths.push(script_path.clone());
            written.push(script_path.display().to_string());
        }
    }

    // Load or initialise the settings document.
    let mut settings: Value = if settings_path.exists() {
        let raw = fs::read_to_string(&settings_path).map_err(|e| {
            format!("Failed to read .claude/settings.json: {}", e)
        })?;
        if raw.trim().is_empty() {
            Value::Object(Map::new())
        } else {
            serde_json::from_str(&raw).map_err(|e| {
                format!(
                    "Failed to parse .claude/settings.json — fix the syntax or remove the file: {}",
                    e
                )
            })?
        }
    } else {
        Value::Object(Map::new())
    };

    // Strip every previously-managed handler from the hooks tree, then merge
    // our fresh set on top. Merging into a freshly stripped tree guarantees
    // re-sync is idempotent: the on-disk state after sync only depends on
    // the current hook set, not on prior sync history.
    let settings_obj = settings
        .as_object_mut()
        .ok_or_else(|| ".claude/settings.json must be a JSON object".to_string())?;

    let hooks_value = settings_obj
        .entry("hooks".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let hooks_obj = hooks_value
        .as_object_mut()
        .ok_or_else(|| "`hooks` in .claude/settings.json must be an object".to_string())?;

    drop_managed_handlers(hooks_obj);
    merge_managed_hooks(hooks_obj, hooks, &hooks_scripts_dir);

    // Drop any now-empty matcher groups / events so we don't leave skeletal
    // objects behind after detach.
    prune_empty_hook_entries(hooks_obj);
    if hooks_obj.is_empty() {
        settings_obj.remove("hooks");
    }

    let pretty =
        serde_json::to_string_pretty(&settings).map_err(|e| format!("JSON error: {}", e))?;
    fs::write(&settings_path, format!("{}\n", pretty))
        .map_err(|e| format!("Failed to write .claude/settings.json: {}", e))?;
    written.push(settings_path.display().to_string());

    // Remove any leftover managed script files from earlier syncs that no
    // longer correspond to a current hook. We identify ours by the
    // ".managed-by-automatic" marker file we drop alongside them; without it
    // we'd risk deleting a user-authored script in .claude/hooks/.
    cleanup_orphaned_managed_scripts(&hooks_scripts_dir, &managed_script_paths)?;

    Ok(written)
}

/// Stable identifier used both as the script filename stem and as the
/// `_hookId` tag.  Just the hook's display-machine-name (derived from the
/// `Hook` record's identity in the library). We don't have direct access to
/// the on-disk filename from the `Hook` value, so use a normalised version of
/// the display name as a fallback.
fn hook_machine_id(hook: &crate::core::Hook) -> String {
    super::hook_slug(hook)
}

fn script_extension_for_interpreter(interpreter: &str) -> &'static str {
    super::hook_script_extension(interpreter)
}

/// Remove every handler in `hooks_obj` that carries the managed-by-automatic
/// tag, leaving user-authored handlers untouched.
fn drop_managed_handlers(hooks_obj: &mut Map<String, Value>) {
    for event_value in hooks_obj.values_mut() {
        let Some(groups) = event_value.as_array_mut() else {
            continue;
        };
        for group in groups.iter_mut() {
            let Some(group_obj) = group.as_object_mut() else {
                continue;
            };
            let Some(handlers) = group_obj.get_mut("hooks").and_then(|h| h.as_array_mut()) else {
                continue;
            };
            handlers.retain(|handler| {
                !handler
                    .get(HOOK_MANAGED_KEY)
                    .and_then(|v| v.as_str())
                    .map(|s| s == HOOK_MANAGED_VALUE)
                    .unwrap_or(false)
            });
        }
    }
}

/// Insert managed handlers grouped by `(event, matcher)` so each unique
/// matcher under a given event becomes a single group entry, matching the
/// shape Claude Code expects.
fn merge_managed_hooks(
    hooks_obj: &mut Map<String, Value>,
    hooks: &[crate::core::Hook],
    scripts_dir: &Path,
) {
    use std::collections::BTreeMap;

    // Group by event → matcher → handlers, sorted so the output is stable.
    type HandlersByMatcher = BTreeMap<Option<String>, Vec<(String, Value)>>;
    let mut grouped: BTreeMap<String, HandlersByMatcher> = BTreeMap::new();

    for hook in hooks {
        let handler = build_handler_value(hook, scripts_dir);
        grouped
            .entry(hook.event.clone())
            .or_default()
            .entry(hook.matcher.clone())
            .or_default()
            .push((hook_machine_id(hook), handler));
    }

    for (event, matchers) in grouped {
        let event_entry = hooks_obj
            .entry(event.clone())
            .or_insert_with(|| Value::Array(Vec::new()));
        let Some(event_arr) = event_entry.as_array_mut() else {
            continue;
        };

        for (matcher, handlers) in matchers {
            // Try to find an existing matcher group with the same matcher
            // value so user-authored handlers and ours can coexist in one
            // group.  Otherwise append a new group.
            let existing_idx = event_arr.iter().position(|group| {
                let group_matcher = group.get("matcher").and_then(|v| v.as_str());
                match (&matcher, group_matcher) {
                    (Some(m), Some(g)) => m == g,
                    (None, None) => true,
                    _ => false,
                }
            });

            if let Some(idx) = existing_idx {
                if let Some(group_obj) = event_arr[idx].as_object_mut() {
                    let group_hooks = group_obj
                        .entry("hooks".to_string())
                        .or_insert_with(|| Value::Array(Vec::new()));
                    if let Some(arr) = group_hooks.as_array_mut() {
                        for (_, handler) in handlers {
                            arr.push(handler);
                        }
                    }
                }
            } else {
                let mut group = Map::new();
                if let Some(m) = matcher {
                    group.insert("matcher".to_string(), Value::String(m));
                }
                let handler_arr: Vec<Value> = handlers.into_iter().map(|(_, v)| v).collect();
                group.insert("hooks".to_string(), Value::Array(handler_arr));
                event_arr.push(Value::Object(group));
            }
        }
    }
}

fn build_handler_value(hook: &crate::core::Hook, scripts_dir: &Path) -> Value {
    let mut handler = Map::new();
    handler.insert("type".to_string(), Value::String("command".to_string()));

    let command_str = match &hook.handler {
        crate::core::HookHandler::Command { command } => command.clone(),
        crate::core::HookHandler::Script { interpreter, .. } => {
            // Reference the script via ${CLAUDE_PROJECT_DIR} so the
            // settings file is portable across machines / containers.
            let ext = script_extension_for_interpreter(interpreter);
            let file_name = format!("{}.{}", hook_machine_id(hook), ext);
            let _ = scripts_dir; // silence unused-warning; kept for clarity
            format!("${{CLAUDE_PROJECT_DIR}}/.claude/hooks/{}", file_name)
        }
        crate::core::HookHandler::Path { path, .. } => {
            // The user owns the file. Pass the path straight through —
            // it may already contain ${CLAUDE_PROJECT_DIR} or similar
            // placeholders that Claude Code expands at run time.
            path.clone()
        }
    };
    handler.insert("command".to_string(), Value::String(command_str));

    if let Some(timeout) = hook.timeout_sec {
        handler.insert(
            "timeout".to_string(),
            Value::Number(serde_json::Number::from(timeout)),
        );
    }

    handler.insert(
        HOOK_MANAGED_KEY.to_string(),
        Value::String(HOOK_MANAGED_VALUE.to_string()),
    );
    handler.insert(
        HOOK_ID_KEY.to_string(),
        Value::String(hook_machine_id(hook)),
    );

    Value::Object(handler)
}

/// Remove empty groups and events left behind after `drop_managed_handlers`
/// emptied them out.
fn prune_empty_hook_entries(hooks_obj: &mut Map<String, Value>) {
    let mut empty_events = Vec::new();
    for (event, value) in hooks_obj.iter_mut() {
        let Some(groups) = value.as_array_mut() else {
            continue;
        };
        groups.retain(|group| {
            group
                .get("hooks")
                .and_then(|h| h.as_array())
                .map(|arr| !arr.is_empty())
                .unwrap_or(false)
        });
        if groups.is_empty() {
            empty_events.push(event.clone());
        }
    }
    for event in empty_events {
        hooks_obj.remove(&event);
    }
}

/// Delete leftover managed script files in `.claude/hooks/` — see
/// [`super::cleanup_managed_hook_scripts`].
fn cleanup_orphaned_managed_scripts(
    scripts_dir: &Path,
    keep_paths: &[PathBuf],
) -> Result<(), String> {
    super::cleanup_managed_hook_scripts(scripts_dir, keep_paths)
}

/// Writes `content` to `final_path` via a temp file + rename, so a crash or
/// kill mid-write can never leave `.mcp.json` truncated or corrupt — which
/// would otherwise drop every configured MCP server for the project, not
/// just the one being synced.
fn write_file_atomic(final_path: &Path, content: &[u8]) -> std::io::Result<()> {
    let mut tmp_name = final_path.as_os_str().to_os_string();
    tmp_name.push(".tmp");
    let tmp_path = PathBuf::from(tmp_name);

    // Remove a stale tmp left behind by a previous crash, if any.
    let _ = fs::remove_file(&tmp_path);

    {
        let mut tmp = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        std::io::Write::write_all(&mut tmp, content)?;
        tmp.sync_all()?;
    } // file handle closed before rename

    fs::rename(&tmp_path, final_path).inspect_err(|_| {
        // Best-effort cleanup so a failed rename doesn't leave litter.
        let _ = fs::remove_file(&tmp_path);
    })
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
        assert!(!ClaudeCode.detect_in(dir.path()));

        fs::write(dir.path().join("CLAUDE.md"), "# Claude").unwrap();
        assert!(ClaudeCode.detect_in(dir.path()));

        fs::remove_file(dir.path().join("CLAUDE.md")).unwrap();
        fs::write(dir.path().join(".mcp.json"), "{}").unwrap();
        assert!(ClaudeCode.detect_in(dir.path()));
    }

    #[test]
    fn test_write_stdio() {
        let dir = tempdir().unwrap();
        ClaudeCode
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // stdio entries should have "type" stripped
        assert!(parsed["mcpServers"]["automatic"]["type"].is_null());
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
    fn test_write_http() {
        let dir = tempdir().unwrap();
        ClaudeCode
            .write_mcp_config(dir.path(), &http_servers())
            .unwrap();

        let content = fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            parsed["mcpServers"]["remote-api"]["type"].as_str().unwrap(),
            "http"
        );
        assert_eq!(
            parsed["mcpServers"]["remote-api"]["url"].as_str().unwrap(),
            "https://api.example.com/mcp"
        );
        assert!(
            parsed["mcpServers"]["remote-api"]["headers"]["Authorization"]
                .as_str()
                .is_some()
        );
        assert_eq!(
            parsed["mcpServers"]["remote-api"]["oauth"]["clientId"]
                .as_str()
                .unwrap(),
            "client_123"
        );
    }

    #[test]
    fn test_write_mcp_config_leaves_no_tmp_file_and_overwrites_cleanly() {
        let dir = tempdir().unwrap();
        ClaudeCode
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();
        ClaudeCode
            .write_mcp_config(dir.path(), &http_servers())
            .unwrap();

        assert!(!dir.path().join(".mcp.json.tmp").exists());
        let content = fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert!(parsed["mcpServers"]["remote-api"].is_object());
        assert!(parsed["mcpServers"]["automatic"].is_null());
    }

    #[test]
    fn test_write_mcp_config_recovers_from_stale_tmp_file() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".mcp.json.tmp"), "leftover from a crash").unwrap();

        ClaudeCode
            .write_mcp_config(dir.path(), &stdio_servers())
            .unwrap();

        assert!(!dir.path().join(".mcp.json.tmp").exists());
        let content = fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert!(parsed["mcpServers"]["automatic"]["command"].is_string());
    }

    // ── discover_claude_global_config tests ─────────────────────────────────

    #[test]
    fn test_discover_global_missing_file() {
        let dir = tempdir().unwrap();
        // No ~/.claude.json — should return empty map, not panic.
        let result = discover_claude_global_config(&dir.path().join(".claude.json"));
        assert!(result.is_empty());
    }

    #[test]
    fn test_discover_global_user_scoped_stdio() {
        // Simulates `claude mcp add --scope user my-server -- npx -y @foo/bar`
        let dir = tempdir().unwrap();
        let claude_json = json!({
            "numStartups": 5,
            "mcpServers": {
                "github": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-github"],
                    "env": { "GITHUB_TOKEN": "ghp_test" }
                }
            },
            "projects": {}
        });
        fs::write(
            dir.path().join(".claude.json"),
            serde_json::to_string(&claude_json).unwrap(),
        )
        .unwrap();

        let result = discover_claude_global_config(&dir.path().join(".claude.json"));

        assert!(
            result.contains_key("github"),
            "should find user-scoped server"
        );
        assert_eq!(result["github"]["command"].as_str().unwrap(), "npx");
        assert_eq!(result["github"]["args"][0].as_str().unwrap(), "-y");
        assert_eq!(
            result["github"]["env"]["GITHUB_TOKEN"].as_str().unwrap(),
            "ghp_test"
        );
    }

    #[test]
    fn test_discover_global_user_scoped_http() {
        // Simulates `claude mcp add --scope user --transport http sentry https://mcp.sentry.dev/mcp`
        let dir = tempdir().unwrap();
        let claude_json = json!({
            "mcpServers": {
                "sentry": {
                    "type": "http",
                    "url": "https://mcp.sentry.dev/mcp"
                }
            }
        });
        fs::write(
            dir.path().join(".claude.json"),
            serde_json::to_string(&claude_json).unwrap(),
        )
        .unwrap();

        let result = discover_claude_global_config(&dir.path().join(".claude.json"));

        assert!(result.contains_key("sentry"));
        assert_eq!(result["sentry"]["type"].as_str().unwrap(), "http");
        assert_eq!(
            result["sentry"]["url"].as_str().unwrap(),
            "https://mcp.sentry.dev/mcp"
        );
    }

    #[test]
    fn test_discover_global_automatic_server_skipped() {
        // The "automatic" entry must always be injected fresh at sync time —
        // it must never be imported from an existing config.
        let dir = tempdir().unwrap();
        let claude_json = json!({
            "mcpServers": {
                "automatic": {
                    "command": "/old/path/to/nexus",
                    "args": ["mcp-serve"]
                },
                "github": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-github"]
                }
            }
        });
        fs::write(
            dir.path().join(".claude.json"),
            serde_json::to_string(&claude_json).unwrap(),
        )
        .unwrap();

        let result = discover_claude_global_config(&dir.path().join(".claude.json"));

        assert!(
            !result.contains_key("automatic"),
            "automatic server must be filtered out"
        );
        assert!(
            result.contains_key("github"),
            "other servers should be kept"
        );
    }

    // ── Hook sync ───────────────────────────────────────────────────────────

    fn cmd_hook(name: &str, event: &str, matcher: Option<&str>, command: &str) -> crate::core::Hook {
        crate::core::Hook {
            name: name.to_string(),
            agent: "claude".to_string(),
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
            agent: "claude".to_string(),
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
            agent: "claude".to_string(),
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

    #[test]
    fn hook_sync_merges_into_empty_settings() {
        let dir = tempdir().unwrap();
        let hooks = vec![cmd_hook(
            "log-session",
            "SessionStart",
            None,
            "echo session started",
        )];

        let written = ClaudeCode.sync_hooks(dir.path(), &hooks).expect("sync");
        let settings_path = dir.path().join(".claude/settings.json");
        assert!(settings_path.exists());
        assert!(written.iter().any(|p| p.ends_with("settings.json")));

        let raw = fs::read_to_string(&settings_path).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let groups = v["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(groups.len(), 1);
        let handlers = groups[0]["hooks"].as_array().unwrap();
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0]["type"], "command");
        assert_eq!(handlers[0]["command"], "echo session started");
        assert_eq!(handlers[0]["_managedBy"], "automatic");
        assert_eq!(handlers[0]["_hookId"], "log-session");
    }

    #[test]
    fn hook_sync_preserves_unrelated_settings_keys() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let preexisting = json!({
            "model": "claude-opus-4-7",
            "permissions": { "allow": ["Bash(npm test)"] }
        });
        fs::write(
            dir.path().join(".claude/settings.json"),
            serde_json::to_string_pretty(&preexisting).unwrap(),
        )
        .unwrap();

        let hooks = vec![cmd_hook("ping", "Stop", None, "echo bye")];
        ClaudeCode.sync_hooks(dir.path(), &hooks).expect("sync");

        let raw = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["model"], "claude-opus-4-7");
        assert_eq!(v["permissions"]["allow"][0], "Bash(npm test)");
        assert!(v["hooks"]["Stop"].is_array());
    }

    #[test]
    fn hook_sync_preserves_user_written_hooks_in_same_event() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let preexisting = json!({
            "hooks": {
                "SessionStart": [{
                    "hooks": [{
                        "type": "command",
                        "command": "echo user-owned"
                    }]
                }]
            }
        });
        fs::write(
            dir.path().join(".claude/settings.json"),
            serde_json::to_string_pretty(&preexisting).unwrap(),
        )
        .unwrap();

        let hooks = vec![cmd_hook(
            "managed",
            "SessionStart",
            None,
            "echo managed",
        )];
        ClaudeCode.sync_hooks(dir.path(), &hooks).expect("sync");

        let raw = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let handlers = v["hooks"]["SessionStart"][0]["hooks"].as_array().unwrap();
        let commands: Vec<&str> = handlers
            .iter()
            .map(|h| h["command"].as_str().unwrap())
            .collect();
        assert!(commands.contains(&"echo user-owned"), "user hook removed: {:?}", commands);
        assert!(commands.contains(&"echo managed"), "managed hook missing: {:?}", commands);
    }

    #[test]
    fn hook_sync_is_idempotent_across_repeats() {
        let dir = tempdir().unwrap();
        let hooks = vec![
            cmd_hook("a", "PreToolUse", Some("Bash"), "echo a"),
            cmd_hook("b", "PreToolUse", Some("Edit"), "echo b"),
        ];
        ClaudeCode.sync_hooks(dir.path(), &hooks).unwrap();
        ClaudeCode.sync_hooks(dir.path(), &hooks).unwrap();

        let raw = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let groups = v["hooks"]["PreToolUse"].as_array().unwrap();
        // One group per distinct matcher, each with exactly one handler.
        assert_eq!(groups.len(), 2);
        for group in groups {
            assert_eq!(group["hooks"].as_array().unwrap().len(), 1);
        }
    }

    #[test]
    fn hook_sync_removes_managed_entries_when_detached() {
        let dir = tempdir().unwrap();
        let initial = vec![cmd_hook("transient", "Stop", None, "echo before")];
        ClaudeCode.sync_hooks(dir.path(), &initial).unwrap();

        // Re-sync with the hook removed — managed entry should vanish, and
        // because nothing else is left under "Stop", the event key should be
        // pruned, and because nothing's left under "hooks", that key too.
        ClaudeCode.sync_hooks(dir.path(), &[]).unwrap();

        let raw = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        assert!(v.get("hooks").is_none(), "hooks key not pruned: {:?}", v);
    }

    #[test]
    fn hook_sync_writes_script_file_and_references_it() {
        let dir = tempdir().unwrap();
        let hooks = vec![script_hook(
            "logger",
            "SessionStart",
            "bash",
            "echo $CLAUDE_PROJECT_DIR\n",
        )];
        ClaudeCode.sync_hooks(dir.path(), &hooks).unwrap();

        let script_path = dir.path().join(".claude/hooks/logger.sh");
        assert!(script_path.exists(), "script file not written");
        let body = fs::read_to_string(&script_path).unwrap();
        assert!(body.starts_with("#!/usr/bin/env bash"));
        assert!(body.contains("managed-by-automatic"));
        assert!(body.contains("echo $CLAUDE_PROJECT_DIR"));

        let raw = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let handler = &v["hooks"]["SessionStart"][0]["hooks"][0];
        assert_eq!(
            handler["command"].as_str().unwrap(),
            "${CLAUDE_PROJECT_DIR}/.claude/hooks/logger.sh"
        );
        assert_eq!(handler["timeout"], 60);
    }

    #[test]
    fn hook_sync_path_handler_references_user_owned_file() {
        let dir = tempdir().unwrap();
        // User points at a script that already lives in their repo.
        let hooks = vec![path_hook(
            "logger",
            "SessionStart",
            "${CLAUDE_PROJECT_DIR}/scripts/log-session.sh",
        )];
        ClaudeCode.sync_hooks(dir.path(), &hooks).unwrap();

        // Automatic must NOT have created a script file under .claude/hooks/
        // — the user owns that file.
        assert!(!dir.path().join(".claude/hooks").exists());

        let raw = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let handler = &v["hooks"]["SessionStart"][0]["hooks"][0];
        assert_eq!(
            handler["command"].as_str().unwrap(),
            "${CLAUDE_PROJECT_DIR}/scripts/log-session.sh"
        );
    }

    #[test]
    fn hook_sync_cleans_orphaned_managed_scripts_only() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".claude/hooks")).unwrap();
        // A user-authored script next to our managed ones — must survive.
        fs::write(
            dir.path().join(".claude/hooks/user-script.sh"),
            "#!/usr/bin/env bash\necho user only\n",
        )
        .unwrap();

        // First sync writes a managed script.
        ClaudeCode
            .sync_hooks(
                dir.path(),
                &[script_hook("temp", "Stop", "bash", "echo temp\n")],
            )
            .unwrap();
        assert!(dir.path().join(".claude/hooks/temp.sh").exists());

        // Re-sync without that hook drops the managed file but keeps user one.
        ClaudeCode.sync_hooks(dir.path(), &[]).unwrap();
        assert!(!dir.path().join(".claude/hooks/temp.sh").exists());
        assert!(dir.path().join(".claude/hooks/user-script.sh").exists());
    }

    #[test]
    fn test_discover_global_local_scoped_not_imported() {
        // Local-scoped servers live under projects["<path>"]["mcpServers"].
        // They must NOT be surfaced by discover_global — they are
        // project-specific and have no meaning outside that project.
        let dir = tempdir().unwrap();
        let claude_json = json!({
            "mcpServers": {
                "user-tool": { "command": "npx", "args": ["-y", "user-tool"] }
            },
            "projects": {
                "/Users/someone/my-project": {
                    "mcpServers": {
                        "local-only-tool": {
                            "command": "npx",
                            "args": ["-y", "local-tool"]
                        }
                    }
                }
            }
        });
        fs::write(
            dir.path().join(".claude.json"),
            serde_json::to_string(&claude_json).unwrap(),
        )
        .unwrap();

        let result = discover_claude_global_config(&dir.path().join(".claude.json"));

        assert!(
            result.contains_key("user-tool"),
            "user-scoped server should be present"
        );
        assert!(
            !result.contains_key("local-only-tool"),
            "local-scoped server must not be imported globally"
        );
    }
}
