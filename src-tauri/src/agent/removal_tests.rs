//! Tests for agent removal.  The central check, [`remove_and_verify`], proves
//! the preview lists exactly what removal changes on disk: every planned
//! path changed as described, and nothing outside the plan changed.

use super::*;
use crate::agent::{
    all, Antigravity, ClaudeCode, CodexCli, Cursor, GeminiCli, GitHubCopilot, Junie, Zed,
};
use std::collections::BTreeMap;
use tempfile::tempdir;

/// Every file and directory under `root`.  Files carry their bytes;
/// directories carry `None`.  Symlinks are recorded, never followed.
fn tree(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                out.insert(path.clone(), None);
                stack.push(path);
            } else if meta.file_type().is_symlink() {
                let target = fs::read_link(&path).unwrap();
                out.insert(path, Some(target.display().to_string().into_bytes()));
            } else {
                out.insert(path.clone(), Some(fs::read(&path).unwrap()));
            }
        }
    }
    out
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// Plan, apply, and check the result against the disk.  Returns the
/// entries as `(relative path, action)` pairs for easy assertions.
fn remove_and_verify(
    agent: &dyn Agent,
    root: &Path,
    remaining: &[&str],
) -> Vec<(String, RemovalAction)> {
    remove_and_verify_with_managed(agent, root, remaining, &[])
}

fn remove_and_verify_with_managed(
    agent: &dyn Agent,
    root: &Path,
    remaining: &[&str],
    managed_mcp_servers: &[&str],
) -> Vec<(String, RemovalAction)> {
    let remaining: Vec<String> = remaining.iter().map(|s| s.to_string()).collect();
    let managed: Vec<String> = managed_mcp_servers
        .iter()
        .map(|s| s.to_string())
        .collect();
    let before = tree(root);

    let plan = plan_agent_removal(agent, root, &remaining, &managed).unwrap();
    let preview = plan.entries();
    let applied = apply_removal_plan(agent, root, &plan).unwrap();

    // Applied is a subset of preview, in the same order.  Every entry in
    // applied appears in preview and describes work that actually took
    // effect.  A preview entry missing from applied is a silent no-op —
    // a soft-delete parent whose strip preserved user content.
    let applied_paths: std::collections::HashSet<String> =
        applied.iter().map(|e| e.path.clone()).collect();
    let applied_sequence: Vec<&RemovalEntry> = preview
        .iter()
        .filter(|e| applied_paths.contains(&e.path))
        .collect();
    let applied_expected: Vec<RemovalEntry> = applied_sequence.into_iter().cloned().collect();
    assert_eq!(
        applied, applied_expected,
        "apply must return preview entries that took effect, in preview order"
    );

    let after = tree(root);
    for entry in &preview {
        let path = PathBuf::from(&entry.path);
        let took_effect = applied_paths.contains(&entry.path);
        match entry.action {
            RemovalAction::Delete | RemovalAction::RemoveEmptyDir => {
                if took_effect {
                    assert!(
                        fs::symlink_metadata(&path).is_err(),
                        "{} should be gone",
                        entry.path
                    )
                } else {
                    // Only a soft-delete parent may survive; it must still
                    // exist on disk with content — the strip preserved
                    // something the user cares about.
                    assert!(
                        entry.is_dir,
                        "{} is not a directory but was skipped",
                        entry.path
                    );
                    assert!(
                        fs::symlink_metadata(&path).is_ok(),
                        "{} was skipped from delete but is gone",
                        entry.path
                    );
                }
            }
            RemovalAction::Strip => assert_ne!(
                before.get(&path),
                after.get(&path),
                "{} should have been stripped",
                entry.path
            ),
            RemovalAction::KeepShared => assert_eq!(
                before.get(&path),
                after.get(&path),
                "{} is shared and must not change",
                entry.path
            ),
        }
    }

    // Nothing outside the plan may change.  A soft-delete parent that
    // survived counts as "covered" because the preview lists it as Delete
    // and only its non-Automatic content may remain.
    let covered = |path: &Path| {
        preview.iter().any(|entry| {
            let planned = Path::new(&entry.path);
            match entry.action {
                RemovalAction::Delete | RemovalAction::RemoveEmptyDir => path.starts_with(planned),
                RemovalAction::Strip => path == planned,
                RemovalAction::KeepShared => false,
            }
        })
    };
    for (path, content) in &before {
        if after.get(path) != Some(content) {
            assert!(
                covered(path),
                "{} changed but the preview did not list it",
                path.display()
            );
        }
    }
    for path in after.keys() {
        assert!(
            before.contains_key(path),
            "removal created {}",
            path.display()
        );
    }

    preview
        .into_iter()
        .map(|e| (e.relative_path, e.action))
        .collect()
}

