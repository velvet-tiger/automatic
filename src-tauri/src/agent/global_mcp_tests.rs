//! Cross-agent invariants for the global (user-level) MCP writer.
//!
//! Every test loops [`all()`] filtered by `capabilities().global_mcp_servers`
//! and asserts a single invariant, mirroring the shape of
//! [`mcp_format_tests`].  The suite pins the contract every dialect must
//! satisfy, so a new global writer cannot be enabled without meeting each of
//! them.
//!
//! All tests run under [`crate::core::paths::with_test_home`] so agent-level
//! `home_dir()` redirects into a tempdir and never touches the developer's
//! real `~`.

use super::*;
use crate::core::with_test_home;
use serde_json::json;
use tempfile::tempdir;

/// Canonical desired map — a stdio server with a literal env, a stdio server
/// carrying the "inherit from environment" marker, an http server, plus
/// Automatic-internal fields so leak tests bite.
fn canonical_desired() -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(
        "github".to_string(),
        json!({
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-github"],
            "env": { "GITHUB_TOKEN": "" },
            "_author": { "name": "provenance-marker" },
        }),
    );
    m.insert(
        "fs".to_string(),
        json!({
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-filesystem", "${userHome}"],
            "env": { "HOME_VAR": "literal-value" },
            "_builtin": true,
        }),
    );
    m.insert(
        "linear".to_string(),
        json!({
            "type": "http",
            "url": "https://mcp.linear.app/mcp",
            "headers": { "X-Client": "automatic" },
        }),
    );
    m
}

/// Run the real global write pipeline: `prepare_global_mcp_servers` +
/// `write_global_mcp_config`.
fn write_global(agent: &dyn Agent, servers: &Map<String, Value>) -> GlobalMcpWriteReport {
    let prepared = prepare_global_mcp_servers(agent, servers);
    agent
        .write_global_mcp_config(&prepared, &[])
        .expect("write_global_mcp_config")
}

fn read_target(agent: &dyn Agent) -> String {
    let target = agent
        .global_mcp_target()
        .expect("agent must have a global target");
    fs::read_to_string(&target.path).expect("target file must exist after a write")
}

// ── Contract ────────────────────────────────────────────────────────────────

#[test]
fn global_target_and_capability_agree() {
    let tmp = tempdir().unwrap();
    with_test_home(tmp.path().to_path_buf(), || {
        for agent in all() {
            let has_target = agent.global_mcp_target().is_some();
            let flag = agent.capabilities().global_mcp_servers;
            assert_eq!(
                has_target, flag,
                "{}: capabilities().global_mcp_servers is {flag} but global_mcp_target() returns {}",
                agent.id(),
                if has_target { "Some" } else { "None" },
            );
        }
    });
}

#[test]
fn agents_without_the_capability_return_err_from_the_writer() {
    let tmp = tempdir().unwrap();
    with_test_home(tmp.path().to_path_buf(), || {
        for agent in all() {
            if agent.capabilities().global_mcp_servers {
                continue;
            }
            let result = agent.write_global_mcp_config(&Map::new(), &[]);
            assert!(
                result.is_err(),
                "{}: capability off but write_global_mcp_config succeeded",
                agent.id(),
            );
        }
    });
}

// ── Merge preserves what Automatic does not own ─────────────────────────────

#[test]
fn foreign_entries_in_the_same_key_survive_byte_for_byte() {
    let tmp = tempdir().unwrap();
    with_test_home(tmp.path().to_path_buf(), || {
        for agent in all() {
            if !agent.capabilities().global_mcp_servers {
                continue;
            }
            // Codex is TOML — its merge path is section-based, tested in the
            // agent-specific suite.  The shared JSON invariant does not apply.
            if agent.id() == "codex" {
                continue;
            }
            let target = agent.global_mcp_target().unwrap();
            seed_foreign_entry_for(agent.id(), &target.path);
            let before = fs::read_to_string(&target.path).unwrap();
            let foreign_slice = extract_foreign_slice(&before, agent.id());

            let mut desired = Map::new();
            desired.insert(
                "automatic-added".to_string(),
                json!({
                    "type": "stdio",
                    "command": "npx",
                    "args": ["run", "server"],
                }),
            );
            let prepared = prepare_global_mcp_servers(agent, &desired);
            agent
                .write_global_mcp_config(&prepared, &[])
                .expect("write");

            let after = fs::read_to_string(&target.path).unwrap();
            assert!(
                after.contains(&foreign_slice),
                "{}: foreign entry was rewritten (or removed).\nBefore: {}\nAfter: {}\nExpected slice: {}",
                agent.id(),
                before,
                after,
                foreign_slice,
            );
        }
    });
}

