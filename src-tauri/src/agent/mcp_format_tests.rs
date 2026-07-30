//! Cross-agent MCP configuration format tests.
//!
//! Every agent writes MCP servers in its own dialect: `mcpServers` in a
//! dedicated file (Claude Code, Cursor, Junie, Kiro, Kilo Code, Droid, Pi),
//! `mcpServers` merged into a shared settings file (Gemini CLI), `servers` in
//! `.vscode/mcp.json` (GitHub Copilot), `context_servers` in
//! `.zed/settings.json` (Zed), `mcp` in `opencode.json`, TOML for Codex CLI,
//! and nothing at all for the agents that keep MCP config outside the project
//! (Cline, Goose, Warp, Antigravity).
//!
//! The table-driven tests pin the invariants that must hold for *every*
//! dialect, so a new agent cannot be registered in [`all()`] without
//! satisfying them.  The per-agent tests below them pin the details that
//! legitimately differ.

use super::*;
use serde_json::json;
use tempfile::tempdir;

/// A canonical server map in the shape `build_selected_servers` produces: a
/// stdio entry with a literal env value, a stdio entry carrying the
/// empty-string "inherit from the environment" marker, and a remote entry.
/// Automatic-internal fields are included so leak tests have something to
/// catch.
fn canonical_servers() -> Map<String, Value> {
    let mut servers = Map::new();
    servers.insert(
        "automatic".to_string(),
        json!({
            "command": "/usr/local/bin/automatic",
            "args": ["mcp-serve"],
            "env": { "AUTOMATIC_PROJECT": "demo" },
            "_builtin": true,
        }),
    );
    servers.insert(
        "github".to_string(),
        json!({
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-github"],
            "env": { "GITHUB_TOKEN": "" },
            "_author": { "name": "provenance-marker" },
        }),
    );
    servers.insert(
        "linear".to_string(),
        json!({
            "type": "http",
            "url": "https://mcp.linear.app/mcp",
            "headers": { "X-Client": "automatic" },
        }),
    );
    servers
}

/// Run a server map through the real write pipeline — `prepare_mcp_servers`
/// then `write_mcp_config`, exactly as `sync::engine` and `sync::drift` do —
/// and return the resulting file contents.  `None` means the agent stores MCP
/// config outside the project and writes nothing.
fn write_and_read(agent: &dyn Agent, dir: &Path, servers: &Map<String, Value>) -> Option<String> {
    let prepared = prepare_mcp_servers(agent, servers);
    let path = agent.write_mcp_config(dir, &prepared).expect("write");
    if path.is_empty() {
        return None;
    }
    Some(fs::read_to_string(&path).expect("written config should be readable"))
}

fn write_canonical(agent: &dyn Agent, dir: &Path) -> Option<String> {
    write_and_read(agent, dir, &canonical_servers())
}

// ── Invariants every dialect must satisfy ───────────────────────────────────

#[test]
fn automatic_internal_fields_never_reach_an_agent_config() {
    for agent in all() {
        let dir = tempdir().unwrap();
        let Some(content) = write_canonical(agent, dir.path()) else {
            continue;
        };

        for marker in ["_builtin", "_author", "provenance-marker"] {
            assert!(
                !content.contains(marker),
                "{}: Automatic-internal field `{marker}` leaked into the config:\n{content}",
                agent.id()
            );
        }
    }
}

#[test]
fn inherited_env_vars_are_referenced_but_never_given_a_value() {
    // Agents spell this differently — `${KEY}`, `${env:KEY}`, or Codex's
    // `env_vars` forward list — but in every dialect the variable must still be
    // referenced, and never written as an empty value.
    for agent in all() {
        let dir = tempdir().unwrap();
        let Some(content) = write_canonical(agent, dir.path()) else {
            continue;
        };

        assert!(
            content.contains("GITHUB_TOKEN"),
            "{}: inherited env var was dropped entirely:\n{content}",
            agent.id()
        );
        for empty in [r#""GITHUB_TOKEN": """#, r#""GITHUB_TOKEN" = """#] {
            assert!(
                !content.contains(empty),
                "{}: the empty-string inherit marker must never reach a config file:\n{content}",
                agent.id()
            );
        }
    }
}

