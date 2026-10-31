//! Adopting an edited custom agent file: working out what the stored Markdown
//! becomes, and keeping the other agents' files in step with it.

use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use crate::agent::{self, Agent};
use crate::core::{
    agent_machine_name, agent_machine_slug, custom_agent_machine_name, extract_agent_name,
    CustomAgent, Project,
};

use super::helpers::{project_write_refusal, read_project_text, write_project_file};

/// Why a file cannot be adopted.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AdoptRefusal {
    /// The format has no conversion back to Markdown, or the file does not
    /// parse.
    CannotConvert,
    /// Changes that would not survive being stored and rendered again.
    CannotKeep(Vec<String>),
    /// Settings stored once for every agent, while other agents are
    /// configured.
    SharedSettings(Vec<String>),
    /// Removing `sandbox_mode` would lose a real tool list.
    WouldDropTools,
    /// The stored value of this field runs over several lines, so one line
    /// of a Codex file cannot replace it.
    MultiLineStoredValue(String),
    /// The file would sync under a different name.
    NameChanged,
    /// The stored name itself fails validation.
    InvalidStoredName(String),
}

impl AdoptRefusal {
    /// The reason to show the user.  `agent_label` names the file's agent.
    pub(crate) fn message(&self, agent_label: &str) -> String {
        match self {
            Self::CannotConvert => format!(
                "Automatic cannot convert this {agent_label} file back to Markdown. Use Overwrite to replace the file with Automatic's copy."
            ),
            Self::CannotKeep(changes) => format!(
                "Automatic's copy cannot hold these {agent_label} changes: {}. Undo them in the file and adopt again, or use Overwrite to replace the file with Automatic's copy.",
                changes.join(", ")
            ),
            Self::SharedSettings(settings) => format!(
                "Automatic stores one {} for every agent in this project. Change it in Automatic's agent editor, or use Overwrite to discard this edit.",
                settings.join(" and ")
            ),
            Self::WouldDropTools => {
                "Removing sandbox_mode would drop the stored tools list, so it cannot be adopted. Restore sandbox_mode in the file, or use Overwrite to replace the file with Automatic's copy."
                    .to_string()
            }
            Self::MultiLineStoredValue(field) => format!(
                "Automatic's copy writes {field} over several lines, so this file cannot change it. Change it in the agent's content on the Agents tab, or use Overwrite to replace the file with Automatic's copy."
            ),
            Self::InvalidStoredName(reason) => reason.clone(),
            Self::NameChanged => {
                "Adopting cannot rename an agent. Restore the name in the file and adopt, then change `name:` in the agent's content on the Agents tab."
                    .to_string()
            }
        }
    }
}

/// The Markdown to store when adopting `disk`, or why it cannot be adopted.
///
/// A Markdown file is the full copy, so it is taken whole.  Other formats
/// hold only part of it (Codex TOML has no `disallowedTools`, and renders
/// `model: sonnet` as `gpt-5.4`), so the stored Markdown is kept and only
/// what the user changed is applied.  Either way the result is rendered back
/// and compared with `disk`.  What passes leaves no difference in this file
/// on the next scan.  The machine name never changes here: a rename moves
/// files and can collide with another agent, so adopting refuses it.
///
/// `shared_with_other_agents` says another configured agent reads the same
/// stored copy.
pub(crate) fn adopted_agent_markdown(
    adopting: &dyn Agent,
    stored: &CustomAgent,
    disk: &str,
    shared_with_other_agents: bool,
) -> Result<String, AdoptRefusal> {
    let machine_name =
        custom_agent_machine_name(stored).map_err(AdoptRefusal::InvalidStoredName)?;
    let markdown = match adopting.agents_file_ext() {
        "md" => whole_markdown_file(disk, stored, &machine_name)?,
        "toml" => merge_edit_into_stored(
            adopting,
            &stored.content,
            disk,
            &machine_name,
            shared_with_other_agents,
        )?,
        _ => return Err(AdoptRefusal::CannotConvert),
    };

    let rendered = adopting.convert_agent_content(&markdown, &machine_name);
    let lost = adopting.agent_file_differences(disk, &rendered);
    if lost.is_empty() {
        Ok(markdown)
    } else {
        Err(AdoptRefusal::CannotKeep(lost))
    }
}

