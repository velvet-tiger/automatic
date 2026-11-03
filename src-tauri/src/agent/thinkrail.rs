use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use super::{Agent, Pi};

/// ThinkRail agent — JetBrains' agentic IDE (<https://github.com/JetBrains/thinkrail>).
///
/// ThinkRail runs Pi in-process and has no project config of its own, so every
/// file surface delegates to [`Pi`]: `AGENTS.md`, `.pi/skills/`, `.pi/mcp.json`,
/// `.pi/agents/`, `.pi/prompts/` and Pi's user-level `mcp.json`.  `SPEC.md`
/// files belong to ThinkRail's own agent and are not synced.
///
/// Both agents can sit on one project.  Neither owns `.pi/`, so removing one
/// leaves the paths the other still claims.  Hooks stay off: Pi's are
/// TypeScript extensions, which Automatic should not generate.
pub struct ThinkRail;

impl Agent for ThinkRail {
    // ── Identity ────────────────────────────────────────────────────────

    fn id(&self) -> &'static str {
        "thinkrail"
    }

    fn label(&self) -> &'static str {
        "ThinkRail (Beta)"
    }

    fn config_description(&self) -> &'static str {
        Pi.config_description()
    }

    fn project_file_name(&self) -> &'static str {
        Pi.project_file_name()
    }

    fn mcp_note(&self) -> Option<&'static str> {
        Some(
            "ThinkRail loads .pi/mcp.json only after you trust the project, and asks you to \
             approve a project server before it first runs and again when its entry changes. \
             New ThinkRail workspaces are git worktrees under ~/.thinkrail/worktrees, so files \
             Automatic keeps in .gitignore do not reach them. Add servers at user level to use \
             them in every workspace. Turning off Manage .gitignore also works, but it can \
             commit literal environment values in .pi/mcp.json. Automatic leaves out legacy SSE \
             servers because ThinkRail rejects them. Automatic sees PI_CODING_AGENT_DIR only \
             if it was started with that variable set, so check the target path shown here.",
        )
    }

    // ── Detection ───────────────────────────────────────────────────────

    fn detect_global_install(&self) -> bool {
        super::cli_available("thinkrail")
            || super::home_dir()
                .map(|h| h.join(".thinkrail").exists())
                .unwrap_or(false)
    }

    /// ThinkRail leaves no marker in a project.  `.pi/` means Pi, so it never
    /// adds ThinkRail to a project on its own.
    fn detect_in(&self, _dir: &Path) -> bool {
        false
    }

    fn skill_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        Pi.skill_dirs(dir)
    }

    fn extra_global_skill_dirs(&self) -> Vec<PathBuf> {
        Pi.extra_global_skill_dirs()
    }

    // ── Capabilities ────────────────────────────────────────────────────

    fn capabilities(&self) -> super::AgentCapabilities {
        Pi.capabilities()
    }

    // ── Config writing ──────────────────────────────────────────────────

    fn write_mcp_config(&self, dir: &Path, servers: &Map<String, Value>) -> Result<String, String> {
        Pi.write_mcp_config(dir, servers)
    }

    fn mcp_unsupported_reason(&self, server: &Value) -> Option<String> {
        Pi.mcp_unsupported_reason(server)
    }

    fn global_mcp_target(&self) -> Option<super::GlobalMcpTarget> {
        Pi.global_mcp_target()
    }

    fn write_global_mcp_config(
        &self,
        desired: &Map<String, Value>,
        previously_managed: &[String],
    ) -> Result<super::GlobalMcpWriteReport, String> {
        Pi.write_global_mcp_config(desired, previously_managed)
    }

    /// Pi and ThinkRail write one user-level file, so they share Pi's record.
    fn global_mcp_state_id(&self) -> &'static str {
        Pi.id()
    }

    // ── Sub-agents ──────────────────────────────────────────────────────

    fn agents_dir(&self, dir: &Path) -> Option<PathBuf> {
        Pi.agents_dir(dir)
    }

    fn convert_agent_content(&self, content: &str, name: &str) -> String {
        Pi.convert_agent_content(content, name)
    }

    fn agent_markdown_is_lossy(&self) -> bool {
        Pi.agent_markdown_is_lossy()
    }

    fn agent_content_refusal(&self, content: &str) -> Option<String> {
        Pi.agent_content_refusal(content)
    }

    fn agent_content_warnings(&self, content: &str) -> Vec<String> {
        Pi.agent_content_warnings(content)
    }

    // ── Commands ────────────────────────────────────────────────────────

    fn commands_dir(&self, dir: &Path) -> Option<PathBuf> {
        Pi.commands_dir(dir)
    }

    // ── Discovery ───────────────────────────────────────────────────────

    fn discover_mcp_servers(&self, dir: &Path) -> Map<String, Value> {
        Pi.discover_mcp_servers(dir)
    }

    fn discover_global_mcp_servers(&self) -> Map<String, Value> {
        Pi.discover_global_mcp_servers()
    }

    // ── Cleanup ─────────────────────────────────────────────────────────

    fn owned_config_paths(&self, dir: &Path) -> Vec<PathBuf> {
        Pi.owned_config_paths(dir)
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_pi_files_do_not_detect_thinkrail() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".pi")).unwrap();
        assert!(!ThinkRail.detect_in(dir.path()));
        assert!(Pi.detect_in(dir.path()));
    }

    #[test]
    fn test_writes_the_same_config_as_pi() {
        let pi = tempdir().unwrap();
        let thinkrail = tempdir().unwrap();
        let mut servers = Map::new();
        servers.insert(
            "github".to_string(),
            json!({ "type": "stdio", "command": "npx" }),
        );
        servers.insert(
            "legacy".to_string(),
            json!({ "type": "sse", "url": "https://example.com/sse" }),
        );

        Pi.write_mcp_config(pi.path(), &servers).unwrap();
        ThinkRail.write_mcp_config(thinkrail.path(), &servers).unwrap();

        assert_eq!(
            fs::read_to_string(pi.path().join(".pi/mcp.json")).unwrap(),
            fs::read_to_string(thinkrail.path().join(".pi/mcp.json")).unwrap()
        );
        assert_eq!(
            ThinkRail.discover_mcp_servers(thinkrail.path()).keys().collect::<Vec<_>>(),
            vec!["github"]
        );
    }

    #[test]
    fn test_converts_sub_agents_like_pi() {
        let content = "---\nname: a\ndescription: b\nmodel: sonnet\n---\nBody\n";
        assert_eq!(
            ThinkRail.convert_agent_content(content, "a"),
            Pi.convert_agent_content(content, "a")
        );
    }

    #[test]
    fn test_refuses_and_warns_like_pi() {
        let denied = "---\nname: a\ndescription: b\ndisallowedTools: Bash\n---\nBody\n";
        assert_eq!(
            ThinkRail.agent_content_refusal(denied),
            Pi.agent_content_refusal(denied)
        );
        let partial = "---\nname: a\ndescription: b\ntools: Read, Task\n---\nBody\n";
        assert_eq!(ThinkRail.agent_content_warnings(partial).len(), 1);
    }

    #[test]
    fn test_shares_the_global_mcp_target_and_record_with_pi() {
        assert_eq!(ThinkRail.global_mcp_state_id(), "pi");
        assert_eq!(
            ThinkRail.global_mcp_target().map(|t| t.path),
            Pi.global_mcp_target().map(|t| t.path)
        );
    }

    #[test]
    fn test_removing_thinkrail_keeps_files_pi_still_uses() {
        let dir = tempdir().unwrap();
        let pi_dir = dir.path().join(".pi");
        fs::create_dir(&pi_dir).unwrap();
        fs::write(pi_dir.join("mcp.json"), "{}").unwrap();
        fs::write(dir.path().join("AGENTS.md"), "# shared\n").unwrap();

        let remaining = vec!["pi".to_string()];
        let plan =
            super::super::plan_agent_removal(&ThinkRail, dir.path(), &remaining, &[]).unwrap();
        super::super::apply_removal_plan(&ThinkRail, dir.path(), &plan).unwrap();

        assert!(pi_dir.join("mcp.json").exists());
        assert!(dir.path().join("AGENTS.md").exists());
    }

    #[test]
    fn test_removing_the_last_of_both_keeps_the_users_settings() {
        let dir = tempdir().unwrap();
        let pi_dir = dir.path().join(".pi");
        fs::create_dir(&pi_dir).unwrap();
        fs::write(pi_dir.join("mcp.json"), "{}").unwrap();
        fs::write(pi_dir.join("settings.json"), "{}").unwrap();

        let plan = super::super::plan_agent_removal(&ThinkRail, dir.path(), &[], &[]).unwrap();
        super::super::apply_removal_plan(&ThinkRail, dir.path(), &plan).unwrap();

        assert!(!pi_dir.join("mcp.json").exists());
        assert!(pi_dir.join("settings.json").exists());
    }

    #[test]
    fn test_removing_keeps_a_system_prompt_with_either_agent_remaining() {
        let pairs: [(&dyn Agent, &str); 2] = [(&ThinkRail, "pi"), (&Pi, "thinkrail")];
        for (leaving, stays) in pairs {
            let dir = tempdir().unwrap();
            let pi_dir = dir.path().join(".pi");
            fs::create_dir_all(pi_dir.join("skills").join("mine")).unwrap();
            fs::write(pi_dir.join("SYSTEM.md"), "mine").unwrap();
            fs::write(pi_dir.join("skills").join("mine").join("SKILL.md"), "mine").unwrap();
            let remaining = vec![stays.to_string()];

            let plan = super::super::plan_agent_removal(leaving, dir.path(), &remaining, &[]).unwrap();
            super::super::apply_removal_plan(leaving, dir.path(), &plan).unwrap();

            assert!(pi_dir.join("SYSTEM.md").exists());
            assert!(pi_dir.join("skills").join("mine").join("SKILL.md").exists());
        }
    }

    #[test]
    fn test_last_removal_keeps_a_system_prompt() {
        let dir = tempdir().unwrap();
        let pi_dir = dir.path().join(".pi");
        fs::create_dir_all(&pi_dir).unwrap();
        fs::write(pi_dir.join("SYSTEM.md"), "mine").unwrap();
        let plan = super::super::plan_agent_removal(&ThinkRail, dir.path(), &[], &[]).unwrap();
        super::super::apply_removal_plan(&ThinkRail, dir.path(), &plan).unwrap();
        assert!(pi_dir.join("SYSTEM.md").exists());
    }

    #[test]
    fn test_prompt_frontmatter_survives_command_conversion() {
        let content = "---\ndescription: Review it\nargument-hint: <path>\n---\nReview $ARGUMENTS\n";
        for agent in [&Pi as &dyn Agent, &ThinkRail] {
            let rendered = agent.convert_command_content(content, "review");
            assert!(rendered.contains("description: Review it\n"), "{rendered}");
            assert!(rendered.contains("argument-hint: <path>\n"), "{rendered}");
            assert!(rendered.contains("$ARGUMENTS"), "{rendered}");
        }
    }

    #[test]
    fn test_note_warns_about_committing_env_values() {
        let note = ThinkRail.mcp_note().expect("note");
        assert!(note.contains("user level"));
        assert!(note.contains("literal environment values"));
    }

    #[test]
    fn test_note_covers_trust_and_worktrees() {
        let note = ThinkRail.mcp_note().expect("ThinkRail should carry an MCP note");
        assert!(note.contains("trust"));
        assert!(note.contains("~/.thinkrail/worktrees"));
        assert!(note.contains("SSE"));
    }
}