#[test]
fn literal_env_values_are_written_verbatim() {
    for agent in all() {
        let dir = tempdir().unwrap();
        let Some(content) = write_canonical(agent, dir.path()) else {
            continue;
        };

        assert!(
            content.contains("demo"),
            "{}: concrete env values must survive the write:\n{content}",
            agent.id()
        );
    }
}

#[test]
fn repeated_writes_are_byte_identical() {
    // Drift detection writes the expected config to a tempdir and compares it
    // against the file on disk, so any nondeterminism in a writer surfaces to
    // the user as permanent, unresolvable drift.
    for agent in all() {
        let dir = tempdir().unwrap();
        let Some(first) = write_canonical(agent, dir.path()) else {
            continue;
        };
        let second = write_canonical(agent, dir.path()).expect("second write");

        assert_eq!(
            first,
            second,
            "{}: writing the same servers twice produced different output",
            agent.id()
        );
    }
}

#[test]
fn written_servers_are_rediscoverable_by_the_same_agent() {
    for agent in all() {
        // Codex CLI writes TOML but `discover_mcp_servers` has no TOML importer
        // yet, so it cannot round trip.  Named explicitly rather than skipped by
        // a generic "discovery returned nothing" guard, so the gap stays visible
        // and every other agent is still held to the invariant.
        if agent.id() == "codex" {
            continue;
        }

        let dir = tempdir().unwrap();
        if write_canonical(agent, dir.path()).is_none() {
            continue;
        }

        let discovered = agent.discover_mcp_servers(dir.path());
        for name in ["github", "linear"] {
            assert!(
                discovered.contains_key(name),
                "{}: server `{name}` was written but not rediscovered (found {:?})",
                agent.id(),
                discovered.keys().collect::<Vec<_>>()
            );
        }
        assert_eq!(
            discovered["github"]["command"],
            "npx",
            "{}: stdio command did not survive a write/discover round trip",
            agent.id()
        );
        assert_eq!(
            discovered["linear"]["url"],
            "https://mcp.linear.app/mcp",
            "{}: remote url did not survive a write/discover round trip",
            agent.id()
        );
        assert!(
            !discovered.contains_key("automatic"),
            "{}: Automatic's own entry is re-injected from the live binary path \
             at sync time and must never be imported back into the registry",
            agent.id()
        );
    }
}

#[test]
fn a_writer_that_writes_reports_the_path_it_wrote() {
    for agent in all() {
        let dir = tempdir().unwrap();
        let prepared = prepare_mcp_servers(agent, &canonical_servers());
        let path = agent
            .write_mcp_config(dir.path(), &prepared)
            .expect("write");

        if path.is_empty() {
            continue;
        }
        assert!(
            Path::new(&path).is_file(),
            "{}: reported writing `{path}` but no such file exists",
            agent.id()
        );
    }
}

// ── prepare_mcp_servers ─────────────────────────────────────────────────────

#[test]
fn prepare_leaves_non_empty_env_values_untouched() {
    let mut servers = Map::new();
    servers.insert(
        "srv".to_string(),
        json!({ "command": "node", "env": { "PORT": "8080", "TOKEN": "" } }),
    );

    let prepared = prepare_mcp_servers(&ClaudeCode, &servers);

    assert_eq!(prepared["srv"]["env"]["PORT"], "8080");
    assert_eq!(prepared["srv"]["env"]["TOKEN"], "${TOKEN}");
}

#[test]
fn prepare_does_not_mutate_the_canonical_map() {
    // engine and drift share one canonical map across every agent in a
    // project, so preparing it for one agent must not affect the next.
    let servers = canonical_servers();

    let cursor = prepare_mcp_servers(&Cursor, &servers);
    let claude = prepare_mcp_servers(&ClaudeCode, &servers);

    assert_eq!(servers["github"]["env"]["GITHUB_TOKEN"], "");
    assert_eq!(
        cursor["github"]["env"]["GITHUB_TOKEN"],
        "${env:GITHUB_TOKEN}"
    );
    assert_eq!(claude["github"]["env"]["GITHUB_TOKEN"], "${GITHUB_TOKEN}");
}