/// A Markdown file to adopt whole, without any byte order mark.  Its name
/// must read the same to every reader, because the machine name decides
/// where the file syncs.
fn whole_markdown_file(
    disk: &str,
    stored: &CustomAgent,
    machine_name: &str,
) -> Result<String, AdoptRefusal> {
    let disk = disk.strip_prefix('\u{feff}').unwrap_or(disk);
    if let Some(key) = agent::frontmatter_key_in_another_form(disk) {
        return Err(AdoptRefusal::CannotKeep(vec![key.to_string()]));
    }
    let (frontmatter, _) = agent::parse_frontmatter(disk);
    let parsed_name = frontmatter
        .get("name")
        .map(String::as_str)
        .filter(|name| !name.is_empty());
    if agent::top_level_frontmatter_key_count(disk, "name") > 1
        || parsed_name != extract_agent_name(disk).as_deref()
    {
        return Err(AdoptRefusal::CannotKeep(vec!["name".to_string()]));
    }
    if agent_machine_name("Custom agent", disk, &stored.name).as_deref() != Ok(machine_name) {
        return Err(AdoptRefusal::NameChanged);
    }
    Ok(disk.to_string())
}

/// What the user changed in a Codex file.
struct CodexEdit {
    table: toml::Table,
    changed: BTreeSet<String>,
}

impl CodexEdit {
    fn changed(&self, field: &str) -> bool {
        self.changed.contains(field)
    }

    fn removes_sandbox(&self) -> bool {
        self.changed("sandbox_mode") && !self.table.contains_key("sandbox_mode")
    }

    fn sets_read_only(&self) -> bool {
        self.changed("sandbox_mode")
            && self.table.get("sandbox_mode").and_then(toml::Value::as_str) == Some("read-only")
    }
}

/// Apply the fields the user changed in a Codex file to the stored Markdown.
fn merge_edit_into_stored(
    adopting: &dyn Agent,
    stored: &str,
    disk: &str,
    machine_name: &str,
    shared_with_other_agents: bool,
) -> Result<String, AdoptRefusal> {
    let table = toml::from_str::<toml::Table>(disk).map_err(|_| AdoptRefusal::CannotConvert)?;
    let old_render = adopting.convert_agent_content(stored, machine_name);
    let changed = adopting
        .agent_file_differences(disk, &old_render)
        .into_iter()
        .collect();
    let edit = CodexEdit { table, changed };

    refuse_unsupported_edit(&edit, stored, machine_name, shared_with_other_agents)?;
    apply_edit(&edit, stored)
}

/// Refuse an edit that cannot be applied at all, before touching the copy.
fn refuse_unsupported_edit(
    edit: &CodexEdit,
    stored: &str,
    machine_name: &str,
    shared_with_other_agents: bool,
) -> Result<(), AdoptRefusal> {
    // A missing name is filled in from the stored copy, so it is no rename.
    if edit.changed("name") {
        if let Some(toml::Value::String(name)) = edit.table.get("name") {
            if agent_machine_slug(name) != machine_name {
                return Err(AdoptRefusal::NameChanged);
            }
        }
    }

    // Codex can only express "read-only or not", so dropping it would drop a
    // real tool list the other agents depend on.
    let tools = agent::top_level_frontmatter_value(stored, "tools");
    if edit.removes_sandbox() && tools.is_some_and(|tools| tools != "inherit") {
        return Err(AdoptRefusal::WouldDropTools);
    }

    if shared_with_other_agents {
        let mut shared = Vec::new();
        if edit.changed("model") {
            shared.push("model".to_string());
        }
        // Another sandbox value cannot be stored at all, which the
        // round-trip check reports more accurately.
        if edit.removes_sandbox() || edit.sets_read_only() {
            shared.push("sandbox_mode".to_string());
        }
        if !shared.is_empty() {
            return Err(AdoptRefusal::SharedSettings(shared));
        }
    }
    Ok(())
}

