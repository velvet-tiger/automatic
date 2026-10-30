//! Adopting an edited custom agent file: working out what the stored Markdown
//! becomes, and keeping the other agents' files in step with it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::agent::{self, Agent};
use crate::core::{CustomAgent, Project};

use super::helpers::{custom_agent_machine_name, read_project_text, write_project_file};

/// Why a file cannot be adopted.  The caller words the message.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AdoptRefusal {
    /// The format has no conversion back to Markdown, or the file does not
    /// parse.
    CannotConvert,
    /// Settings that would not survive being stored and rendered again.
    CannotKeep(Vec<String>),
    /// Settings stored once for every agent, while other agents are
    /// configured.
    SharedSettings(Vec<String>),
    /// The resulting file name would not be one path component.
    NotAPlainName(String),
}

/// The Markdown to store when adopting `disk`, or why it cannot be adopted.
///
/// A Markdown file is the full copy, so it is taken whole.  Other formats
/// hold only part of it (Codex TOML has no `disallowedTools`, and renders
/// `model: sonnet` as `gpt-5.4`), so the stored Markdown is kept and only
/// what the user changed is applied.  Either way the result is rendered back
/// and compared with `disk`.  What passes leaves no difference in this file
/// on the next scan.
///
/// `shared_with_other_agents` says another configured agent reads the same
/// stored copy.
pub(crate) fn adopted_agent_markdown(
    adopting: &dyn Agent,
    stored: &CustomAgent,
    disk: &str,
    shared_with_other_agents: bool,
) -> Result<String, AdoptRefusal> {
    let old_name = custom_agent_machine_name(&stored.content, &stored.name);
    let markdown = match adopting.agents_file_ext() {
        "md" => disk.to_string(),
        "toml" => merge_edit_into_stored(
            adopting,
            &stored.content,
            disk,
            &old_name,
            shared_with_other_agents,
        )?,
        _ => return Err(AdoptRefusal::CannotConvert),
    };

    let new_name = custom_agent_machine_name(&markdown, &stored.name);
    if !agent::is_plain_file_name(&new_name) {
        return Err(AdoptRefusal::NotAPlainName(new_name));
    }

    let rendered = adopting.convert_agent_content(&markdown, &new_name);
    let lost = adopting.agent_file_differences(disk, &rendered);
    if lost.is_empty() {
        Ok(markdown)
    } else {
        Err(AdoptRefusal::CannotKeep(lost))
    }
}