// ── Per-agent dialect details ───────────────────────────────────────────────

#[test]
fn claude_code_uses_bare_placeholders_and_omits_stdio_type() {
    let dir = tempdir().unwrap();
    let content = write_canonical(&ClaudeCode, dir.path()).expect("writes .mcp.json");
    let parsed: Value = serde_json::from_str(&content).unwrap();

    assert_eq!(
        parsed["mcpServers"]["github"]["env"]["GITHUB_TOKEN"],
        "${GITHUB_TOKEN}"
    );
    assert!(
        parsed["mcpServers"]["github"].get("type").is_none(),
        "stdio `type` is stripped for Claude Desktop backward compatibility"
    );
}

#[test]
fn cursor_uses_env_scoped_placeholders_and_keeps_stdio_type() {
    let dir = tempdir().unwrap();
    let content = write_canonical(&Cursor, dir.path()).expect("writes .cursor/mcp.json");
    let parsed: Value = serde_json::from_str(&content).unwrap();

    // Cursor resolves ${env:NAME}; a bare ${NAME} is passed through literally.
    assert_eq!(
        parsed["mcpServers"]["github"]["env"]["GITHUB_TOKEN"],
        "${env:GITHUB_TOKEN}"
    );
    // Cursor's reference table lists `type` as required for stdio entries, and
    // infers it even for entries Automatic stored without one.
    assert_eq!(parsed["mcpServers"]["automatic"]["type"], "stdio");
    assert_eq!(parsed["mcpServers"]["github"]["type"], "stdio");
}

#[test]
fn cursor_maps_oauth_onto_its_auth_block() {
    let mut servers = Map::new();
    servers.insert(
        "remote".to_string(),
        json!({
            "type": "http",
            "url": "https://api.example.com/mcp",
            "oauth": {
                "clientId": "client_123",
                "clientSecret": "secret_456",
                "scope": "read write",
                "callbackPort": 8787,
            },
        }),
    );

    let content = {
        let dir = tempdir().unwrap();
        write_and_read(&Cursor, dir.path(), &servers).expect("writes")
    };
    let parsed: Value = serde_json::from_str(&content).unwrap();
    let remote = &parsed["mcpServers"]["remote"];

    assert_eq!(remote["auth"]["CLIENT_ID"], "client_123");
    assert_eq!(remote["auth"]["CLIENT_SECRET"], "secret_456");
    assert_eq!(remote["auth"]["scopes"], json!(["read", "write"]));
    assert!(
        remote.get("oauth").is_none(),
        "Automatic's `oauth` block is not part of Cursor's schema"
    );
    assert!(
        remote["auth"].get("callbackPort").is_none(),
        "Cursor registers fixed redirect URLs, so callbackPort has no counterpart"
    );
}

#[test]
fn cursor_omits_the_auth_block_when_there_is_no_client_id() {
    let mut servers = Map::new();
    servers.insert(
        "remote".to_string(),
        json!({
            "type": "http",
            "url": "https://api.example.com/mcp",
            // Scope alone is useless without a client id — Cursor should fall
            // back to Dynamic Client Registration rather than see a half block.
            "oauth": { "scope": "read" },
        }),
    );

    let content = {
        let dir = tempdir().unwrap();
        write_and_read(&Cursor, dir.path(), &servers).expect("writes")
    };
    let parsed: Value = serde_json::from_str(&content).unwrap();
    let remote = &parsed["mcpServers"]["remote"];

    assert!(remote.get("auth").is_none());
    assert!(remote.get("oauth").is_none());
}

#[test]
fn cursor_strips_internal_fields_from_remote_entries_too() {
    let mut servers = Map::new();
    servers.insert(
        "remote".to_string(),
        json!({
            "type": "http",
            "url": "https://api.example.com/mcp",
            "enabled": true,
            "timeout": 30,
        }),
    );

    let content = {
        let dir = tempdir().unwrap();
        write_and_read(&Cursor, dir.path(), &servers).expect("writes")
    };
    let parsed: Value = serde_json::from_str(&content).unwrap();
    let remote = &parsed["mcpServers"]["remote"];

    assert!(
        remote.get("enabled").is_none(),
        "`enabled` is Automatic-internal"
    );
    assert!(
        remote.get("timeout").is_none(),
        "`timeout` is Automatic-internal"
    );
    assert_eq!(remote["url"], "https://api.example.com/mcp");
}

