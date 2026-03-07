use crate::languages;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// ── Project snapshot for AI context generation ────────────────────────────────

/// Directory / file names that are never worth including in a project snapshot.
/// These are build artefacts, caches, or VCS internals that add noise and size.
const SNAPSHOT_IGNORE_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    "vendor",
    ".cargo",
    "coverage",
    ".nyc_output",
    "out",
    ".turbo",
    ".venv",
    "venv",
    "env",
    ".tox",
    ".eggs",
    "*.egg-info",
];

/// Config / manifest files to read in full (checked in order; first match wins
/// for each filename pattern).
const PRIORITY_CONFIG_FILES: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "setup.py",
    "go.mod",
    "Makefile",
    "justfile",
    "Taskfile.yml",
    "Taskfile.yaml",
    "composer.json",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "CMakeLists.txt",
    "Dockerfile",
    "docker-compose.yml",
    "docker-compose.yaml",
    ".agents.md",
    "AGENTS.md",
    "CLAUDE.md",
    "claude.md",
];

/// Documentation files to include (first 80 lines only).
const PRIORITY_DOC_FILES: &[&str] = &[
    "README.md",
    "README.rst",
    "README.txt",
    "README",
    "CONTRIBUTING.md",
    "ARCHITECTURE.md",
    "DESIGN.md",
];

/// Build a compact text snapshot of a project directory for use as AI context.
///
/// The snapshot contains:
/// 1. A directory tree (up to 3 levels, ignoring build artefacts and any
///    language-module-declared ignore dirs)
/// 2. Detected language(s) and their config files
/// 3. Entry-point source files identified by the matched language modules
///    (first 150 lines each)
/// 4. First 80 lines of README / doc files found at the root
///
/// The output is plain text formatted for readability in an LLM prompt.
/// It is deliberately bounded so it fits comfortably within a single request.
pub fn build_project_snapshot(directory: &str) -> Result<String, String> {
    let root = Path::new(directory);
    if !root.is_dir() {
        return Err(format!("'{}' is not a directory", directory));
    }

    // ── Detect languages ──────────────────────────────────────────────────────
    let matched_modules = languages::detect(root);

    // Aggregate ignore_dirs from all matched modules (merged with global list)
    let mut extra_ignore: Vec<String> = matched_modules
        .iter()
        .flat_map(|m| m.ignore_dirs.clone())
        .collect();
    extra_ignore.sort();
    extra_ignore.dedup();

    let mut out = String::new();

    // ── 1. Directory tree ─────────────────────────────────────────────────────
    out.push_str("# Directory tree (3 levels)\n\n```\n");
    append_tree(&mut out, root, root, 0, 3, &extra_ignore);
    out.push_str("```\n\n");

    // ── 2. Detected languages + config files ──────────────────────────────────
    if !matched_modules.is_empty() {
        let names: Vec<&str> = matched_modules.iter().map(|m| m.name.as_str()).collect();
        out.push_str(&format!(
            "# Detected language(s)\n\n{}\n\n",
            names.join(", ")
        ));

        // Collect config files from all matched modules (plus the global list),
        // deduplicate, then read each one that exists.
        let mut config_files: Vec<String> = PRIORITY_CONFIG_FILES
            .iter()
            .map(|s| s.to_string())
            .collect();
        for m in &matched_modules {
            for cf in &m.config_files {
                if !config_files.iter().any(|x| x == cf) {
                    config_files.push(cf.clone());
                }
            }
        }

        out.push_str("# Key configuration files\n\n");
        let mut found_any = false;
        for name in &config_files {
            let path = root.join(name);
            if path.is_file() {
                if let Ok(content) = fs::read_to_string(&path) {
                    out.push_str(&format!("## {}\n\n```\n{}\n```\n\n", name, content.trim()));
                    found_any = true;
                }
            }
        }
        if !found_any {
            out.push_str("(no standard config files found at root)\n\n");
        }
    } else {
        // No language detected — still try the global config file list
        out.push_str("# Key configuration files\n\n");
        let mut found_any = false;
        for name in PRIORITY_CONFIG_FILES {
            let path = root.join(name);
            if path.is_file() {
                if let Ok(content) = fs::read_to_string(&path) {
                    out.push_str(&format!("## {}\n\n```\n{}\n```\n\n", name, content.trim()));
                    found_any = true;
                }
            }
        }
        if !found_any {
            out.push_str("(no standard config files found at root)\n\n");
        }
    }

    // ── 3. Entry-point source files ───────────────────────────────────────────
    // Collect candidates from all matched modules, deduplicate, filter to those
    // that exist on disk, then read the first 150 lines of each.
    let mut seen_eps = std::collections::HashSet::new();
    let entry_point_paths: Vec<PathBuf> = matched_modules
        .iter()
        .flat_map(|m| m.entry_points.iter())
        .filter(|p| seen_eps.insert(p.as_str()))
        .map(|p| root.join(p))
        .filter(|p| p.is_file())
        .collect();

    if !entry_point_paths.is_empty() {
        out.push_str("# Entry-point source files\n\n");
        for path in &entry_point_paths {
            let rel = path.strip_prefix(root).unwrap_or(path);
            if let Ok(content) = fs::read_to_string(path) {
                let lines: Vec<&str> = content.lines().collect();
                let total = lines.len();
                let preview = lines[..total.min(150)].join("\n");
                out.push_str(&format!(
                    "## {} ({} lines total)\n\n```\n{}\n```\n",
                    rel.display(),
                    total,
                    preview
                ));
                if total > 150 {
                    out.push_str("[... file continues ...]\n");
                }
                out.push('\n');
            }
        }
    }

    // ── 4. Documentation / README ─────────────────────────────────────────────
    out.push_str("# Documentation\n\n");
    let mut found_any_doc = false;
    for name in PRIORITY_DOC_FILES {
        let path = root.join(name);
        if path.is_file() {
            if let Ok(content) = fs::read_to_string(&path) {
                let preview: String = content.lines().take(80).collect::<Vec<_>>().join("\n");
                let truncated = content.lines().count() > 80;
                out.push_str(&format!("## {} (first 80 lines)\n\n{}\n", name, preview));
                if truncated {
                    out.push_str("\n[... file continues ...]\n");
                }
                out.push('\n');
                found_any_doc = true;
            }
        }
    }
    if !found_any_doc {
        out.push_str("(no standard documentation files found at root)\n\n");
    }

    Ok(out)
}