fn entry(path: &str, action: RemovalAction) -> (String, RemovalAction) {
    (path.to_string(), action)
}

use RemovalAction::{Delete, KeepShared, RemoveEmptyDir, Strip};

#[test]
fn claude_remove_deletes_dot_claude_whole_and_its_instruction_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(&root.join(".claude/settings.json"), "{\"model\":\"x\"}");
    write(&root.join(".claude/notes/mine.md"), "user file");
    write(&root.join(".claude/skills/demo/SKILL.md"), "# Demo");
    write(&root.join("CLAUDE.md"), "# Instructions");
    write(&root.join(".mcp.json"), "{}");
    write(
        &root.join(".automatic/snapshots/CLAUDE.md"),
        "# Instructions",
    );
    write(&root.join(".automatic/snapshots/AGENTS.md"), "# Other");
    write(&root.join("README.md"), "# Project");

    let result = remove_and_verify(&ClaudeCode, root, &[]);

    assert_eq!(
        result,
        vec![
            entry(".claude", Delete),
            entry("CLAUDE.md", Delete),
            entry(".automatic/snapshots/CLAUDE.md", Delete),
            entry(".mcp.json", Delete),
        ]
    );
    assert!(root.join("README.md").exists());
    assert!(root.join(".automatic/snapshots/AGENTS.md").exists());
}

#[test]
fn cursor_remove_keeps_agents_md_and_hub_while_codex_uses_them() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(&root.join(".cursor/mcp.json"), "{}");
    write(&root.join(".cursor/rules/user.mdc"), "user rule");
    write(&root.join("AGENTS.md"), "# Shared");
    write(&root.join(".agents/skills/demo/SKILL.md"), "# Demo");
    write(&root.join(".automatic/state/cursor-hooks.json"), "{}");
    write(&root.join(".automatic/instructions/rule.md"), "rule");

    let result = remove_and_verify(&Cursor, root, &["codex"]);

    assert_eq!(
        result,
        vec![
            entry(".cursor", Delete),
            entry("AGENTS.md", KeepShared),
            entry(".agents/skills", KeepShared),
            entry(".automatic/state/cursor-hooks.json", Delete),
            entry(".automatic/state", RemoveEmptyDir),
        ]
    );
    let plan = plan_agent_removal(&Cursor, root, &["codex".to_string()], &[]).unwrap();
    assert!(
        plan.entries().iter().all(|e| e.action == KeepShared),
        "a second removal has nothing left to delete"
    );
}

#[test]
fn shared_entries_name_the_agents_still_using_them() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(&root.join("AGENTS.md"), "# Shared");

    let remaining = vec!["codex".to_string(), "claude".to_string()];
    let plan = plan_agent_removal(&Cursor, root, &remaining, &[]).unwrap();
    let entries = plan.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].action, KeepShared);
    assert_eq!(entries[0].shared_with, vec!["Codex CLI".to_string()]);
}

#[test]
fn cursor_remove_as_last_agents_md_user_deletes_shared_files() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(&root.join(".cursor/mcp.json"), "{}");
    write(&root.join("AGENTS.md"), "# Instructions");
    write(&root.join(".agents/skills/demo/SKILL.md"), "# Demo");

    let result = remove_and_verify(&Cursor, root, &["claude"]);

    assert_eq!(
        result,
        vec![
            entry(".cursor", Delete),
            entry("AGENTS.md", Delete),
            entry(".agents/skills", Delete),
            entry(".agents", RemoveEmptyDir),
        ]
    );
}

#[test]
fn junie_remove_deletes_dot_junie_whole_including_user_files() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(&root.join(".junie/mcp/mcp.json"), "{}");
    write(&root.join(".junie/AGENTS.md"), "# Guidelines");
    write(&root.join(".junie/skills/demo/SKILL.md"), "# Demo");
    write(&root.join(".junie/guidelines/notes.md"), "user notes");
    write(&root.join(".agents/skills/demo/SKILL.md"), "# Demo");

    let result = remove_and_verify(&Junie, root, &["codex"]);

    assert_eq!(
        result,
        vec![entry(".junie", Delete), entry(".agents/skills", KeepShared)]
    );
}