/// Apply the fields the user changed in a Codex file to the stored Markdown.
/// A field that is not a value this can apply (`max_turns = "many"`) is left
/// for the round-trip check in [`adopted_agent_markdown`] to refuse.
fn merge_edit_into_stored(
    adopting: &dyn Agent,
    stored: &str,
    disk: &str,
    machine_name: &str,
    shared_with_other_agents: bool,
) -> Result<String, AdoptRefusal> {
    let table = toml::from_str::<toml::Table>(disk).map_err(|_| AdoptRefusal::CannotConvert)?;
    let old_render = adopting.convert_agent_content(stored, machine_name);
    let changed: BTreeSet<String> = adopting
        .agent_file_differences(disk, &old_render)
        .into_iter()
        .collect();

    let sandbox_changed = changed.contains("sandbox_mode");
    let sandbox = table.get("sandbox_mode");
    let read_only = sandbox.and_then(toml::Value::as_str) == Some("read-only");
    let stored_tools = agent::top_level_frontmatter_value(stored, "tools");

    // Codex can only express "read-only or not", so dropping it would drop a
    // real tool list the other agents depend on.
    if sandbox_changed && sandbox.is_none() && stored_tools.is_some_and(|t| t != "inherit") {
        return Err(AdoptRefusal::CannotKeep(vec!["tools".to_string()]));
    }

    if shared_with_other_agents {
        let mut shared = Vec::new();
        if changed.contains("model") {
            shared.push("model".to_string());
        }
        // Another sandbox value cannot be stored at all, which the
        // round-trip check reports more accurately.
        if sandbox_changed && (sandbox.is_none() || read_only) {
            shared.push("sandbox_mode".to_string());
        }
        if !shared.is_empty() {
            return Err(AdoptRefusal::SharedSettings(shared));
        }
    }

    let mut merged = stored.to_string();
    for (toml_key, frontmatter_key) in [
        ("description", "description"),
        ("model_reasoning_effort", "modelReasoningEffort"),
        ("name", "name"),
    ] {
        if changed.contains(toml_key) {
            merged = apply_text_field(merged, table.get(toml_key), toml_key, frontmatter_key)?;
        }
    }

    if changed.contains("max_turns") {
        merged = match table.get("max_turns") {
            None => apply_field(&merged, "max_turns", "maxTurns", None)?,
            Some(toml::Value::Integer(turns)) => {
                apply_field(&merged, "max_turns", "maxTurns", Some(&turns.to_string()))?
            }
            Some(_) => merged,
        };
    }

    if changed.contains("model") {
        // Codex reads a missing `model` as `inherit`.
        let inherit = toml::Value::String("inherit".to_string());
        let model = table.get("model").unwrap_or(&inherit);
        merged = apply_text_field(merged, Some(model), "model", "model")?;
    }

    if sandbox_changed {
        if read_only && stored_tools.is_none() {
            merged = apply_field(&merged, "sandbox_mode", "tools", Some("inherit"))?;
        } else if sandbox.is_none() {
            merged = apply_field(&merged, "sandbox_mode", "tools", None)?;
        }
    }

    if changed.contains("developer_instructions") {
        merged = match table.get("developer_instructions") {
            None => agent::replace_markdown_body(&merged, ""),
            Some(toml::Value::String(text)) => agent::replace_markdown_body(&merged, text.trim()),
            Some(_) => merged,
        };
    }

    Ok(merged)
}

/// Set or remove a frontmatter key.  `field` is what the user touched, so a
/// refusal names that.
fn apply_field(
    markdown: &str,
    field: &str,
    frontmatter_key: &str,
    value: Option<&str>,
) -> Result<String, AdoptRefusal> {
    agent::set_frontmatter_value(markdown, frontmatter_key, value)
        .ok_or_else(|| AdoptRefusal::CannotKeep(vec![field.to_string()]))
}

/// Set a frontmatter key from a TOML text value, or remove it when the value
/// is absent.  Any other value type is left alone.
fn apply_text_field(
    markdown: String,
    value: Option<&toml::Value>,
    field: &str,
    frontmatter_key: &str,
) -> Result<String, AdoptRefusal> {
    match value {
        None => apply_field(&markdown, field, frontmatter_key, None),
        Some(toml::Value::String(text)) => {
            apply_field(&markdown, field, frontmatter_key, Some(text))
        }
        Some(_) => Ok(markdown),
    }
}

/// Another agent's copy of a custom agent, which matches the stored version.
pub(crate) struct OtherAgentFile {
    agent: &'static dyn Agent,
    path: PathBuf,
}

/// The other configured agents' files that match the stored copy.  Adopting
/// changes that copy, so these would otherwise turn into conflicts, and
/// taking one of those would undo the adopt.  A file that really differs is
/// left out: it stays a conflict for the user to decide.
pub(crate) fn other_agent_files_matching(
    project: &Project,
    dir: &Path,
    stored: &CustomAgent,
    adopting: &dyn Agent,
) -> Vec<OtherAgentFile> {
    let machine_name = custom_agent_machine_name(&stored.content, &stored.name);
    let mut files = Vec::new();
    for agent_id in project.agents.iter().filter(|id| *id != adopting.id()) {
        let Some(other) = agent::from_id(agent_id) else {
            continue;
        };
        let Some(agents_dir) = other.agents_dir(dir) else {
            continue;
        };
        let path = agents_dir.join(other.agent_file_name(&machine_name));
        let Ok(Some(disk)) = read_project_text(dir, &path) else {
            continue;
        };
        let old_render = other.convert_agent_content(&stored.content, &machine_name);
        if other.agent_file_differences(&disk, &old_render).is_empty() {
            files.push(OtherAgentFile { agent: other, path });
        }
    }
    files
}