/// Write each changed field into the stored Markdown.  A field that is not a
/// value this can apply (`max_turns = "many"`) is left for the round-trip
/// check in [`adopted_agent_markdown`] to refuse.
fn apply_edit(edit: &CodexEdit, stored: &str) -> Result<String, AdoptRefusal> {
    let mut merged = stored.to_string();
    for (field, frontmatter_key) in [
        ("description", "description"),
        ("model_reasoning_effort", "modelReasoningEffort"),
        ("name", "name"),
    ] {
        if edit.changed(field) {
            merged = apply_text_field(merged, edit.table.get(field), field, frontmatter_key)?;
        }
    }

    if edit.changed("max_turns") {
        merged = match edit.table.get("max_turns") {
            None => apply_field(&merged, "max_turns", "maxTurns", None)?,
            Some(toml::Value::Integer(turns)) => {
                agent::set_frontmatter_integer(&merged, "maxTurns", *turns)
                    .map_err(|refusal| field_refusal(refusal, "max_turns"))?
            }
            Some(_) => merged,
        };
    }

    if edit.changed("model") {
        // Codex reads a missing `model` as `inherit`.
        let inherit = toml::Value::String("inherit".to_string());
        let model = edit.table.get("model").unwrap_or(&inherit);
        merged = apply_text_field(merged, Some(model), "model", "model")?;
    }

    let tools = agent::top_level_frontmatter_value(stored, "tools");
    if edit.sets_read_only() && tools.is_none() {
        merged = apply_field(&merged, "sandbox_mode", "tools", Some("inherit"))?;
    } else if edit.removes_sandbox() {
        merged = apply_field(&merged, "sandbox_mode", "tools", None)?;
    }

    if edit.changed("developer_instructions") {
        merged = match edit.table.get("developer_instructions") {
            None => agent::replace_markdown_body(&merged, ""),
            Some(toml::Value::String(text)) => {
                agent::replace_markdown_body(&merged, &with_line_endings_of(stored, text.trim()))
            }
            Some(_) => merged,
        };
    }

    Ok(merged)
}

/// `text` with the line endings `stored` uses, so one copy never mixes them.
fn with_line_endings_of(stored: &str, text: &str) -> String {
    let unix = text.replace("\r\n", "\n");
    if stored.contains("\r\n") {
        unix.replace('\n', "\r\n")
    } else {
        unix
    }
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
        .map_err(|refusal| field_refusal(refusal, field))
}