#[test]
fn codex_remove_deletes_agents_md_once_no_remaining_agent_reads_it() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    // `.codex/config.toml` holds only the user's hand-edited model setting;
    // no `[mcp_servers.*]` blocks means the strip has nothing to remove and
    // the file must survive Remove (VEL-174).  `.codex/` becomes a
    // soft-delete parent for the same reason and stays behind with the
    // file.  Codex's own sub-agent directory still goes.
    write(&root.join(".codex/config.toml"), "model = \"x\"\n");
    write(&root.join(".codex/agents/reviewer.toml"), "name = \"r\"\n");
    write(&root.join("AGENTS.md"), "# Instructions");
    write(&root.join(".agents/skills/demo/SKILL.md"), "# Demo");
    write(&root.join(".agents/mcp_config.json"), "{}");
    write(&root.join("CLAUDE.md"), "# Claude");

    let result = remove_and_verify(&CodexCli, root, &["claude"]);

    assert_eq!(
        result,
        vec![
            entry(".codex", Delete),
            entry("AGENTS.md", Delete),
            entry(".agents/skills", Delete),
            entry(".codex/agents", Delete),
        ]
    );
    assert!(
        root.join(".codex/config.toml").exists(),
        "user-authored .codex/config.toml must survive"
    );
    assert!(
        root.join(".codex").exists(),
        ".codex/ survives when a strip preserves user content inside it"
    );
    assert!(
        !root.join(".codex/agents").exists(),
        "Codex's sub-agent directory still goes"
    );
    assert!(
        root.join(".agents/mcp_config.json").exists(),
        "Antigravity's file is not Codex's"
    );
    assert!(root.join("CLAUDE.md").exists());
}

#[test]
fn copilot_remove_deletes_its_github_folders_and_strips_vscode_mcp() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".github/copilot-instructions.md"),
        "# Instructions",
    );
    write(&root.join(".github/agents/mine.agent.md"), "user agent");
    write(&root.join(".github/prompts/review.prompt.md"), "prompt");
    write(&root.join(".github/hooks/automatic.json"), "{}");
    write(&root.join(".github/workflows/ci.yml"), "on: push\n");
    write(
        &root.join(".vscode/mcp.json"),
        "{\"servers\":{\"a\":{}},\"inputs\":[]}",
    );
    write(&root.join(".vscode/settings.json"), "{}");

    let result = remove_and_verify_with_managed(&GitHubCopilot, root, &[], &["a"]);

    assert_eq!(
        result,
        vec![
            entry(".github/agents", Delete),
            entry(".github/prompts", Delete),
            entry(".github/hooks", Delete),
            entry(".github/copilot-instructions.md", Delete),
            entry(".vscode/mcp.json", Strip),
        ]
    );
    assert!(root.join(".github/workflows/ci.yml").exists());
    assert!(root.join(".vscode/settings.json").exists());
}

/// The bug: removing Copilot used to drop the whole `servers` key from
/// `.vscode/mcp.json`, taking servers the user added in VS Code with it.
/// The fix strips only the entries Automatic manages; every foreign server
/// survives.
#[test]
fn copilot_remove_keeps_user_added_servers_in_vscode_mcp() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".vscode/mcp.json"),
        "{\"servers\":{\"automatic\":{\"type\":\"stdio\",\"command\":\"automatic\"},\
         \"user-server\":{\"type\":\"stdio\",\"command\":\"user\"}},\
         \"inputs\":[]}",
    );

    let result = remove_and_verify_with_managed(&GitHubCopilot, root, &[], &["automatic"]);

    assert_eq!(result, vec![entry(".vscode/mcp.json", Strip)]);

    let raw = fs::read_to_string(root.join(".vscode/mcp.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(
        parsed["servers"]["user-server"]["command"].is_string(),
        "user-added server must survive Copilot removal: {parsed}"
    );
    assert!(
        parsed["servers"]["automatic"].is_null(),
        "Automatic's own entry must be stripped: {parsed}"
    );
    assert!(parsed["inputs"].is_array(), "other keys survive: {parsed}");
}