#[test]
fn github_copilot_uses_the_servers_key() {
    let dir = tempdir().unwrap();
    let content = write_canonical(&GitHubCopilot, dir.path()).expect("writes .vscode/mcp.json");
    let parsed: Value = serde_json::from_str(&content).unwrap();

    assert!(
        parsed["servers"].is_object(),
        "VS Code uses `servers`, not `mcpServers`"
    );
    assert!(parsed.get("mcpServers").is_none());
}

#[test]
fn zed_uses_the_context_servers_key() {
    let dir = tempdir().unwrap();
    let content = write_canonical(&Zed, dir.path()).expect("writes .zed/settings.json");
    let parsed: Value = serde_json::from_str(&content).unwrap();

    assert!(parsed["context_servers"].is_object());
    assert!(parsed.get("mcpServers").is_none());
}

#[test]
fn codex_forwards_inherited_env_vars_instead_of_writing_a_placeholder() {
    let dir = tempdir().unwrap();
    let content = write_canonical(&CodexCli, dir.path()).expect("writes .codex/config.toml");

    assert!(
        content.contains(r#"env_vars = ["GITHUB_TOKEN"]"#),
        "inherited vars belong in Codex's forward list:\n{content}"
    );
    assert!(
        !content.contains("${GITHUB_TOKEN}"),
        "TOML has no interpolation — a placeholder would reach the server literally:\n{content}"
    );
    assert!(
        !content.contains("[mcp_servers.github.env]"),
        "the env table should be dropped once its only key was forwarded:\n{content}"
    );
    // A concrete value still belongs in the env table.
    assert!(content.contains("[mcp_servers.automatic.env]"));
    assert!(content.contains(r#""AUTOMATIC_PROJECT" = "demo""#));
}

#[test]
fn codex_infers_transport_and_uses_http_headers() {
    let dir = tempdir().unwrap();
    let content = write_canonical(&CodexCli, dir.path()).expect("writes .codex/config.toml");

    // Codex has no `type`/`transport` key — `command` means stdio and `url`
    // means streamable HTTP.
    assert!(
        !content.contains("type ="),
        "`type` is not a Codex key:\n{content}"
    );
    assert!(content.contains(r#"url = "https://mcp.linear.app/mcp""#));
    assert!(
        content.contains("[mcp_servers.linear.http_headers]"),
        "Codex spells static headers `http_headers`:\n{content}"
    );
}

#[test]
fn codex_output_parses_as_toml() {
    let dir = tempdir().unwrap();
    let content = write_canonical(&CodexCli, dir.path()).expect("writes .codex/config.toml");

    let parsed: toml::Value = toml::from_str(&content)
        .unwrap_or_else(|e| panic!("Codex writes invalid TOML: {e}\n{content}"));
    let servers = parsed["mcp_servers"].as_table().expect("mcp_servers table");

    assert_eq!(
        servers["automatic"]["command"].as_str(),
        Some("/usr/local/bin/automatic")
    );
    assert_eq!(
        servers["github"]["env_vars"].as_array().map(Vec::len),
        Some(1)
    );
    assert!(
        servers["linear"].get("command").is_none(),
        "a remote entry must not also carry a command — Codex treats that as a config error"
    );
}

#[test]
fn opencode_uses_the_mcp_key_and_declares_its_schema() {
    let dir = tempdir().unwrap();
    let content = write_canonical(&OpenCode, dir.path()).expect("writes opencode.json");
    let parsed: Value = serde_json::from_str(&content).unwrap();

    assert!(parsed["mcp"].is_object());
    assert_eq!(parsed["$schema"], "https://opencode.ai/config.json");
}

// ── Merge writers ───────────────────────────────────────────────────────────
//
// Five agents read their config file before writing it: Codex CLI
// (`.codex/config.toml`), Gemini CLI (`.gemini/settings.json`), GitHub Copilot
// (`.vscode/mcp.json`), OpenCode (`opencode.json`) and Zed
// (`.zed/settings.json`).  Each of those files is the user's, not Automatic's —
// it also carries their model choice, theme, permissions and editor settings.
// `mcp_merge_inputs()` is the registry of those files, so these two tests reach
// every merge writer that exists now and every one added later.

/// Seed content for a merge input, in the file's own syntax, carrying one
/// unrelated top-level key that a correct writer must leave alone.
fn seed_for(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("json") => Some("{\n  \"_userKey\": \"keep\"\n}\n"),
        Some("toml") => Some("_user_key = \"keep\"\n"),
        _ => None,
    }
}

fn write_seed(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn merge_preserves_unrelated_top_level_keys() {
    let mut checked = 0;

    for agent in all() {
        let dir = tempdir().unwrap();
        for input in agent.mcp_merge_inputs(dir.path()) {
            let Some(seed) = seed_for(&input) else {
                panic!(
                    "{}: no seed defined for merge input `{}` — add its syntax to `seed_for`",
                    agent.id(),
                    input.display()
                );
            };
            write_seed(&input, seed);
            checked += 1;
        }

        let Some(content) = write_canonical(agent, dir.path()) else {
            continue;
        };

        for input in agent.mcp_merge_inputs(dir.path()) {
            let after = fs::read_to_string(&input).expect("merge input still readable");
            assert!(
                after.contains("keep"),
                "{}: writing MCP config destroyed an unrelated key in `{}`. \
                 That file is shared with the user:\n{after}",
                agent.id(),
                input.display()
            );
        }

        // The config the writer reported must still carry the servers.
        assert!(
            content.contains("linear"),
            "{}: merging must not drop the servers it was asked to write:\n{content}",
            agent.id()
        );
    }

    assert!(
        checked > 0,
        "no agent declared an mcp_merge_inputs() path — the test would pass vacuously"
    );
}

#[test]
fn a_malformed_target_config_is_an_error_not_a_clobber() {
    // JSON only: Codex's TOML merge is line-based and has no parse step to
    // fail, so it has no clobber to guard against here.
    let mut checked = 0;

    for agent in all() {
        let dir = tempdir().unwrap();
        let json_inputs: Vec<PathBuf> = agent
            .mcp_merge_inputs(dir.path())
            .into_iter()
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();
        if json_inputs.is_empty() {
            continue;
        }

        let corrupt = "{ not json";
        for input in &json_inputs {
            write_seed(input, corrupt);
        }

        let prepared = prepare_mcp_servers(agent, &canonical_servers());
        let result = agent.write_mcp_config(dir.path(), &prepared);

        assert!(
            result.is_err(),
            "{}: a merge target that does not parse must be an error — \
             falling back to an empty document silently destroys the file",
            agent.id()
        );
        for input in &json_inputs {
            assert_eq!(
                fs::read_to_string(input).unwrap(),
                corrupt,
                "{}: the unparseable file must be left exactly as the user left it",
                agent.id()
            );
        }
        checked += 1;
    }

    assert!(
        checked > 0,
        "no agent declared a JSON mcp_merge_inputs() path — the test would pass vacuously"
    );
}

#[test]
fn agents_without_project_level_mcp_config_write_nothing() {
    // These agents keep MCP config in global CLI state or app settings; a sync
    // must not scatter stray files into the project for them.
    for agent in [
        &Cline as &dyn Agent,
        &Goose as &dyn Agent,
        &Warp as &dyn Agent,
        &Antigravity as &dyn Agent,
    ] {
        let dir = tempdir().unwrap();
        let prepared = prepare_mcp_servers(agent, &canonical_servers());
        let path = agent
            .write_mcp_config(dir.path(), &prepared)
            .expect("write");

        assert!(
            path.is_empty(),
            "{}: expected no project-level MCP config, got `{path}`",
            agent.id()
        );
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            0,
            "{}: nothing should be written into the project directory",
            agent.id()
        );
    }
}