/// Render the adopted agent into each file.  A refused write is an error
/// naming the file.
pub(crate) fn rewrite_other_agent_files(
    dir: &Path,
    files: &[OtherAgentFile],
    adopted: &CustomAgent,
) -> Result<(), String> {
    let machine_name = custom_agent_machine_name(&adopted.content, &adopted.name);
    for file in files {
        let rendered = file.agent.convert_agent_content(&adopted.content, &machine_name);
        write_project_file(dir, &file.path, rendered)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{ClaudeCode, CodexCli};

    /// Keys only Claude reads, a Claude model alias and a real tool list.
    /// None of these can live in a Codex file.
    const STORED: &str = "---\nname: planner\ndescription: Plans\nmodel: sonnet\ntools: Read, Grep\ndisallowedTools: Bash\npermissionMode: plan\ncolor: blue\n---\n\nStored body.\n";

    const PLAIN: &str = "---\nname: planner\ndescription: Plans\n---\nPlan.\n";

    fn stored(content: &str) -> CustomAgent {
        CustomAgent {
            name: "Planner".into(),
            content: content.into(),
        }
    }

    fn rendered(content: &str) -> String {
        CodexCli.convert_agent_content(content, "planner")
    }

    fn adopt(content: &str, disk: &str, shared: bool) -> Result<String, AdoptRefusal> {
        adopted_agent_markdown(&CodexCli, &stored(content), disk, shared)
    }

    fn cannot_keep(setting: &str) -> AdoptRefusal {
        AdoptRefusal::CannotKeep(vec![setting.to_string()])
    }

    #[test]
    fn a_body_edit_keeps_the_stored_frontmatter_byte_identical() {
        let disk = rendered(STORED).replace("Stored body.", "Edited body.");

        let merged = adopt(STORED, &disk, true).unwrap();

        assert_eq!(merged, STORED.replace("Stored body.", "Edited body."));
    }

    #[test]
    fn a_description_edit_changes_only_that_line() {
        let disk = rendered(STORED).replace("\"Plans\"", "\"Reviews\"");

        let merged = adopt(STORED, &disk, true).unwrap();

        assert_eq!(merged, STORED.replace("description: Plans", "description: Reviews"));
    }

    #[test]
    fn a_new_setting_goes_at_the_end_of_the_frontmatter() {
        let disk = format!("max_turns = 5\nmodel_reasoning_effort = \"high\"\n{}", rendered(STORED));

        let merged = adopt(STORED, &disk, true).unwrap();

        assert_eq!(
            merged,
            STORED.replace(
                "color: blue\n",
                "color: blue\nmodelReasoningEffort: high\nmaxTurns: 5\n"
            )
        );
    }

    #[test]
    fn a_deleted_setting_removes_its_line() {
        let with_turns = STORED.replace("color: blue\n", "maxTurns: 5\ncolor: blue\n");
        let disk = rendered(&with_turns).replace("max_turns = 5\n", "");

        assert_eq!(adopt(&with_turns, &disk, true).unwrap(), STORED);
    }

    /// Whitespace around the instructions is not a difference, so the stored
    /// body stays as it was written.
    #[test]
    fn whitespace_around_the_instructions_is_not_an_edit() {
        let disk = rendered(STORED).replace("Stored body.", "\n\n  Stored body.\n\n");

        assert_eq!(adopt(STORED, &disk, true).unwrap(), STORED);
    }

    #[test]
    fn a_model_edit_is_refused_when_other_agents_share_the_copy() {
        let disk = rendered(STORED).replace("gpt-5.4", "gpt-5.5");

        assert_eq!(
            adopt(STORED, &disk, true),
            Err(AdoptRefusal::SharedSettings(vec!["model".to_string()]))
        );
    }

    #[test]
    fn a_model_edit_applies_when_codex_is_alone() {
        let disk = rendered(STORED).replace("gpt-5.4", "gpt-5.5");

        let merged = adopt(STORED, &disk, false).unwrap();

        assert_eq!(merged, STORED.replace("model: sonnet", "model: gpt-5.5"));
    }

    #[test]
    fn a_deleted_model_becomes_inherit_when_codex_is_alone() {
        let disk = rendered(STORED).replace("model = \"gpt-5.4\"\n", "");

        let merged = adopt(STORED, &disk, false).unwrap();

        assert_eq!(merged, STORED.replace("model: sonnet", "model: inherit"));
    }

    /// `sonnet` renders as `gpt-5.4`, so storing it would change the file.
    #[test]
    fn a_claude_alias_in_a_codex_file_is_refused() {
        let disk = rendered(PLAIN).replacen("name =", "model = \"sonnet\"\nname =", 1);

        assert_eq!(adopt(PLAIN, &disk, false), Err(cannot_keep("model")));
    }

    #[test]
    fn a_read_only_sandbox_is_refused_when_other_agents_share_the_copy() {
        let disk = format!("sandbox_mode = \"read-only\"\n{}", rendered(PLAIN));

        assert_eq!(
            adopt(PLAIN, &disk, true),
            Err(AdoptRefusal::SharedSettings(vec!["sandbox_mode".to_string()]))
        );
    }

    #[test]
    fn a_read_only_sandbox_sets_tools_when_codex_is_alone() {
        let disk = format!("sandbox_mode = \"read-only\"\n{}", rendered(PLAIN));

        let merged = adopt(PLAIN, &disk, false).unwrap();

        assert_eq!(
            merged,
            PLAIN.replace("description: Plans\n", "description: Plans\ntools: inherit\n")
        );
    }

    #[test]
    fn removing_the_sandbox_removes_tools_inherit_when_codex_is_alone() {
        let with_tools = PLAIN.replace("description: Plans\n", "description: Plans\ntools: inherit\n");
        let disk = rendered(&with_tools).replace("sandbox_mode = \"read-only\"\n", "");

        assert_eq!(adopt(&with_tools, &disk, false).unwrap(), PLAIN);
    }

    /// A real tool list cannot be expressed in Codex, in any project.
    #[test]
    fn removing_the_sandbox_cannot_lose_a_stored_tool_list() {
        let disk = rendered(STORED).replace("sandbox_mode = \"read-only\"\n", "");

        assert_eq!(adopt(STORED, &disk, false), Err(cannot_keep("tools")));
        assert_eq!(adopt(STORED, &disk, true), Err(cannot_keep("tools")));
    }

    /// Any other sandbox value cannot be stored at all, so "use another
    /// agent" would be the wrong advice.
    #[test]
    fn another_sandbox_value_is_refused_in_any_project() {
        let disk = format!("sandbox_mode = \"workspace-write\"\n{}", rendered(PLAIN));

        assert_eq!(adopt(PLAIN, &disk, false), Err(cannot_keep("sandbox_mode")));
        assert_eq!(adopt(PLAIN, &disk, true), Err(cannot_keep("sandbox_mode")));
    }

    #[test]
    fn an_unknown_key_is_refused() {
        let disk = format!("custom_flag = true\n{}", rendered(STORED));

        assert_eq!(adopt(STORED, &disk, false), Err(cannot_keep("custom_flag")));
    }

    #[test]
    fn a_file_that_does_not_parse_cannot_be_converted() {
        assert_eq!(adopt(STORED, "not = = toml", false), Err(AdoptRefusal::CannotConvert));
    }

    #[test]
    fn a_name_that_is_not_a_plain_file_name_is_refused() {
        let disk = rendered(PLAIN).replace("name = \"planner\"", "name = \"../planner\"");
        assert_eq!(
            adopt(PLAIN, &disk, false),
            Err(AdoptRefusal::NotAPlainName("../planner".to_string()))
        );

        assert_eq!(
            adopted_agent_markdown(
                &ClaudeCode,
                &stored(PLAIN),
                "---\nname: ../planner\n---\nPlan.\n",
                false
            ),
            Err(AdoptRefusal::NotAPlainName("../planner".to_string()))
        );
    }

    /// A Markdown file is the full copy, so nothing is merged.
    #[test]
    fn a_markdown_file_is_adopted_whole() {
        let disk = "---\nname: planner\ncolor: red\n---\nAll new.\n";

        assert_eq!(
            adopted_agent_markdown(&ClaudeCode, &stored(STORED), disk, true).unwrap(),
            disk
        );
    }

    /// A value holding a line break could add keys of its own or close the
    /// frontmatter block, so it is refused, naming the field.
    #[test]
    fn a_value_that_cannot_stay_on_one_line_is_refused_naming_the_field() {
        for value in ["x\\n---\\nfoo: bar", "a\\rb", "a\\u0085b", "a\\u2028b", "a\\u2029b", "a\\u0007b"] {
            let disk = rendered(PLAIN).replace("\"Plans\"", &format!("\"{value}\""));
            assert_eq!(adopt(PLAIN, &disk, false), Err(cannot_keep("description")), "{value}");
        }

        let disk = format!("model_reasoning_effort = \"a\\nb\"\n{}", rendered(PLAIN));
        assert_eq!(adopt(PLAIN, &disk, false), Err(cannot_keep("model_reasoning_effort")));
    }

    #[test]
    fn a_value_with_both_quote_kinds_that_needs_quoting_is_refused() {
        let disk = rendered(PLAIN).replace("\"Plans\"", "\"a: 'b\\\"c\"");
        assert_eq!(adopt(PLAIN, &disk, false), Err(cannot_keep("description")));
    }

    /// Replacing only the first line of a block value would leave the rest
    /// behind, so the edit is refused, naming what the user changed.
    #[test]
    fn editing_a_stored_block_value_is_refused_naming_the_field() {
        let block = PLAIN.replace("description: Plans\n", "description: |\n  Plans\n  more\n");
        let disk = rendered(&block).replace("description = \"|\"", "description = \"Edited\"");
        assert_eq!(adopt(&block, &disk, false), Err(cannot_keep("description")));
    }

    #[test]
    fn a_body_edit_leaves_a_stored_block_value_alone() {
        let block = PLAIN.replace("description: Plans\n", "description: |\n  Plans\n  more\n");
        let disk = rendered(&block).replace("Plan.", "Edited.");

        let merged = adopt(&block, &disk, false).unwrap();

        assert_eq!(merged, block.replace("Plan.", "Edited."));
    }

    /// A stored tool list written as a block cannot be removed by its first
    /// line, and an indented `tools` under another key is not the tool list.
    #[test]
    fn removing_the_sandbox_reads_only_the_top_level_tools_line() {
        let listed = PLAIN.replace("description: Plans\n", "description: Plans\ntools:\n  - Read\n");
        let disk = rendered(&listed).replace("sandbox_mode = \"read-only\"\n", "");
        assert_eq!(
            adopt(&listed, &disk, false),
            Err(cannot_keep("tools"))
        );

        let nested = PLAIN.replace("description: Plans\n", "description: Plans\nhooks:\n  tools: Read\n");
        let disk = rendered(&nested).replace("sandbox_mode = \"read-only\"\n", "");
        assert_eq!(adopt(&nested, &disk, false), Err(cannot_keep("sandbox_mode")));
    }

    #[test]
    fn a_nested_key_is_never_edited() {
        let nested = "---\nhooks:\n  description: nested\nname: planner\ndescription: Plans\n---\nPlan.\n";
        let disk = rendered(nested).replace("\"Plans\"", "\"Edited\"");

        let merged = adopt(nested, &disk, false).unwrap();

        assert_eq!(merged, nested.replace("description: Plans", "description: Edited"));
    }

    /// `parse_frontmatter` strips quotes and whitespace, so these must be
    /// quoted when written or the round trip refuses a value it can hold.
    #[test]
    fn values_the_frontmatter_parser_would_change_survive() {
        for value in [r#"\"Plans\""#, "'Plans'", "Plans "] {
            let disk = rendered(PLAIN).replace("\"Plans\"", &format!("\"{value}\""));

            let merged = adopt(PLAIN, &disk, false).unwrap();

            let new_render = rendered(&merged);
            assert_eq!(
                CodexCli.agent_file_differences(&disk, &new_render),
                Vec::<String>::new(),
                "{value}"
            );
        }
    }
}