/// When the file holds only user-added servers (nothing Automatic manages),
/// removing Copilot must not report a Strip action, and the file must not
/// change on disk.
#[test]
fn copilot_remove_reports_no_strip_when_only_user_servers_exist() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".vscode/mcp.json"),
        "{\"servers\":{\"user-only\":{\"type\":\"stdio\",\"command\":\"u\"}}}",
    );

    let result = remove_and_verify_with_managed(&GitHubCopilot, root, &[], &["automatic"]);
    assert!(result.is_empty(), "{result:?}");

    let raw = fs::read_to_string(root.join(".vscode/mcp.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(parsed["servers"]["user-only"]["command"].is_string());
}

/// When every managed server is present and nothing else remains, the file
/// is deleted whole — matching the pre-fix behaviour for the empty case.
#[test]
fn copilot_remove_deletes_file_when_no_user_entries_remain() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".vscode/mcp.json"),
        "{\"servers\":{\"automatic\":{\"type\":\"stdio\",\"command\":\"a\"}}}",
    );

    let result = remove_and_verify_with_managed(&GitHubCopilot, root, &[], &["automatic"]);
    assert_eq!(result, vec![entry(".vscode/mcp.json", Strip)]);
    assert!(
        !root.join(".vscode/mcp.json").exists(),
        "file with nothing left must be deleted"
    );
}

// End-to-end Remove tests for Codex CLI, Gemini CLI, and Zed.  Each of
// these three agents owns its `.codex` / `.gemini` / `.zed` directory
// outright, but the shared config file inside — `.codex/config.toml`,
// `.gemini/settings.json`, `.zed/settings.json` — is a merged MCP target
// that may hold user hand-edits alongside Automatic's entries.  VEL-174
// requires Remove to strip only the managed entries and let a
// user-preserved file (and its parent directory) survive.  These tests
// exercise the full plan + apply flow via `remove_and_verify_with_managed`
// so a regression at either layer trips a failure.

/// Codex mirror of `copilot_remove_keeps_user_added_servers_in_vscode_mcp`.
/// `.codex/config.toml` may hold `[mcp_servers.*]` blocks the user added by
/// hand alongside their own model/history settings.  Remove must strip
/// only the blocks Automatic manages, leave user blocks and every other
/// top-level key intact, and keep `.codex/` on disk with the file.
#[test]
fn codex_remove_keeps_user_added_servers_in_codex_config_toml() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".codex/config.toml"),
        "model = \"gpt-5\"\n\
         \n\
         [mcp_servers.automatic]\n\
         command = \"automatic\"\n\
         \n\
         [mcp_servers.user-server]\n\
         command = \"user\"\n",
    );

    let result = remove_and_verify_with_managed(&CodexCli, root, &[], &["automatic"]);

    assert_eq!(
        result,
        vec![
            entry(".codex", Delete),
            entry(".codex/config.toml", Strip),
        ]
    );

    let raw = fs::read_to_string(root.join(".codex/config.toml")).unwrap();
    assert!(
        !raw.contains("[mcp_servers.automatic]"),
        "Automatic's own block must be stripped: {raw}"
    );
    assert!(
        raw.contains("[mcp_servers.user-server]"),
        "user-added block must survive Codex removal: {raw}"
    );
    assert!(
        raw.contains("command = \"user\""),
        "user block body must survive: {raw}"
    );
    assert!(
        raw.contains("model = \"gpt-5\""),
        "other top-level keys survive: {raw}"
    );
    assert!(
        root.join(".codex").exists(),
        ".codex/ must survive when user content stays inside"
    );
}

/// When `.codex/config.toml` holds only user-added servers, Remove reports
/// no Strip (the parent's Delete is a silent no-op) and the file is
/// untouched on disk.
#[test]
fn codex_remove_reports_no_strip_when_only_user_servers_exist() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let before = "[mcp_servers.user-only]\ncommand = \"u\"\n";
    write(&root.join(".codex/config.toml"), before);

    let result = remove_and_verify_with_managed(&CodexCli, root, &[], &["automatic"]);
    assert_eq!(
        result,
        vec![entry(".codex", Delete)],
        "the soft-delete parent is planned but takes no effect: {result:?}"
    );

    let raw = fs::read_to_string(root.join(".codex/config.toml")).unwrap();
    assert_eq!(raw, before, "user-only file must not change: {raw}");
    assert!(root.join(".codex").exists());
}

/// When every `[mcp_servers.*]` in `.codex/config.toml` is managed and no
/// other content remains, the strip deletes the file whole and `.codex/`
/// is then removed as an empty directory.
#[test]
fn codex_remove_deletes_file_when_no_user_entries_remain() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".codex/config.toml"),
        "[mcp_servers.automatic]\ncommand = \"a\"\n",
    );

    let result = remove_and_verify_with_managed(&CodexCli, root, &[], &["automatic"]);
    assert_eq!(
        result,
        vec![
            entry(".codex", Delete),
            entry(".codex/config.toml", Strip),
        ]
    );
    assert!(
        !root.join(".codex/config.toml").exists(),
        "file with nothing left must be deleted"
    );
    assert!(
        !root.join(".codex").exists(),
        ".codex/ must go once the strip empties it"
    );
}

