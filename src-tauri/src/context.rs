use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProjectContext {
    #[serde(default)]
    pub commands: HashMap<String, String>,
    #[serde(default)]
    pub entry_points: HashMap<String, String>,
    #[serde(default)]
    pub concepts: HashMap<String, Concept>,
    #[serde(default)]
    pub conventions: HashMap<String, String>,
    #[serde(default)]
    pub gotchas: HashMap<String, String>,
    #[serde(default)]
    pub docs: HashMap<String, DocEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Concept {
    pub files: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DocEntry {
    pub path: String,
    pub summary: String,
}

pub fn get_project_context(directory: &str) -> Result<ProjectContext, String> {
    if directory.is_empty() {
        return Err("Project has no directory configured".into());
    }

    let dir_path = PathBuf::from(directory);
    let context_path = dir_path.join(".automatic").join("context.toml");

    if !context_path.exists() {
        return Ok(ProjectContext::default());
    }

    let content = fs::read_to_string(&context_path).map_err(|e| e.to_string())?;
    let context: ProjectContext = toml::from_str(&content).map_err(|e| format!("Failed to parse context.toml: {}", e))?;

    Ok(context)
}

// ============================================================================
// Formatters for context tools
// ============================================================================

pub fn get_commands(context: &ProjectContext, project_name: &str, command_type: Option<&str>) -> Result<String, String> {
    if context.commands.is_empty() {
        return Ok(format!("No commands defined for project '{}'. Define them in .automatic/context.toml under [commands].", project_name));
    }

    match command_type {
        Some(cmd_type) => context
            .commands
            .get(cmd_type)
            .map(|cmd| format!("{}: {}", cmd_type, cmd))
            .ok_or_else(|| {
                format!(
                    "Command '{}' not found for project '{}'",
                    cmd_type, project_name
                )
            }),
        None => {
            let mut output = format!("# Commands for '{}'\n\n", project_name);
            for (name, cmd) in &context.commands {
                output.push_str(&format!("- **{}**: `{}`\n", name, cmd));
            }
            Ok(output)
        }
    }
}

pub fn get_architecture(context: &ProjectContext, project_name: &str, concept_name: &str, path: &std::path::Path) -> Result<String, String> {
    if context.concepts.is_empty() {
        return Ok(format!("No concepts defined for project '{}'. Define them in .automatic/context.toml under [concepts].", project_name));
    }

    // Try exact match first
    if let Some(concept) = context.concepts.get(concept_name) {
        return Ok(format_concept(path, concept_name, concept));
    }

    // Try case-insensitive match
    let concept_lower = concept_name.to_lowercase();
    for (name, concept) in &context.concepts {
        if name.to_lowercase() == concept_lower {
            return Ok(format_concept(path, name, concept));
        }
    }

    // Try partial match
    for (name, concept) in &context.concepts {
        if name.to_lowercase().contains(&concept_lower)
            || concept.summary.to_lowercase().contains(&concept_lower)
        {
            return Ok(format_concept(path, name, concept));
        }
    }

    // List available concepts
    let available: Vec<&str> = context.concepts.keys().map(|s| s.as_str()).collect();
    Err(format!(
        "Concept '{}' not found. Available concepts: {}",
        concept_name,
        available.join(", ")
    ))
}

fn format_concept(root: &std::path::Path, name: &str, concept: &Concept) -> String {
    let mut output = format!("# Concept: {}\n\n", name);
    output.push_str(&format!("**Summary:** {}\n\n", concept.summary));
    output.push_str("## Relevant Files\n");

    if concept.files.is_empty() {
        output.push_str("No specific files listed.\n");
    } else {
        for file in &concept.files {
            output.push_str(&format!("- {}/{}\n", root.display(), file));
        }
    }

    output
}

pub fn get_conventions(context: &ProjectContext, project_name: &str, category: Option<&str>) -> Result<String, String> {
    let has_conventions = !context.conventions.is_empty();
    let has_gotchas = !context.gotchas.is_empty();

    if !has_conventions && !has_gotchas {
        return Ok(format!(
            "No conventions found for '{}'. Create .automatic/context.toml to add project-specific conventions and gotchas.",
            project_name
        ));
    }

    let mut output = String::new();

    match category {
        Some("conventions") => {
            if !has_conventions {
                return Ok("No conventions defined.".to_string());
            }
            output.push_str(&format!("# Conventions for '{}'\n\n", project_name));
            for (name, desc) in &context.conventions {
                output.push_str(&format!("## {}\n{}\n\n", name, desc));
            }
        }
        Some("gotchas") => {
            if !has_gotchas {
                return Ok("No gotchas defined.".to_string());
            }
            output.push_str(&format!("# Gotchas for '{}'\n\n", project_name));
            for (name, desc) in &context.gotchas {
                output.push_str(&format!("## {}\n{}\n\n", name, desc));
            }
        }
        None => {
            if has_conventions {
                output.push_str(&format!("# Conventions for '{}'\n\n", project_name));
                for (name, desc) in &context.conventions {
                    output.push_str(&format!("## {}\n{}\n\n", name, desc));
                }
            }
            if has_gotchas {
                output.push_str(&format!("# Gotchas for '{}'\n\n", project_name));
                for (name, desc) in &context.gotchas {
                    output.push_str(&format!("## {}\n{}\n\n", name, desc));
                }
            }
        }
        Some(c) => {
            return Err(format!(
                "Unknown category '{}'. Use 'conventions' or 'gotchas'.",
                c
            ))
        }
    }

    Ok(output)
}

pub fn get_docs(context: &ProjectContext, project_name: &str, topic: Option<&str>, path: &std::path::Path) -> Result<String, String> {
    if context.docs.is_empty() {
        return Ok(format!(
            "No documentation index found for '{}'. Define them in .automatic/context.toml under [docs].",
            project_name
        ));
    }

    match topic {
        Some(t) => {
            // Return path to specific doc
            let doc = context.docs.get(t).ok_or_else(|| {
                let available: Vec<&str> = context.docs.keys().map(|s| s.as_str()).collect();
                format!("Doc '{}' not found. Available: {}", t, available.join(", "))
            })?;
            let full_path = path.join(&doc.path);
            Ok(format!(
                "## {}\n**Summary:** {}\n**Path:** {}",
                t,
                doc.summary,
                full_path.display()
            ))
        }
        None => {
            // List all docs with summaries
            let mut output = format!("# Documentation for '{}'\n\n", project_name);
            for (name, doc) in &context.docs {
                output.push_str(&format!("- **{}**: {}\n", name, doc.summary));
            }
            output.push_str("\nUse get_docs(project, topic) to get the path to a specific doc.");
            Ok(output)
        }
    }
}
