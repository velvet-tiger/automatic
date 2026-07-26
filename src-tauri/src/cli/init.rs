//! `automatic init <template>` — apply a template to a directory.
//!
//! Loads the named template from the library, builds an ephemeral `Project`
//! whose `directory` points at the target, and runs the file-writing half of
//! the sync engine. The projects registry, activity log, and MCP server
//! registry are deliberately untouched — this command exists for users who
//! want template-driven setup without committing to a long-lived project
//! entry.

use serde::Serialize;
use std::path::{Path, PathBuf};

use super::output::{emit, OutputOptions};
use super::CliError;
use crate::core::{self, Project, ProjectTemplate};
use crate::sync;

/// Default synthetic name used when the target directory's basename cannot
/// be sanitised into a valid Automatic project name.
const FALLBACK_PROJECT_NAME: &str = "project";

#[derive(Debug, Serialize)]
struct InitReport {
    template: String,
    directory: String,
    project_name: String,
    files: Vec<String>,
}

pub fn run(
    template: &str,
    directory: Option<&str>,
    name: Option<&str>,
    opts: OutputOptions,
) -> Result<(), CliError> {
    if !core::is_valid_name(template) {
        return Err(CliError::Usage(format!(
            "'{}' is not a valid template name",
            template
        )));
    }

    let directory = resolve_directory(directory)?;
    let directory_str = directory
        .to_str()
        .ok_or_else(|| CliError::Usage("target directory is not valid UTF-8".to_string()))?
        .to_string();

    let project_name = match name {
        Some(supplied) => {
            if !core::is_valid_name(supplied) {
                return Err(CliError::Usage(format!(
                    "'{}' is not a valid project name",
                    supplied
                )));
            }
            supplied.to_string()
        }
        None => sanitise_name_from_path(&directory),
    };

    // Load the template. Errors include the underlying core message so users
    // can distinguish a missing template from a parse failure.
    let template_raw = core::read_template(template).map_err(CliError::from)?;
    let parsed: ProjectTemplate = serde_json::from_str(&template_raw)
        .map_err(|e| CliError::Io(format!("Invalid template '{}': {}", template, e)))?;

    // Fresh in-memory project. Default() picks up the per-agent instruction
    // mode and an empty `file_rules`, which matches what the GUI does for a
    // brand-new project, so the sync engine writes the same files.
    let mut project = Project {
        name: project_name.clone(),
        directory: directory_str.clone(),
        ..Default::default()
    };

    // Union the template into the project. This also writes any inline
    // `project_files` (e.g. CLAUDE.md, AGENTS.md) into `directory` directly.
    core::merge_templates_into_project(&mut project, &[parsed]).map_err(CliError::from)?;

    // Write agent configs, skills, hooks, instruction files. The registry
    // save inside `sync_project_without_autodetect` is the only difference
    // versus this call — this is the path that leaves no trace of a project.
    let files = sync::sync_to_directory(&mut project).map_err(CliError::from)?;

    let report = InitReport {
        template: template.to_string(),
        directory: directory_str,
        project_name,
        files,
    };

    let human = || {
        let count = report.files.len();
        format!(
            "Applied template '{}' to {} — {} file{} written.",
            report.template,
            report.directory,
            count,
            if count == 1 { "" } else { "s" }
        )
    };
    emit(opts, &report, human).map_err(CliError::Io)
}

fn resolve_directory(directory: Option<&str>) -> Result<PathBuf, CliError> {
    let path = match directory {
        Some(d) => PathBuf::from(d),
        None => std::env::current_dir()
            .map_err(|e| CliError::Io(format!("Failed to read current directory: {}", e)))?,
    };

    if !path.exists() {
        return Err(CliError::Io(format!(
            "directory does not exist: {}",
            path.display()
        )));
    }
    if !path.is_dir() {
        return Err(CliError::Usage(format!(
            "{} is not a directory",
            path.display()
        )));
    }

    // The sync engine reads the directory back from `project.directory`
    // with `PathBuf::from`, so a relative path would resolve against the
    // *current* working directory at sync time. Canonicalise here so that
    // the value sync sees is stable regardless of any cwd changes.
    std::fs::canonicalize(&path)
        .map_err(|e| CliError::Io(format!("Failed to resolve {}: {}", path.display(), e)))
}

/// Derive a synthetic project name from the target directory's basename.
///
/// `is_valid_name` rejects empty strings, `.`, `..`, and anything containing
/// path separators. We replace each invalid character with `-` and fall back
/// to a literal `"project"` so the sync engine always has something to
/// hash into filenames.
fn sanitise_name_from_path(path: &Path) -> String {
    let raw = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(FALLBACK_PROJECT_NAME);
    sanitise_name(raw)
}

fn sanitise_name(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .map(|c| match c {
            '/' | '\\' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();

    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        FALLBACK_PROJECT_NAME.to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitise_strips_separators() {
        assert_eq!(sanitise_name("foo/bar"), "foo-bar");
        assert_eq!(sanitise_name("foo\\bar"), "foo-bar");
    }

    #[test]
    fn sanitise_falls_back_when_unusable() {
        assert_eq!(sanitise_name(""), FALLBACK_PROJECT_NAME);
        assert_eq!(sanitise_name("."), FALLBACK_PROJECT_NAME);
        assert_eq!(sanitise_name(".."), FALLBACK_PROJECT_NAME);
    }

    #[test]
    fn sanitise_keeps_normal_names() {
        assert_eq!(sanitise_name("my-app"), "my-app");
        assert_eq!(sanitise_name("Acme Co"), "Acme Co");
    }

    #[test]
    fn sanitised_names_satisfy_is_valid_name() {
        for input in ["foo/bar", "", "..", "/etc/passwd", "ok"] {
            let cleaned = sanitise_name(input);
            assert!(
                core::is_valid_name(&cleaned),
                "sanitise_name produced invalid name '{}' from '{}'",
                cleaned,
                input
            );
        }
    }
}