/// Gemini mirror of `copilot_remove_keeps_user_added_servers_in_vscode_mcp`.
/// `.gemini/settings.json` may hold user-added servers under `mcpServers`
/// alongside their auth/model settings.  Remove must strip only entries
/// Automatic manages and keep `.gemini/` on disk with the file.
#[test]
fn gemini_remove_keeps_user_added_servers_in_gemini_settings() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".gemini/settings.json"),
        "{\"mcpServers\":{\"automatic\":{\"command\":\"automatic\"},\
         \"user-server\":{\"command\":\"user\"}},\
         \"theme\":\"dark\"}",
    );

    let result = remove_and_verify_with_managed(&GeminiCli, root, &[], &["automatic"]);
    assert_eq!(
        result,
        vec![
            entry(".gemini", Delete),
            entry(".gemini/settings.json", Strip),
        ]
    );

    let raw = fs::read_to_string(root.join(".gemini/settings.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(
        parsed["mcpServers"]["user-server"]["command"].is_string(),
        "user-added server must survive Gemini removal: {parsed}"
    );
    assert!(
        parsed["mcpServers"]["automatic"].is_null(),
        "Automatic's own entry must be stripped: {parsed}"
    );
    assert_eq!(
        parsed["theme"].as_str().unwrap(),
        "dark",
        "other keys survive: {parsed}"
    );
    assert!(
        root.join(".gemini").exists(),
        ".gemini/ must survive when user content stays inside"
    );
}

/// When `.gemini/settings.json` holds only user-added servers, Remove
/// reports no Strip and the file is untouched.
#[test]
fn gemini_remove_reports_no_strip_when_only_user_servers_exist() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let before = "{\"mcpServers\":{\"user-only\":{\"command\":\"u\"}}}";
    write(&root.join(".gemini/settings.json"), before);

    let result = remove_and_verify_with_managed(&GeminiCli, root, &[], &["automatic"]);
    assert_eq!(
        result,
        vec![entry(".gemini", Delete)],
        "the soft-delete parent is planned but takes no effect: {result:?}"
    );

    let raw = fs::read_to_string(root.join(".gemini/settings.json")).unwrap();
    assert_eq!(raw, before, "user-only file must not change: {raw}");
    assert!(root.join(".gemini").exists());
}

/// When every entry under `mcpServers` is managed and nothing else remains,
/// the strip deletes `.gemini/settings.json` and `.gemini/` goes with it.
#[test]
fn gemini_remove_deletes_file_when_no_user_entries_remain() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".gemini/settings.json"),
        "{\"mcpServers\":{\"automatic\":{\"command\":\"a\"}}}",
    );

    let result = remove_and_verify_with_managed(&GeminiCli, root, &[], &["automatic"]);
    assert_eq!(
        result,
        vec![
            entry(".gemini", Delete),
            entry(".gemini/settings.json", Strip),
        ]
    );
    assert!(
        !root.join(".gemini/settings.json").exists(),
        "file with nothing left must be deleted"
    );
    assert!(
        !root.join(".gemini").exists(),
        ".gemini/ must go once the strip empties it"
    );
}

/// Zed mirror of `copilot_remove_keeps_user_added_servers_in_vscode_mcp`.
/// `.zed/settings.json` may hold user-added servers under `context_servers`
/// alongside their agent/font/theme settings.  Remove must strip only
/// entries Automatic manages and keep `.zed/` on disk with the file.
#[test]
fn zed_remove_keeps_user_added_servers_in_zed_settings() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".zed/settings.json"),
        "{\"context_servers\":{\"automatic\":{\"command\":\"automatic\"},\
         \"user-server\":{\"command\":\"user\"}},\
         \"ui_font_size\":16}",
    );

    let result = remove_and_verify_with_managed(&Zed, root, &[], &["automatic"]);
    assert_eq!(
        result,
        vec![
            entry(".zed", Delete),
            entry(".zed/settings.json", Strip),
        ]
    );

    let raw = fs::read_to_string(root.join(".zed/settings.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(
        parsed["context_servers"]["user-server"]["command"].is_string(),
        "user-added server must survive Zed removal: {parsed}"
    );
    assert!(
        parsed["context_servers"]["automatic"].is_null(),
        "Automatic's own entry must be stripped: {parsed}"
    );
    assert_eq!(
        parsed["ui_font_size"].as_u64().unwrap(),
        16,
        "other keys survive: {parsed}"
    );
    assert!(
        root.join(".zed").exists(),
        ".zed/ must survive when user content stays inside"
    );
}

