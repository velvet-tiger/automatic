//! AI-assisted generation of new library assets (skills, commands, rules,
//! sub-agents).
//!
//! Mirrors the call-shape of `ai_generate_instruction` in
//! `commands/project_files.rs` — a single LLM call via `core::ai::chat` with a
//! kind-specific system prompt that reminds the model of the on-disk format.
//! The model returns raw markdown; the frontend reviews and saves through the
//! existing per-kind save commands.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetKind {
    Skill,
    Command,
    Rule,
    Subagent,
}

impl AssetKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "skill" => Ok(Self::Skill),
            "command" => Ok(Self::Command),
            "rule" => Ok(Self::Rule),
            "subagent" => Ok(Self::Subagent),
            other => Err(format!(
                "Unknown asset kind '{}'. Expected one of: skill, command, rule, subagent.",
                other
            )),
        }
    }
}

/// Generate a new library asset of the given kind from a free-text
/// description. Optionally revise a previous draft using user feedback.
///
/// Returns the raw markdown body the model produced. The caller is
/// responsible for saving it via the kind-specific Tauri command after the
/// user has reviewed it.
pub async fn generate_library_asset(
    kind: AssetKind,
    description: &str,
    previous_attempt: Option<&str>,
    feedback: Option<&str>,
) -> Result<String, String> {
    if description.trim().is_empty() && previous_attempt.is_none() {
        return Err("Describe what you want the assistant to generate.".into());
    }

    let system = system_prompt(kind);
    let user_msg = user_prompt(description, previous_attempt, feedback);

    crate::core::ai::chat(
        vec![crate::core::ai::AiMessage {
            role: "user".into(),
            content: user_msg,
        }],
        None,
        None,
        Some(system),
        Some(4096),
    )
    .await
}

fn user_prompt(
    description: &str,
    previous_attempt: Option<&str>,
    feedback: Option<&str>,
) -> String {
    match (previous_attempt, feedback) {
        (Some(prev), Some(fb)) => format!(
            "The user originally asked for the following:\n\n{}\n\n\
             A previous draft was produced:\n\n<previous_draft>\n{}\n</previous_draft>\n\n\
             The user has given this feedback for the next revision:\n\n<feedback>\n{}\n</feedback>\n\n\
             Produce the full revised file now.",
            description.trim(),
            prev.trim(),
            fb.trim()
        ),
        _ => format!(
            "Generate the file now from the following request:\n\n{}",
            description.trim()
        ),
    }
}