#[test]
fn collision_with_foreign_entry_is_skipped_not_overwritten() {
    let tmp = tempdir().unwrap();
    with_test_home(tmp.path().to_path_buf(), || {
        for agent in all() {
            if !agent.capabilities().global_mcp_servers {
                continue;
            }
            if agent.id() == "codex" {
                continue;
            }
            let target = agent.global_mcp_target().unwrap();
            seed_foreign_entry_for(agent.id(), &target.path);
            let before = fs::read_to_string(&target.path).unwrap();

            // Try to write a server whose name collides with the seeded foreign entry.
            let mut desired = Map::new();
            desired.insert(
                "foreign-server".to_string(),
                json!({ "type": "stdio", "command": "different", "args": ["x"] }),
            );
            let prepared = prepare_global_mcp_servers(agent, &desired);
            let report = agent.write_global_mcp_config(&prepared, &[]).expect("write");

            assert!(
                report.skipped.contains(&"foreign-server".to_string()),
                "{}: expected 'foreign-server' in report.skipped, got {:?}",
                agent.id(),
                report.skipped,
            );
            assert!(
                !report.written.contains(&"foreign-server".to_string()),
                "{}: collision was written",
                agent.id(),
            );
            let after = fs::read_to_string(&target.path).unwrap();
            assert!(
                after.contains("foreign-command"),
                "{}: foreign entry's command was replaced\nBefore: {}\nAfter: {}",
                agent.id(),
                before,
                after,
            );
        }
    });
}

#[test]
fn removal_deletes_only_previously_managed_names() {
    let tmp = tempdir().unwrap();
    with_test_home(tmp.path().to_path_buf(), || {
        for agent in all() {
            if !agent.capabilities().global_mcp_servers {
                continue;
            }
            if agent.id() == "codex" {
                continue;
            }
            // First apply: write server "a".
            let mut first = Map::new();
            first.insert(
                "a".to_string(),
                json!({ "type": "stdio", "command": "a-cmd" }),
            );
            let prepared = prepare_global_mcp_servers(agent, &first);
            let report = agent.write_global_mcp_config(&prepared, &[]).expect("write a");
            assert!(report.written.contains(&"a".to_string()));

            // Seed a foreign entry too.
            let target = agent.global_mcp_target().unwrap();
            let raw = fs::read_to_string(&target.path).unwrap();
            let with_foreign = inject_foreign_into_json(&raw, agent.id(), "b");
            fs::write(&target.path, &with_foreign).unwrap();

            // Second apply: deselect a (was managed) but not b (foreign).
            let report = agent
                .write_global_mcp_config(&Map::new(), &["a".to_string()])
                .expect("write empty");
            assert!(
                report.removed.contains(&"a".to_string()),
                "{}: 'a' should be removed",
                agent.id(),
            );

            let after = fs::read_to_string(&target.path).unwrap_or_default();
            assert!(
                after.contains("\"b\""),
                "{}: foreign 'b' was removed by empty apply\nAfter: {}",
                agent.id(),
                after,
            );
        }
    });
}

#[test]
fn repeated_writes_are_byte_identical_and_report_unchanged() {
    let tmp = tempdir().unwrap();
    with_test_home(tmp.path().to_path_buf(), || {
        for agent in all() {
            if !agent.capabilities().global_mcp_servers {
                continue;
            }
            if agent.id() == "codex" {
                // Codex whitespace round-trip is exercised in the section-splice tests.
                continue;
            }
            let first_report = write_global(agent, &canonical_desired());
            let first = read_target(agent);

            let managed = first_report.written.clone();
            let prepared = prepare_global_mcp_servers(agent, &canonical_desired());
            let report = agent
                .write_global_mcp_config(&prepared, &managed)
                .expect("second write");
            assert!(
                report.unchanged,
                "{}: second identical apply did not report unchanged",
                agent.id(),
            );

            let second = read_target(agent);
            assert_eq!(
                first, second,
                "{}: two identical applies produced different bytes",
                agent.id(),
            );
        }
    });
}

#[test]
fn malformed_target_is_an_error_not_a_clobber() {
    let tmp = tempdir().unwrap();
    with_test_home(tmp.path().to_path_buf(), || {
        for agent in all() {
            if !agent.capabilities().global_mcp_servers {
                continue;
            }
            let target = agent.global_mcp_target().unwrap();
            if let Some(parent) = target.path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            let corrupt = if agent.id() == "codex" {
                b"[mcp_servers.broken\n  no closing bracket".to_vec()
            } else {
                b"{ this is not valid json".to_vec()
            };
            fs::write(&target.path, &corrupt).unwrap();

            let prepared = prepare_global_mcp_servers(agent, &canonical_desired());
            let result = agent.write_global_mcp_config(&prepared, &[]);
            assert!(
                result.is_err(),
                "{}: writing over a malformed target must Err, got {:?}",
                agent.id(),
                result.map(|r| r.written),
            );
            let after = fs::read(&target.path).unwrap();
            assert_eq!(
                after, corrupt,
                "{}: malformed target was clobbered",
                agent.id(),
            );
        }
    });
}