/// When `.zed/settings.json` holds only user-added servers, Remove
/// reports no Strip and the file is untouched.
#[test]
fn zed_remove_reports_no_strip_when_only_user_servers_exist() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let before = "{\"context_servers\":{\"user-only\":{\"command\":\"u\"}}}";
    write(&root.join(".zed/settings.json"), before);

    let result = remove_and_verify_with_managed(&Zed, root, &[], &["automatic"]);
    assert_eq!(
        result,
        vec![entry(".zed", Delete)],
        "the soft-delete parent is planned but takes no effect: {result:?}"
    );

    let raw = fs::read_to_string(root.join(".zed/settings.json")).unwrap();
    assert_eq!(raw, before, "user-only file must not change: {raw}");
    assert!(root.join(".zed").exists());
}

/// When every entry under `context_servers` is managed and nothing else
/// remains, the strip deletes `.zed/settings.json` and `.zed/` goes too.
#[test]
fn zed_remove_deletes_file_when_no_user_entries_remain() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".zed/settings.json"),
        "{\"context_servers\":{\"automatic\":{\"command\":\"a\"}}}",
    );

    let result = remove_and_verify_with_managed(&Zed, root, &[], &["automatic"]);
    assert_eq!(
        result,
        vec![
            entry(".zed", Delete),
            entry(".zed/settings.json", Strip),
        ]
    );
    assert!(
        !root.join(".zed/settings.json").exists(),
        "file with nothing left must be deleted"
    );
    assert!(
        !root.join(".zed").exists(),
        ".zed/ must go once the strip empties it"
    );
}

// VEL-174 hand-edit preservation regression tests.  Each seeds the shared
// merged-MCP file with a mix of managed entries, user-added entries, and
// an agent-specific top-level key that Automatic never touches.  After
// Remove, only the managed entries are gone and the parent directory
// survives.

/// `.codex/config.toml` with hand edits and a Codex-specific top-level
/// section must survive Remove — only Automatic's `[mcp_servers.*]` block
/// disappears, and `.codex/` stays on disk.
#[test]
fn codex_remove_keeps_hand_edited_config_toml_and_owned_dir() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".codex/config.toml"),
        "# User's notes on Codex configuration.\n\
         model = \"gpt-5-pro\"\n\
         approval_policy = \"on-request\"\n\
         \n\
         [history]\n\
         persistence = \"save-all\"\n\
         \n\
         [mcp_servers.automatic]\n\
         command = \"automatic\"\n\
         \n\
         [mcp_servers.user-server]\n\
         command = \"user\"\n",
    );

    let result = remove_and_verify_with_managed(&CodexCli, root, &[], &["automatic"]);
    assert_eq!(
        result,
        vec![
            entry(".codex", Delete),
            entry(".codex/config.toml", Strip),
        ]
    );

    let raw = fs::read_to_string(root.join(".codex/config.toml")).unwrap();
    assert!(!raw.contains("[mcp_servers.automatic]"), "{raw}");
    assert!(raw.contains("[mcp_servers.user-server]"), "{raw}");
    assert!(raw.contains("model = \"gpt-5-pro\""), "{raw}");
    assert!(raw.contains("approval_policy"), "{raw}");
    assert!(raw.contains("[history]"), "{raw}");
    assert!(raw.contains("# User's notes"), "user comment survives: {raw}");
    assert!(
        root.join(".codex").exists(),
        ".codex/ must survive the Remove"
    );
}