fn field_refusal(refusal: agent::FrontmatterEditRefusal, field: &str) -> AdoptRefusal {
    match refusal {
        agent::FrontmatterEditRefusal::UnsafeValue => {
            AdoptRefusal::CannotKeep(vec![field.to_string()])
        }
        agent::FrontmatterEditRefusal::MultiLineValue => {
            AdoptRefusal::MultiLineStoredValue(field.to_string())
        }
    }
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
    let Ok(machine_name) = custom_agent_machine_name(stored) else {
        return Vec::new();
    };
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

/// Why each file cannot be rewritten, without changing any of them.  Adopt
/// checks this before saving, so a blocked file leaves nothing half done.
pub(crate) fn unwritable_other_agent_files(dir: &Path, files: &[OtherAgentFile]) -> Vec<String> {
    files
        .iter()
        .filter_map(|file| {
            project_write_refusal(dir, &file.path).or_else(|| {
                let opened = OpenOptions::new().write(true).open(&file.path);
                opened
                    .err()
                    .map(|e| format!("{}: {e}", file.path.display()))
            })
        })
        .collect()
}

/// Render the adopted agent into each file.  Every file is tried, and the
/// failures come back naming each one.
pub(crate) fn rewrite_other_agent_files(
    dir: &Path,
    files: &[OtherAgentFile],
    adopted: &CustomAgent,
) -> Result<(), Vec<String>> {
    let machine_name = custom_agent_machine_name(adopted).map_err(|reason| vec![reason])?;
    let failures: Vec<String> = files
        .iter()
        .filter_map(|file| {
            let rendered = file
                .agent
                .convert_agent_content(&adopted.content, &machine_name);
            write_project_file(dir, &file.path, rendered).err()
        })
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
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

        assert_eq!(
            merged,
            STORED.replace("description: Plans", "description: Reviews")
        );
    }

    #[test]
    fn a_new_setting_goes_at_the_end_of_the_frontmatter() {
        let disk = format!(
            "max_turns = 5\nmodel_reasoning_effort = \"high\"\n{}",
            rendered(STORED)
        );

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
            Err(AdoptRefusal::SharedSettings(vec![
                "sandbox_mode".to_string()
            ]))
        );
    }

    #[test]
    fn a_read_only_sandbox_sets_tools_when_codex_is_alone() {
        let disk = format!("sandbox_mode = \"read-only\"\n{}", rendered(PLAIN));

        let merged = adopt(PLAIN, &disk, false).unwrap();

        assert_eq!(
            merged,
            PLAIN.replace(
                "description: Plans\n",
                "description: Plans\ntools: inherit\n"
            )
        );
    }

    #[test]
    fn removing_the_sandbox_removes_tools_inherit_when_codex_is_alone() {
        let with_tools = PLAIN.replace(
            "description: Plans\n",
            "description: Plans\ntools: inherit\n",
        );
        let disk = rendered(&with_tools).replace("sandbox_mode = \"read-only\"\n", "");

        assert_eq!(adopt(&with_tools, &disk, false).unwrap(), PLAIN);
    }

    /// A real tool list cannot be expressed in Codex, in any project.
    #[test]
    fn removing_the_sandbox_cannot_lose_a_stored_tool_list() {
        let disk = rendered(STORED).replace("sandbox_mode = \"read-only\"\n", "");

        assert_eq!(
            adopt(STORED, &disk, false),
            Err(AdoptRefusal::WouldDropTools)
        );
        assert_eq!(
            adopt(STORED, &disk, true),
            Err(AdoptRefusal::WouldDropTools)
        );
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
        assert_eq!(
            adopt(STORED, "not = = toml", false),
            Err(AdoptRefusal::CannotConvert)
        );
    }

    /// A rename moves files and can collide with another agent, so adopting
    /// refuses it for every format, whatever else the file changes.
    #[test]
    fn adopting_cannot_rename_an_agent() {
        for name in ["reviewer", "../planner", "", "planner ", "\\\"planner\\\""] {
            let disk = rendered(PLAIN).replace("name = \"planner\"", &format!("name = \"{name}\""));
            assert_eq!(
                adopt(PLAIN, &disk, false),
                Err(AdoptRefusal::NameChanged),
                "{name}"
            );
        }

        for name in ["reviewer", "../planner"] {
            assert_eq!(
                adopted_agent_markdown(
                    &ClaudeCode,
                    &stored(PLAIN),
                    &format!("---\nname: {name}\n---\nPlan.\n"),
                    false
                ),
                Err(AdoptRefusal::NameChanged),
                "{name}"
            );
        }
    }

    /// The name counts as changed only when the file name would change.
    #[test]
    fn a_name_spelled_differently_with_the_same_machine_name_is_kept() {
        let disk = rendered(PLAIN).replace("name = \"planner\"", "name = \"Planner\"");
        let merged = adopt(PLAIN, &disk, false).unwrap();
        assert_eq!(merged, PLAIN.replace("name: planner", "name: Planner"));

        let crlf = "---\r\nname: Planner\r\n---\r\nPlan.\r\n";
        assert_eq!(
            adopted_agent_markdown(&ClaudeCode, &stored(PLAIN), crlf, false).unwrap(),
            crlf
        );
    }

    /// A deleted name is filled in from the stored copy, so it is no rename.
    #[test]
    fn a_missing_name_in_a_codex_file_is_not_a_rename() {
        let disk = rendered(PLAIN).replace("name = \"planner\"\n", "");
        assert_eq!(adopt(PLAIN, &disk, false).unwrap(), PLAIN);
    }

    #[test]
    fn each_refusal_says_what_to_do() {
        const OVERWRITE: &str = "use Overwrite to replace the file with Automatic's copy.";
        for (refusal, says) in [
            (AdoptRefusal::CannotConvert, "Use Overwrite"),
            (AdoptRefusal::WouldDropTools, OVERWRITE),
            (
                AdoptRefusal::CannotKeep(vec!["a".into(), "b".into()]),
                OVERWRITE,
            ),
            (
                AdoptRefusal::CannotKeep(vec!["a".into(), "b".into()]),
                "changes: a, b.",
            ),
            (
                AdoptRefusal::MultiLineStoredValue("description".into()),
                "on the Agents tab",
            ),
            (
                AdoptRefusal::MultiLineStoredValue("description".into()),
                OVERWRITE,
            ),
            (
                AdoptRefusal::SharedSettings(vec!["model".into()]),
                "one model for every agent",
            ),
        ] {
            let message = refusal.message("Codex CLI");
            assert!(message.contains(says), "{message}");
        }
        assert_eq!(
            AdoptRefusal::NameChanged.message("Codex CLI"),
            "Adopting cannot rename an agent. Restore the name in the file and adopt, then change `name:` in the agent's content on the Agents tab."
        );
    }

    /// A nested `name` is what the Codex render shows, but with `name`
    /// unchanged it is no rename, so a body edit must not be refused.
    #[test]
    fn a_nested_name_does_not_refuse_a_body_edit() {
        let nested = "---\nname: planner\nhooks:\n  name: inner\n---\nPlan.\n";
        let disk = rendered(nested).replace("Plan.", "Edited.");

        assert_eq!(
            adopt(nested, &disk, false).unwrap(),
            nested.replace("Plan.", "Edited.")
        );
    }

    /// Two readers could send this file to two names, so it is refused
    /// rather than picking one.
    #[test]
    fn a_markdown_file_with_an_ambiguous_name_is_refused() {
        for disk in [
            "---\nname: planner\nname: planner\n---\nPlan.\n",
            "---\nname: planner\nname: reviewer\n---\nPlan.\n",
            "---\nname: planner\nhooks:\n  name: inner\n---\nPlan.\n",
        ] {
            assert_eq!(
                adopted_agent_markdown(&ClaudeCode, &stored(PLAIN), disk, false),
                Err(cannot_keep("name")),
                "{disk}"
            );
        }
    }

    /// Each of these sets a key a YAML reader sees and `parse_frontmatter`
    /// does not, so the file would sync under one name and load as another.
    #[test]
    fn a_markdown_file_with_another_form_of_a_key_is_refused() {
        for (block, key) in [
            ("\"name\": other\n", "name"),
            ("'name': other\n", "name"),
            ("\"n\\x61me\": other\n", "name"),
            ("name: planner\n? name\n: other\n", "name"),
            ("{name: other}\n", "name"),
            ("name: planner\n<<: {name: other}\n", "name"),
            ("name: planner\n<<: *defaults\n", "name"),
            ("&anchor name: other\n", "name"),
            ("!!str name: other\n", "name"),
            ("name: planner\n  and more\n", "name"),
            ("name: >\n  planner\n", "name"),
            ("name: |\n  planner\n", "name"),
            ("name:\n  planner\n", "name"),
            ("name: planner\n\"automatic-managed\": false\n", "automatic-managed"),
            ("name: planner\n'automatic-managed': false\n", "automatic-managed"),
        ] {
            let disk = format!("---\n{block}---\nBody.\n");

            assert_eq!(
                adopted_agent_markdown(&ClaudeCode, &stored(PLAIN), &disk, false),
                Err(cannot_keep(key)),
                "{block}"
            );
        }
    }

    #[test]
    fn a_markdown_file_with_ordinary_keys_is_kept_whole() {
        let disk = "---\r\nname: planner\r\n# note\r\n\"other key\": 1\r\nhooks:\r\n  - name: nested\r\n_private: x\r\n---\r\nBody.\r\n";

        assert_eq!(
            adopted_agent_markdown(&ClaudeCode, &stored(PLAIN), disk, false).as_deref(),
            Ok(disk)
        );
    }

    #[test]
    fn a_markdown_file_with_automatic_managed_false_is_refused() {
        let disk = "---\nname: planner\nautomatic-managed: false\n---\nEdited.\n";

        assert_eq!(
            adopted_agent_markdown(&ClaudeCode, &stored(PLAIN), disk, false),
            Err(cannot_keep("automatic-managed"))
        );
    }

    #[test]
    fn a_byte_order_mark_is_stripped_from_a_markdown_file() {
        let disk = "---\nname: planner\n---\nEdited.\n";

        assert_eq!(
            adopted_agent_markdown(
                &ClaudeCode,
                &stored(PLAIN),
                &format!("\u{feff}{disk}"),
                false
            )
            .unwrap(),
            disk
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
        for value in [
            "x\\n---\\nfoo: bar",
            "a\\rb",
            "a\\u0085b",
            "a\\u2028b",
            "a\\u2029b",
            "a\\u0007b",
        ] {
            let disk = rendered(PLAIN).replace("\"Plans\"", &format!("\"{value}\""));
            assert_eq!(
                adopt(PLAIN, &disk, false),
                Err(cannot_keep("description")),
                "{value}"
            );
        }

        let disk = format!("model_reasoning_effort = \"a\\nb\"\n{}", rendered(PLAIN));
        assert_eq!(
            adopt(PLAIN, &disk, false),
            Err(cannot_keep("model_reasoning_effort"))
        );
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
        assert_eq!(
            adopt(&block, &disk, false),
            Err(AdoptRefusal::MultiLineStoredValue(
                "description".to_string()
            ))
        );
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
        let listed = PLAIN.replace(
            "description: Plans\n",
            "description: Plans\ntools:\n  - Read\n",
        );
        let disk = rendered(&listed).replace("sandbox_mode = \"read-only\"\n", "");
        assert_eq!(
            adopt(&listed, &disk, false),
            Err(AdoptRefusal::WouldDropTools)
        );

        let nested = PLAIN.replace(
            "description: Plans\n",
            "description: Plans\nhooks:\n  tools: Read\n",
        );
        let disk = rendered(&nested).replace("sandbox_mode = \"read-only\"\n", "");
        assert_eq!(
            adopt(&nested, &disk, false),
            Err(cannot_keep("sandbox_mode"))
        );
    }

    #[test]
    fn a_nested_key_is_never_edited() {
        let nested =
            "---\nhooks:\n  description: nested\nname: planner\ndescription: Plans\n---\nPlan.\n";
        let disk = rendered(nested).replace("\"Plans\"", "\"Edited\"");

        let merged = adopt(nested, &disk, false).unwrap();

        assert_eq!(
            merged,
            nested.replace("description: Plans", "description: Edited")
        );
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

    /// A file can fail after the check passed.  The rest must still be
    /// written, every failure named, and a second run must finish the job.
    #[cfg(unix)]
    #[test]
    fn rewrite_tries_every_file_and_can_be_run_again() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let project = Project {
            name: "rewrite".into(),
            directory: dir.path().display().to_string(),
            agents: vec!["claude".into(), "copilot".into(), "codex".into()],
            ..Default::default()
        };
        let old = stored(PLAIN);
        let claude = dir.path().join(".claude/agents/planner.md");
        let copilot = dir.path().join(".github/agents/planner.agent.md");
        for path in [&claude, &copilot] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, ClaudeCode.convert_agent_content(PLAIN, "planner")).unwrap();
        }
        let files = other_agent_files_matching(&project, dir.path(), &old, &CodexCli);
        assert_eq!(files.len(), 2);
        fs::set_permissions(&claude, fs::Permissions::from_mode(0o444)).unwrap();
        let adopted = stored(&PLAIN.replace("Plan.", "Edited."));

        let failed = rewrite_other_agent_files(dir.path(), &files, &adopted).unwrap_err();

        assert_eq!(failed.len(), 1, "{failed:?}");
        assert!(
            failed[0].contains(".claude/agents/planner.md"),
            "{failed:?}"
        );
        assert!(fs::read_to_string(&copilot).unwrap().contains("Edited."));
        assert!(fs::read_to_string(&claude).unwrap().contains("Plan."));

        fs::set_permissions(&claude, fs::Permissions::from_mode(0o644)).unwrap();
        rewrite_other_agent_files(dir.path(), &files, &adopted).unwrap();
        assert!(fs::read_to_string(&claude).unwrap().contains("Edited."));
    }

    /// A name that fails main's validation never reaches a path.
    #[test]
    fn other_agent_files_use_the_validated_name() {
        let dir = tempfile::tempdir().unwrap();
        let project = Project {
            name: "invalid".into(),
            directory: dir.path().display().to_string(),
            agents: vec!["claude".into(), "codex".into()],
            ..Default::default()
        };
        let invalid = stored("---\nname: ../planner\n---\nPlan.\n");
        let outside = dir.path().join(".claude/planner.md");
        std::fs::create_dir_all(dir.path().join(".claude/agents")).unwrap();
        std::fs::write(
            &outside,
            ClaudeCode.convert_agent_content(&invalid.content, "../planner"),
        )
        .unwrap();

        let files = other_agent_files_matching(&project, dir.path(), &invalid, &CodexCli);

        assert!(files.is_empty());
        let failed = rewrite_other_agent_files(dir.path(), &[], &invalid).unwrap_err();
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert_eq!(
            adopted_agent_markdown(&CodexCli, &invalid, "name = \"x\"\n", false),
            Err(AdoptRefusal::InvalidStoredName(failed[0].clone()))
        );
    }

    /// Windows editors re-save with CRLF.  That is no edit, so nothing about
    /// the stored copy may change.
    #[test]
    fn a_crlf_resave_is_not_a_difference() {
        let render = rendered(&STORED.replace("Stored body.", "One.\nTwo."));
        let disk = render.replace('\n', "\r\n");

        assert_eq!(
            CodexCli.agent_file_differences(&disk, &render),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_crlf_edit_is_stored_with_the_copys_line_endings() {
        let multi = STORED.replace("Stored body.", "One.\nTwo.");
        let disk = rendered(&multi)
            .replace("Two.", "Edited.")
            .replace('\n', "\r\n");

        let merged = adopt(&multi, &disk, true).unwrap();

        assert_eq!(merged, multi.replace("Two.", "Edited."));
        assert!(!merged.contains('\r'), "{merged:?}");
    }

    #[test]
    fn an_lf_edit_of_a_crlf_copy_stores_no_mixed_line_endings() {
        let crlf = STORED
            .replace("Stored body.", "One.\nTwo.")
            .replace('\n', "\r\n");
        let disk = rendered(&crlf)
            .replace("\r\n", "\n")
            .replace("Two.", "Edited.");

        let merged = adopt(&crlf, &disk, true).unwrap();

        assert_eq!(merged, crlf.replace("Two.", "Edited."));
        assert!(!merged.replace("\r\n", "").contains('\n'), "{merged:?}");
    }
}
