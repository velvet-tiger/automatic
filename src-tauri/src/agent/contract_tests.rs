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
droid | Droid (Beta) | AGENTS.md | skills,instructions,mcp_servers | - | - | .agents/skills
gemini | Gemini CLI (Beta) | GEMINI.md | skills,instructions,mcp_servers,agents,commands | .gemini/agents | .gemini/commands | .agents/skills
copilot | GitHub Copilot (Beta) | .github/copilot-instructions.md | skills,instructions,mcp_servers,commands | - | .github/prompts | .agents/skills
goose | Goose (Beta) | AGENTS.md | skills,instructions | - | - | .agents/skills
junie | Junie (Beta) | .junie/AGENTS.md | skills,instructions,mcp_servers | - | - | .junie/skills,.agents/skills
kilo | Kilo Code (Beta) | AGENTS.md | skills,instructions,mcp_servers | - | - | .agents/skills
kiro | Kiro (Beta) | AGENTS.md | skills,instructions,mcp_servers | - | - | .kiro/skills
opencode | OpenCode | AGENTS.md | skills,instructions,mcp_servers,agents,commands | .opencode/agents | .opencode/commands | .agents/skills
pi | Pi (Beta) | AGENTS.md | skills,instructions,mcp_servers,agents | .pi/agents | - | .pi/skills
warp | Warp (Beta) | AGENTS.md | skills,instructions | - | - | .agents/skills
zed | Zed (Beta) | .rules | skills,instructions,mcp_servers | - | - | .agents/skills";

    assert_eq!(
        actual, expected,
        "\nThe agent capability matrix changed. If the change is intended, paste \
         the actual block above into `expected`.\n"
    );
}