/// `.gemini/settings.json` with hand edits and a Gemini-specific top-level
/// key (`ide.theme`) must survive Remove.
#[test]
fn gemini_remove_keeps_hand_edited_settings_and_owned_dir() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".gemini/settings.json"),
        "{\
            \"mcpServers\":{\
                \"automatic\":{\"command\":\"automatic\"},\
                \"user-server\":{\"command\":\"user\",\"env\":{\"K\":\"v\"}}\
            },\
            \"ide\":{\"theme\":\"solarized\",\"fontSize\":15},\
            \"telemetry\":{\"enabled\":false}\
         }",
    );

    let result = remove_and_verify_with_managed(&GeminiCli, root, &[], &["automatic"]);
    assert_eq!(
        result,
        vec![
            entry(".gemini", Delete),
            entry(".gemini/settings.json", Strip),
        ]
    );

    let raw = fs::read_to_string(root.join(".gemini/settings.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(parsed["mcpServers"]["automatic"].is_null(), "{parsed}");
    assert!(
        parsed["mcpServers"]["user-server"]["command"].is_string(),
        "{parsed}"
    );
    assert_eq!(
        parsed["mcpServers"]["user-server"]["env"]["K"]
            .as_str()
            .unwrap(),
        "v",
        "user's env survives: {parsed}"
    );
    assert_eq!(
        parsed["ide"]["theme"].as_str().unwrap(),
        "solarized",
        "user's ide.theme survives: {parsed}"
    );
    assert_eq!(
        parsed["ide"]["fontSize"].as_u64().unwrap(),
        15,
        "user's ide.fontSize survives: {parsed}"
    );
    assert_eq!(
        parsed["telemetry"]["enabled"].as_bool().unwrap(),
        false,
        "unrelated top-level keys survive: {parsed}"
    );
    assert!(
        root.join(".gemini").exists(),
        ".gemini/ must survive the Remove"
    );
}

/// `.zed/settings.json` with hand edits and a Zed-specific top-level key
/// (`buffer_font_family`, `theme`) must survive Remove.
#[test]
fn zed_remove_keeps_hand_edited_settings_and_owned_dir() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".zed/settings.json"),
        "{\
            \"context_servers\":{\
                \"automatic\":{\"command\":\"automatic\"},\
                \"user-server\":{\"command\":\"user\"}\
            },\
            \"buffer_font_family\":\"JetBrains Mono\",\
            \"theme\":\"One Dark\",\
            \"ui_font_size\":16\
         }",
    );

    let result = remove_and_verify_with_managed(&Zed, root, &[], &["automatic"]);
    assert_eq!(
        result,
        vec![
            entry(".zed", Delete),
            entry(".zed/settings.json", Strip),
        ]
    );

    let raw = fs::read_to_string(root.join(".zed/settings.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(parsed["context_servers"]["automatic"].is_null(), "{parsed}");
    assert!(
        parsed["context_servers"]["user-server"]["command"].is_string(),
        "{parsed}"
    );
    assert_eq!(
        parsed["buffer_font_family"].as_str().unwrap(),
        "JetBrains Mono",
        "user's font family survives: {parsed}"
    );
    assert_eq!(
        parsed["theme"].as_str().unwrap(),
        "One Dark",
        "user's theme survives: {parsed}"
    );
    assert_eq!(
        parsed["ui_font_size"].as_u64().unwrap(),
        16,
        "user's ui_font_size survives: {parsed}"
    );
    assert!(root.join(".zed").exists(), ".zed/ must survive the Remove");
}

// Regression: the delete-if-empty path still deletes `.codex/`, `.gemini/`,
// and `.zed/` when the strip empties the shared file and nothing else is
// inside.  A behaviour change in the strip that leaves residue behind would
// trip these tests.

#[test]
fn codex_remove_deletes_owned_dir_when_only_automatic_content_inside() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".codex/config.toml"),
        "[mcp_servers.automatic]\ncommand = \"automatic\"\n",
    );

    let result = remove_and_verify_with_managed(&CodexCli, root, &[], &["automatic"]);
    assert_eq!(
        result,
        vec![
            entry(".codex", Delete),
            entry(".codex/config.toml", Strip),
        ]
    );
    assert!(
        !root.join(".codex").exists(),
        ".codex/ must be deleted when it holds only Automatic content"
    );
}

#[test]
fn gemini_remove_deletes_owned_dir_when_only_automatic_content_inside() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".gemini/settings.json"),
        "{\"mcpServers\":{\"automatic\":{\"command\":\"automatic\"}}}",
    );

    let result = remove_and_verify_with_managed(&GeminiCli, root, &[], &["automatic"]);
    assert_eq!(
        result,
        vec![
            entry(".gemini", Delete),
            entry(".gemini/settings.json", Strip),
        ]
    );
    assert!(
        !root.join(".gemini").exists(),
        ".gemini/ must be deleted when it holds only Automatic content"
    );
}

#[test]
fn zed_remove_deletes_owned_dir_when_only_automatic_content_inside() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".zed/settings.json"),
        "{\"context_servers\":{\"automatic\":{\"command\":\"automatic\"}}}",
    );

    let result = remove_and_verify_with_managed(&Zed, root, &[], &["automatic"]);
    assert_eq!(
        result,
        vec![
            entry(".zed", Delete),
            entry(".zed/settings.json", Strip),
        ]
    );
    assert!(
        !root.join(".zed").exists(),
        ".zed/ must be deleted when it holds only Automatic content"
    );
}