fn system_prompt(kind: AssetKind) -> String {
    let shared = "You are an expert author of agent configuration files for the Automatic \
        desktop app. Your output is the raw file contents only — no preamble, no \
        code fences, no meta-commentary. Use Automatic's machine_name convention \
        (lowercase letters, digits, and hyphens only) wherever a slug is required. \
        Keep wording concrete and specific to the user's request; do not invent \
        unrelated capabilities.";

    let kind_specific = match kind {
        AssetKind::Skill => {
            "You are producing a SKILL.md file.\n\
             \n\
             Required structure:\n\
             1. YAML frontmatter delimited by `---` lines, containing exactly two keys:\n\
                - `name`: machine_name slug (lowercase, hyphenated).\n\
                - `description`: a single-sentence description that begins with a \
                  verb and clearly states the activation conditions (when the skill \
                  should fire). This is the trigger an agent reads to decide whether \
                  to load the skill, so be precise about the situations it covers.\n\
             2. A blank line.\n\
             3. An H1 heading with the skill's display name.\n\
             4. A `## When to Apply` section listing the activation triggers as bullets.\n\
             5. A `## Steps` or `## Guidance` section with the actionable content.\n\
             \n\
             Example shape:\n\
             ---\n\
             name: pennant-development\n\
             description: Manages feature flags with Laravel Pennant. Activates when \
             creating, checking, or toggling feature flags.\n\
             ---\n\
             \n\
             # Pennant Features\n\
             \n\
             ## When to Apply\n\
             ...\n"
        }
        AssetKind::Command => {
            "You are producing a slash-command markdown file.\n\
             \n\
             Required structure:\n\
             1. YAML frontmatter delimited by `---` lines, containing exactly one key:\n\
                - `description`: a short single-sentence summary of what the command does.\n\
             2. A blank line.\n\
             3. The command body — the prompt or instructions the agent will execute \
                when the user invokes the command. Use second-person voice \
                (\"Run X, then do Y\"). Keep it focused on a single workflow.\n\
             \n\
             Do NOT include a `name` field — the file's machine name is set at save time.\n\
             \n\
             Example shape:\n\
             ---\n\
             description: A simple test command\n\
             ---\n\
             \n\
             This is a test command to verify command discovery works.\n\
             Just output \"Test command works!\"\n"
        }
        AssetKind::Rule => {
            "You are producing a rule — a short, always-on instruction loaded into \
             every agent session.\n\
             \n\
             Required structure:\n\
             1. An H1 heading naming the rule (e.g. `# Commit Style`). This becomes \
                the rule's display name.\n\
             2. A blank line.\n\
             3. The rule body as markdown. Keep it concise — a rule is read on every \
                request, so favour clarity and brevity over exhaustive detail. Use \
                bullet lists for enumerated guidance. State the *what* and the *why* \
                in one or two short paragraphs at most.\n\
             \n\
             Do NOT include YAML frontmatter — rule storage wraps the markdown in JSON \
             at save time."
        }
        AssetKind::Subagent => {
            "You are producing a sub-agent markdown file. Sub-agents are spawned by \
             the primary agent to perform focused tasks (review code, run tests, \
             investigate logs, etc.).\n\
             \n\
             Required structure:\n\
             1. YAML frontmatter delimited by `---` lines, containing:\n\
                - `name`: machine_name slug (lowercase, hyphenated).\n\
                - `description`: a single-sentence description that begins with a verb \
                  and states when the parent agent should invoke this sub-agent.\n\
                - `tools` (optional): comma-separated list of tools the sub-agent may \
                  use (e.g. `Read, Grep, Glob, Bash`). Omit if the sub-agent should \
                  inherit the parent's full tool set.\n\
                - `model: inherit` (recommended default).\n\
             2. A blank line.\n\
             3. The system prompt body. Write it in second-person, addressed to the \
                sub-agent itself (\"You are a senior code reviewer ...\"). Cover: role, \
                when invoked, what to check, and how to report results.\n\
             \n\
             Example shape:\n\
             ---\n\
             name: code-reviewer\n\
             description: Expert code review specialist. Use immediately after \
             writing or modifying code.\n\
             tools: Read, Grep, Glob, Bash\n\
             model: inherit\n\
             ---\n\
             \n\
             You are a senior code reviewer ensuring high standards of code quality \
             and security.\n\
             \n\
             When invoked:\n\
             1. ...\n"
        }
    };

    format!("{}\n\n{}", shared, kind_specific)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_known_kinds() {
        assert_eq!(AssetKind::parse("skill").unwrap(), AssetKind::Skill);
        assert_eq!(AssetKind::parse("command").unwrap(), AssetKind::Command);
        assert_eq!(AssetKind::parse("rule").unwrap(), AssetKind::Rule);
        assert_eq!(AssetKind::parse("subagent").unwrap(), AssetKind::Subagent);
    }

    #[test]
    fn parse_rejects_unknown() {
        assert!(AssetKind::parse("agent").is_err());
        assert!(AssetKind::parse("Skill").is_err());
        assert!(AssetKind::parse("").is_err());
    }

    #[test]
    fn system_prompts_mention_their_format() {
        assert!(system_prompt(AssetKind::Skill).contains("SKILL.md"));
        assert!(system_prompt(AssetKind::Command).contains("slash-command"));
        assert!(system_prompt(AssetKind::Rule).contains("rule"));
        assert!(system_prompt(AssetKind::Subagent).contains("sub-agent"));
    }

    #[test]
    fn user_prompt_includes_feedback_when_revising() {
        let p = user_prompt("desc", Some("draft"), Some("fix Y"));
        assert!(p.contains("desc"));
        assert!(p.contains("draft"));
        assert!(p.contains("fix Y"));
        assert!(p.contains("previous_draft"));
    }

    #[test]
    fn user_prompt_skips_feedback_block_on_first_attempt() {
        let p = user_prompt("desc", None, None);
        assert!(p.contains("desc"));
        assert!(!p.contains("previous_draft"));
    }
}