/// Recursively append a tree view to `out`, ignoring noisy directories.
///
/// `extra_ignore` is a list of additional directory names to skip, contributed
/// by the matched language modules (e.g. `["target", ".cargo"]` for Rust).
fn append_tree(
    out: &mut String,
    root: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    extra_ignore: &[String],
) {
    if depth >= max_depth {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let indent = "  ".repeat(depth);

    let mut names: Vec<(String, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
        .filter(|(name, _)| {
            // Skip dot files at depth 0 except well-known config ones
            if depth == 0 && name.starts_with('.') {
                return matches!(name.as_str(), ".agents.md" | ".automatic" | ".env.example");
            }
            // Skip global ignore list
            if SNAPSHOT_IGNORE_DIRS
                .iter()
                .any(|ig| name == *ig || name.ends_with(".egg-info"))
            {
                return false;
            }
            // Skip language-module-contributed ignore dirs
            if extra_ignore.iter().any(|ig| name == ig) {
                return false;
            }
            true
        })
        .collect();

    names.sort_by(|(a, _), (b, _)| a.cmp(b));

    for (_name, path) in names {
        if path.is_dir() {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            out.push_str(&format!("{}{}/\n", indent, rel.display()));
            append_tree(out, root, &path, depth + 1, max_depth, extra_ignore);
        } else {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            out.push_str(&format!("{}{}\n", indent, rel.display()));
        }
    }
}

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
    let context_path = dir_path.join(".automatic").join("context.json");

    if !context_path.exists() {
        return Ok(ProjectContext::default());
    }

    let content = fs::read_to_string(&context_path).map_err(|e| e.to_string())?;
    let context: ProjectContext = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse context.json: {}", e))?;

    Ok(context)
}

// ============================================================================
// Formatters for context tools
// ============================================================================

pub fn get_commands(
    context: &ProjectContext,
    project_name: &str,
    command_type: Option<&str>,
) -> Result<String, String> {
    if context.commands.is_empty() {
        return Ok(format!("No commands defined for project '{}'. Define them in .automatic/context.json under \"commands\".", project_name));
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

pub fn get_architecture(
    context: &ProjectContext,
    project_name: &str,
    concept_name: &str,
    path: &std::path::Path,
) -> Result<String, String> {
    if context.concepts.is_empty() {
        return Ok(format!("No concepts defined for project '{}'. Define them in .automatic/context.json under \"concepts\".", project_name));
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

pub fn get_conventions(
    context: &ProjectContext,
    project_name: &str,
    category: Option<&str>,
) -> Result<String, String> {
    let has_conventions = !context.conventions.is_empty();
    let has_gotchas = !context.gotchas.is_empty();

    if !has_conventions && !has_gotchas {
        return Ok(format!(
            "No conventions found for '{}'. Create .automatic/context.json to add project-specific conventions and gotchas.",
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

pub fn get_docs(
    context: &ProjectContext,
    project_name: &str,
    topic: Option<&str>,
    path: &std::path::Path,
) -> Result<String, String> {
    if context.docs.is_empty() {
        return Ok(format!(
            "No documentation index found for '{}'. Define them in .automatic/context.json under \"docs\".",
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