#[test]
fn copilot_remove_prunes_an_emptied_github_folder() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join(".github/copilot-instructions.md"),
        "# Instructions",
    );
    write(&root.join(".github/prompts/review.prompt.md"), "prompt");

    let result = remove_and_verify(&GitHubCopilot, root, &[]);

    assert_eq!(
        result,
        vec![
            entry(".github/prompts", Delete),
            entry(".github/copilot-instructions.md", Delete),
            entry(".github", RemoveEmptyDir),
        ]
    );
}

#[test]
fn strip_is_not_listed_when_the_file_holds_no_automatic_entries() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(&root.join(".vscode/mcp.json"), "{\"inputs\":[]}");

    let result = remove_and_verify(&GitHubCopilot, root, &[]);
    assert!(result.is_empty(), "{result:?}");
}

#[test]
fn antigravity_remove_keeps_gemini_md_while_gemini_uses_it() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write(&root.join("GEMINI.md"), "# Shared");
    write(&root.join(".agents/mcp_config.json"), "{}");
    write(&root.join(".agents/skills/demo/SKILL.md"), "# Demo");

    let result = remove_and_verify(&Antigravity, root, &["gemini"]);

    assert_eq!(
        result,
        vec![
            entry("GEMINI.md", KeepShared),
            entry(".agents/skills", KeepShared),
            entry(".agents/mcp_config.json", Delete),
        ]
    );
}

#[test]
fn deleting_the_hub_removes_skill_symlinks_not_their_targets() {
    let dir = tempdir().unwrap();
    let library = tempdir().unwrap();
    let target = library.path().join("demo");
    write(&target.join("SKILL.md"), "# Demo");
    let root = dir.path();
    fs::create_dir_all(root.join(".agents/skills")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, root.join(".agents/skills/demo")).unwrap();

    remove_and_verify(&Cursor, root, &[]);

    assert!(
        target.join("SKILL.md").exists(),
        "the library copy must survive"
    );
}

#[test]
fn nothing_on_disk_means_an_empty_plan() {
    let dir = tempdir().unwrap();
    for agent in all() {
        let plan = plan_agent_removal(agent, dir.path(), &[], &[]).unwrap();
        assert!(
            plan.entries().is_empty(),
            "{} planned {:?}",
            agent.id(),
            plan.entries()
        );
    }
}

#[test]
fn remove_is_refused_in_the_home_directory() {
    let home = tempdir().unwrap();
    crate::core::with_test_home(home.path().to_path_buf(), || {
        write(&home.path().join(".claude/settings.json"), "{}");
        let err = plan_agent_removal(&ClaudeCode, home.path(), &[], &[]).unwrap_err();
        assert!(err.contains("home folder"), "{err}");
        assert!(home.path().join(".claude/settings.json").exists());
    });
}

#[test]
fn removal_mode_parses_remove_and_keep_only() {
    assert_eq!("remove".parse::<RemovalMode>(), Ok(RemovalMode::Remove));
    assert_eq!("keep".parse::<RemovalMode>(), Ok(RemovalMode::Keep));
    assert!("delete".parse::<RemovalMode>().is_err());
}

/// An owned directory is deleted whole, so no other agent may write
/// anything inside it, and it may not contain another agent's paths.
/// Breaking this would let removing one agent delete another's files when
/// both are in the same project.
#[test]
fn owned_dirs_never_overlap_another_agents_paths() {
    let root = Path::new("/project");
    for owner in all() {
        for owned in owner.owned_dirs(root) {
            assert!(
                owned.starts_with(root) && owned != root,
                "{} owns {} outside the project",
                owner.id(),
                owned.display()
            );
            for other in all() {
                if other.id() == owner.id() {
                    continue;
                }
                for claim in candidates(other, root).into_iter().map(|c| c.path) {
                    assert!(
                        !paths_overlap(&claim, &owned),
                        "{} owns {} but {} writes {}",
                        owner.id(),
                        owned.display(),
                        other.id(),
                        claim.display()
                    );
                }
            }
        }
    }
}

/// Every path an agent writes outside its owned directories must be one
/// Remove can find: listed by a trait method that [`candidates`] reads.
/// Shared parents such as `.agents/`, `.github/` and `.vscode/` must never
/// be owned.
#[test]
fn shared_parent_directories_are_never_owned() {
    let root = Path::new("/project");
    let shared = [".agents", ".github", ".vscode", ".automatic"];
    for agent in all() {
        for owned in agent.owned_dirs(root) {
            for name in shared {
                assert_ne!(
                    owned,
                    root.join(name),
                    "{} owns shared {}",
                    agent.id(),
                    name
                );
            }
        }
    }
}
