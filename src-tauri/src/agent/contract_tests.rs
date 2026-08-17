//! Contract tests over the whole agent registry.
//!
//! [`AgentCapabilities`] and the trait methods that implement those
//! capabilities are two independent declarations of the same fact, and nothing
//! in the sync engine forces them to agree.  Sub-agent sync runs whenever
//! `agents_dir()` returns `Some`, command sync whenever `commands_dir()` does;
//! only `capabilities().hooks` gates anything.  So a flag can go stale in
//! either direction:
//!
//! - a flag with no method lies to the user — the UI shows a badge for a
//!   feature that never syncs;
//! - a method with no flag is invisible — sync writes files the UI never
//!   mentions.
//!
//! Zed shipped as the first kind: `agents: true` with an `agents_dir` of
//! `.zed/agents`, a directory Zed does not read.  The tests below turn both
//! failure modes into a test failure, and the capability-matrix snapshot makes
//! every future change to the registry show up as one reviewable diff.

use super::*;
use tempfile::tempdir;

/// A fixed root so rendered paths are stable across machines.
const ROOT: &str = "/project";

/// Render a path below [`ROOT`] with forward slashes, so the snapshot reads
/// the same on every platform.
fn relative(path: &Path) -> String {
    path.strip_prefix(ROOT)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn enabled_capabilities(caps: &AgentCapabilities) -> String {
    let flags = [
        ("skills", caps.skills),
        ("instructions", caps.instructions),
        ("mcp_servers", caps.mcp_servers),
        ("agents", caps.agents),
        ("commands", caps.commands),
        ("hooks", caps.hooks),
    ];
    let on: Vec<&str> = flags
        .iter()
        .filter(|(_, v)| *v)
        .map(|(name, _)| *name)
        .collect();
    if on.is_empty() {
        "-".to_string()
    } else {
        on.join(",")
    }
}

// ── Capability / method agreement ───────────────────────────────────────────

#[test]
fn agents_dir_and_the_agents_capability_agree() {
    let root = Path::new(ROOT);
    for agent in all() {
        let has_dir = agent.agents_dir(root).is_some();
        let flag = agent.capabilities().agents;
        assert_eq!(
            has_dir,
            flag,
            "{}: capabilities().agents is {flag} but agents_dir() returns {}. \
             The method drives sync and the flag drives the UI badge — they must move together.",
            agent.id(),
            if has_dir { "Some" } else { "None" },
        );
    }
}

#[test]
fn commands_dir_and_the_commands_capability_agree() {
    let root = Path::new(ROOT);
    for agent in all() {
        let has_dir = agent.commands_dir(root).is_some();
        let flag = agent.capabilities().commands;
        assert_eq!(
            has_dir,
            flag,
            "{}: capabilities().commands is {flag} but commands_dir() returns {}.",
            agent.id(),
            if has_dir { "Some" } else { "None" },
        );
    }
}

// ── Sub-agents ───────────────────────────────────────────────────────────────

/// `agent_file_name` must produce a filename `Path::extension` can round-trip
/// back to `agents_file_ext()`, and that starts with the machine name given —
/// otherwise a compound-extension vendor (Copilot's `{name}.agent.md`) would
/// reintroduce the `file_stem()` bug this method exists to fix: `file_stem()`
/// only strips the *last* dot-segment, so recovering `name` from a filename
/// that doesn't literally start with it would already be lossy before a stale
/// sweep ever runs.
#[test]
fn sub_agent_filenames_round_trip() {
    for agent in all() {
        let file_name = agent.agent_file_name("my-agent");
        let ext = agent.agents_file_ext();
        assert!(
            file_name.starts_with("my-agent"),
            "{}: agent_file_name(\"my-agent\") = \"{file_name}\" does not start with the machine name",
            agent.id(),
        );
        assert!(
            file_name.ends_with(&format!(".{ext}")),
            "{}: agent_file_name(\"my-agent\") = \"{file_name}\" does not end with .{ext}",
            agent.id(),
        );
        assert_eq!(
            Path::new(&file_name).extension().and_then(|e| e.to_str()),
            Some(ext),
            "{}: Path::extension() of \"{file_name}\" does not recover agents_file_ext() \"{ext}\"",
            agent.id(),
        );
    }
}

// ── Hooks ────────────────────────────────────────────────────────────────────

/// A minimal command-handler hook, matcher-free so it fits every event this
/// registry declares.
fn cmd_hook(agent_id: &str, name: &str, event: &str) -> crate::core::Hook {
    crate::core::Hook {
        name: name.to_string(),
        agent: agent_id.to_string(),
        event: event.to_string(),
        matcher: None,
        handler: crate::core::HookHandler::Command {
            command: "echo hi".to_string(),
        },
        timeout_sec: None,
        plugin_id: None,
        _author: None,
    }
}

#[test]
fn hook_events_and_the_hooks_capability_agree() {
    for agent in all() {
        let has_events = !agent.hook_events().is_empty();
        let flag = agent.capabilities().hooks;
        assert_eq!(
            has_events,
            flag,
            "{}: capabilities().hooks is {flag} but hook_events() returns {} events. \
             hook_events() drives the UI picker and the flag drives the badge — they must move together.",
            agent.id(),
            agent.hook_events().len(),
        );
    }
}

/// One-directional, unlike the test above: a `hook_config_target()` with no
/// `hooks` capability would mean drift detection watches a file the UI never
/// admits exists. The reverse is not required — Cursor declares `hooks:
/// true` but returns `None` here, because its sidecar-manifest mechanism
/// fits neither `HookConfigTarget` flavour and drift detection for it is
/// deliberately out of scope (see `cursor.rs` and Phase 6 of the agent gap
/// remediation plan).
#[test]
fn hook_config_target_implies_the_hooks_capability() {
    let root = Path::new(ROOT);
    for agent in all() {
        if agent.hook_config_target(root).is_some() {
            assert!(
                agent.capabilities().hooks,
                "{}: hook_config_target() returns Some but capabilities().hooks is false",
                agent.id(),
            );
        }
    }
}

/// For every agent that declares hook events, build one hook per declared
/// event and confirm a sync actually writes it somewhere. This is what would
/// have caught `CODEX_SUPPORTED_EVENTS` sitting at 6 events while the vendor
/// documented 11 — the array itself was internally consistent, it was just
/// stale against upstream.
#[test]
fn every_declared_hook_event_survives_a_sync() {
    for agent in all() {
        let events = agent.hook_events();
        if events.is_empty() {
            continue;
        }

        let dir = tempdir().unwrap();
        let hooks: Vec<crate::core::Hook> = events
            .iter()
            .enumerate()
            .map(|(i, event)| cmd_hook(agent.id(), &format!("fixture-{i}"), event))
            .collect();

        let written = agent
            .sync_hooks(dir.path(), &hooks)
            .unwrap_or_else(|e| panic!("{}: sync_hooks failed: {e}", agent.id()));
        assert!(
            !written.is_empty(),
            "{}: sync_hooks wrote no files for {} declared events",
            agent.id(),
            events.len()
        );

        let mut content = String::new();
        for path in &written {
            content.push_str(&std::fs::read_to_string(path).unwrap_or_else(|e| {
                panic!("{}: could not read written file '{path}': {e}", agent.id())
            }));
        }
        for event in events {
            assert!(
                content.contains(event),
                "{}: declared event '{event}' does not appear in the synced config",
                agent.id(),
            );
        }
    }
}

// ── Skill directories ───────────────────────────────────────────────────────

#[test]
fn every_skill_dir_is_inside_the_project_dir() {
    let root = Path::new(ROOT);
    for agent in all() {
        let dirs = agent.skill_dirs(root);
        assert!(
            !dirs.is_empty(),
            "{}: skill_dirs() must name at least one directory",
            agent.id()
        );
        for dir in dirs {
            let rel = dir.strip_prefix(root).unwrap_or_else(|_| {
                panic!(
                    "{}: skill dir `{}` is outside the project directory",
                    agent.id(),
                    dir.display()
                )
            });
            assert!(
                rel.components().count() > 0,
                "{}: skill dir must not be the project directory itself",
                agent.id()
            );
            assert!(
                !rel.components()
                    .any(|c| c.as_os_str() == std::ffi::OsStr::new("..")),
                "{}: skill dir `{}` escapes the project directory",
                agent.id(),
                dir.display()
            );
        }
    }
}

/// Drift detection is the only caller of `sync_skills`: it writes the expected
/// state into a tempdir and then compares every entry of `skill_dirs` against
/// disk.  A directory `sync_skills` skips is a directory drift can never check,
/// which is how `.junie/skills` stayed outside drift detection.
#[test]
fn sync_skills_populates_every_skill_dir() {
    let skills = vec![("contract-fixture".to_string(), "# Fixture\n".to_string())];
    let selected = vec!["contract-fixture".to_string()];

    for agent in all() {
        let dir = tempdir().unwrap();
        agent
            .sync_skills(dir.path(), &skills, &selected, &[])
            .unwrap_or_else(|e| panic!("{}: sync_skills failed: {e}", agent.id()));

        for skills_dir in agent.skill_dirs(dir.path()) {
            let written = skills_dir.join("contract-fixture").join("SKILL.md");
            assert!(
                written.is_file(),
                "{}: skill_dirs() names `{}` but sync_skills did not write into it",
                agent.id(),
                relative(skills_dir.strip_prefix(dir.path()).unwrap()),
            );
        }
    }
}

// ── Capability matrix snapshot ──────────────────────────────────────────────

/// One line per agent, in `all()` order (alphabetical by label).
///
/// This is the review surface.  Adding an agent, flipping a capability, or
/// moving a directory shows up here as a single diff, which is exactly what was
/// missing when Zed's `agents: true` went stale.  Update the expected string
/// deliberately — never to make a red test go green.
#[test]
fn the_capability_matrix_is_unchanged() {
    let root = Path::new(ROOT);

    let actual: String = all()
        .iter()
        .map(|a| {
            format!(
                "{} | {} | {} | {} | {} | {} | {}",
                a.id(),
                a.label(),
                a.project_file_name(),
                enabled_capabilities(&a.capabilities()),
                a.agents_dir(root)
                    .map(|d| relative(&d))
                    .unwrap_or("-".into()),
                a.commands_dir(root)
                    .map(|d| relative(&d))
                    .unwrap_or("-".into()),
                a.skill_dirs(root)
                    .iter()
                    .map(|d| relative(d))
                    .collect::<Vec<_>>()
                    .join(","),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let expected = "\
antigravity | Antigravity (Beta) | GEMINI.md | skills,instructions | - | - | .agents/skills
claude | Claude Code | CLAUDE.md | skills,instructions,mcp_servers,agents,commands,hooks | .claude/agents | .claude/commands | .claude/skills
cline | Cline (Beta) | .clinerules/automatic.md | skills,instructions | - | - | .cline/skills
codex | Codex CLI | AGENTS.md | skills,instructions,mcp_servers,agents,hooks | .codex/agents | - | .agents/skills
cursor | Cursor | AGENTS.md | skills,instructions,mcp_servers,agents,commands,hooks | .cursor/agents | .cursor/commands | .agents/skills
droid | Droid (Beta) | AGENTS.md | skills,instructions,mcp_servers,agents,hooks | .factory/droids | - | .agents/skills
gemini | Gemini CLI (Beta) | GEMINI.md | skills,instructions,mcp_servers,agents,commands,hooks | .gemini/agents | .gemini/commands | .agents/skills
copilot | GitHub Copilot (Beta) | .github/copilot-instructions.md | skills,instructions,mcp_servers,agents,commands,hooks | .github/agents | .github/prompts | .agents/skills
goose | Goose (Beta) | AGENTS.md | skills,instructions | - | - | .agents/skills
junie | Junie (Beta) | .junie/AGENTS.md | skills,instructions,mcp_servers | - | - | .junie/skills,.agents/skills
kilo | Kilo (Beta) | AGENTS.md | skills,instructions,mcp_servers | - | - | .agents/skills
kimi | Kimi Code | AGENTS.md | skills,instructions,mcp_servers,agents | .kimi-code/agents | - | .agents/skills
kiro | Kiro (Beta) | AGENTS.md | skills,instructions,mcp_servers,agents | .kiro/agents | - | .kiro/skills
opencode | OpenCode | AGENTS.md | skills,instructions,mcp_servers,agents,commands | .opencode/agents | .opencode/commands | .agents/skills
pi | Pi (Beta) | AGENTS.md | skills,instructions,mcp_servers,agents | .pi/agents | - | .pi/skills
warp | Warp (Beta) | AGENTS.md | skills,instructions | - | - | .agents/skills
zcode | Z Code (Beta) | AGENTS.md | skills,instructions,mcp_servers | - | - | .zcode/skills
zed | Zed (Beta) | AGENTS.md | skills,instructions,mcp_servers | - | - | .agents/skills";

    assert_eq!(
        actual, expected,
        "\nThe agent capability matrix changed. If the change is intended, paste \
         the actual block above into `expected`.\n"
    );
}