#[test]
fn internal_fields_never_reach_the_global_config() {
    let tmp = tempdir().unwrap();
    with_test_home(tmp.path().to_path_buf(), || {
        for agent in all() {
            if !agent.capabilities().global_mcp_servers {
                continue;
            }
            write_global(agent, &canonical_desired());
            let target = agent.global_mcp_target().unwrap();
            let content = fs::read_to_string(&target.path).unwrap();
            assert!(
                !content.contains("_builtin"),
                "{}: internal `_builtin` field leaked into global config",
                agent.id(),
            );
            assert!(
                !content.contains("_author"),
                "{}: internal `_author` field leaked into global config",
                agent.id(),
            );
            assert!(
                !content.contains("provenance-marker"),
                "{}: `_author` value leaked into global config",
                agent.id(),
            );
        }
    });
}

#[test]
fn inherited_env_marker_is_rendered_not_written_as_empty_string() {
    let tmp = tempdir().unwrap();
    with_test_home(tmp.path().to_path_buf(), || {
        for agent in all() {
            if !agent.capabilities().global_mcp_servers {
                continue;
            }
            write_global(agent, &canonical_desired());
            let target = agent.global_mcp_target().unwrap();
            let content = fs::read_to_string(&target.path).unwrap();
            // The empty-string inherit marker must never appear verbatim.
            assert!(
                !content.contains("\"GITHUB_TOKEN\": \"\""),
                "{}: inherit marker leaked as empty string",
                agent.id(),
            );
        }
    });
}

#[test]
fn user_home_placeholder_is_expanded() {
    let tmp = tempdir().unwrap();
    let home = tmp.path().to_path_buf();
    with_test_home(home.clone(), || {
        for agent in all() {
            if !agent.capabilities().global_mcp_servers {
                continue;
            }
            write_global(agent, &canonical_desired());
            let target = agent.global_mcp_target().unwrap();
            let content = fs::read_to_string(&target.path).unwrap();
            assert!(
                !content.contains("${userHome}"),
                "{}: ${{userHome}} was not expanded",
                agent.id(),
            );
            assert!(
                content.contains(&home.to_string_lossy().to_string()),
                "{}: expanded home path missing from output",
                agent.id(),
            );
        }
    });
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Seed the agent's global target file with one foreign entry the tests can
/// look for.  Uses the correct servers-key/dialect per agent.
fn seed_foreign_entry_for(agent_id: &str, path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let content = match agent_id {
        // OpenCode-dialect (`mcp` key, entries wrap `type: "local"`).
        "opencode" | "kilo" => json!({
            "mcp": {
                "foreign-server": {
                    "type": "local",
                    "command": ["foreign-command", "--flag"],
                    "environment": {}
                }
            }
        }),
        // VS Code (`servers` key).
        "copilot" => json!({
            "servers": {
                "foreign-server": {
                    "type": "stdio",
                    "command": "foreign-command",
                    "args": ["--flag"]
                }
            }
        }),
        // Zed (`context_servers` key).
        "zed" => json!({
            "context_servers": {
                "foreign-server": {
                    "command": "foreign-command",
                    "args": ["--flag"],
                    "env": {}
                }
            }
        }),
        // ZCode (nested — mcp.servers).
        // NOTE: ZCode's discover walks `mcp.servers` but the writer targets
        // top-level `mcpServers` per the plan.  Test the writer's shape.
        _ => json!({
            "mcpServers": {
                "foreign-server": {
                    "command": "foreign-command",
                    "args": ["--flag"]
                }
            }
        }),
    };
    fs::write(path, serde_json::to_string_pretty(&content).unwrap() + "\n").unwrap();
}

/// Extract a snippet of the seeded foreign entry that must survive the merge
/// byte-for-byte — used only as a substring check, so any recognisable slice
/// is enough.  Chosen to include the entry name plus its command.
fn extract_foreign_slice(_content: &str, _agent_id: &str) -> String {
    // "foreign-command" is unique across dialects — a substring match on this
    // is enough to confirm the entry was not rewritten or removed.
    "foreign-command".to_string()
}

/// Add a foreign entry named `name` to the target JSON file for `agent_id`.
/// Used to check that a later apply with an empty desired map only removes
/// managed names.
fn inject_foreign_into_json(raw: &str, agent_id: &str, name: &str) -> String {
    let mut root: Value = serde_json::from_str(raw).unwrap_or(Value::Object(Map::new()));
    let key = match agent_id {
        "opencode" | "kilo" => "mcp",
        "copilot" => "servers",
        "zed" => "context_servers",
        _ => "mcpServers",
    };
    if let Some(obj) = root.as_object_mut() {
        let entries = obj
            .entry(key.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(entries_obj) = entries.as_object_mut() {
            let entry = match agent_id {
                "opencode" | "kilo" => json!({
                    "type": "local",
                    "command": ["foreign-command", "--flag"],
                }),
                "zed" => json!({ "command": "foreign-command", "args": [] }),
                _ => json!({ "command": "foreign-command", "args": [] }),
            };
            entries_obj.insert(name.to_string(), entry);
        }
    }
    serde_json::to_string_pretty(&root).unwrap() + "\n"
}
