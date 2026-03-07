//! Language module system for project snapshot generation.
//!
//! Each language is described by a declarative TOML file (`*.mod`) bundled
//! into the binary at compile time.  The engine loads every bundled module,
//! evaluates each against a project root directory, and returns the set that
//! match.  Callers use the matched modules to drive snapshot content (which
//! config files to read, which entry-point source files to include, which
//! directories to skip in the tree walk).
//!
//! # Detection rules
//!
//! A module matches a project if **any** `[[detect]]` block passes.
//! A `[[detect]]` block passes when:
//!   - All paths listed in `files` exist at the project root, AND
//!   - (if `contains` is set) any string in `contains` appears in the first
//!     listed file's content.
//!
//! A module may also declare `glob_extensions` (e.g. `[".csproj"]`) — it
//! matches if any file at the project root has one of those extensions.
//!
//! # Adding a new language
//!
//! Create `src-tauri/languages/<id>.mod` following the TOML schema below and
//! add it to the `BUNDLED_MODULES` constant at the bottom of this file.
//! No Rust code changes are required.

use serde::Deserialize;
use std::path::Path;

// ── Data model ────────────────────────────────────────────────────────────────

/// One detection probe within a language module.
/// The probe passes when all `files` exist at the project root, and (if set)
/// any entry in `contains` is found in the content of the first listed file.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct DetectProbe {
    /// File paths (relative to project root) that must all exist.
    #[serde(default)]
    pub files: Vec<String>,

    /// If non-empty, at least one of these strings must appear in the content
    /// of `files[0]`.
    #[serde(default)]
    pub contains: Vec<String>,
}

/// A language / framework module loaded from a `.mod` TOML file.
#[derive(Debug, Clone, Deserialize)]
pub struct LanguageModule {
    /// Unique identifier, e.g. `"rust"`, `"react"`.
    pub id: String,

    /// Human-readable name, e.g. `"Rust"`, `"React"`.
    pub name: String,

    /// Detection probes.  The module matches if **any** probe passes.
    /// May be absent if `glob_extensions` is used instead.
    #[serde(default)]
    pub detect: Vec<DetectProbe>,

    /// Root-level file extensions that trigger a match (e.g. `[".csproj"]`).
    /// Evaluated in addition to `detect` probes — module matches on any hit.
    #[serde(default)]
    pub glob_extensions: Vec<String>,

    /// Config / manifest files to read in full when this module matches.
    #[serde(default)]
    pub config_files: Vec<String>,

    /// Candidate entry-point source file paths (relative to project root).
    /// Only paths that exist on disk are included in the snapshot.
    #[serde(default)]
    pub entry_points: Vec<String>,

    /// Additional directory names to skip during the tree walk.
    /// These are merged with the global ignore list in the snapshot builder.
    #[serde(default)]
    pub ignore_dirs: Vec<String>,
}

impl LanguageModule {
    /// Parse a module from a TOML string.  Returns an error string if the
    /// TOML is malformed or required fields are missing.
    pub fn from_toml(src: &str) -> Result<Self, String> {
        toml::from_str(src).map_err(|e| format!("Failed to parse language module: {}", e))
    }

    /// Test whether this module matches the given project root directory.
    ///
    /// Returns `true` if any detection probe passes **or** any glob extension
    /// is present at the root.
    pub fn matches(&self, root: &Path) -> bool {
        // Check [[detect]] probes
        for probe in &self.detect {
            if self.probe_matches(probe, root) {
                return true;
            }
        }

        // Check glob_extensions — any root-level file with a matching extension
        if !self.glob_extensions.is_empty() {
            if let Ok(entries) = std::fs::read_dir(root) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    for ext in &self.glob_extensions {
                        if name.ends_with(ext.as_str()) {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    fn probe_matches(&self, probe: &DetectProbe, root: &Path) -> bool {
        if probe.files.is_empty() {
            return false;
        }

        // All listed files must exist
        if !probe.files.iter().all(|f| root.join(f).exists()) {
            return false;
        }

        // If `contains` is set, at least one string must appear in the first file
        if !probe.contains.is_empty() {
            let first_file = &probe.files[0];
            let content = match std::fs::read_to_string(root.join(first_file)) {
                Ok(c) => c,
                Err(_) => return false,
            };
            if !probe.contains.iter().any(|s| content.contains(s.as_str())) {
                return false;
            }
        }

        true
    }
}

// ── Bundled modules ───────────────────────────────────────────────────────────
//
// Each entry is (id, toml_source).  The id is used as a stable key; it should
// match the `id` field inside the TOML.  Add new entries here when adding a
// new language module file.

const BUNDLED_MODULES: &[(&str, &str)] = &[
    ("rust", include_str!("../languages/rust.mod")),
    ("react", include_str!("../languages/react.mod")),
    ("nextjs", include_str!("../languages/nextjs.mod")),
    ("node", include_str!("../languages/node.mod")),
    ("vue", include_str!("../languages/vue.mod")),
    ("svelte", include_str!("../languages/svelte.mod")),
    ("python", include_str!("../languages/python.mod")),
    ("go", include_str!("../languages/go.mod")),
    ("ruby", include_str!("../languages/ruby.mod")),
    ("java", include_str!("../languages/java.mod")),
    ("csharp", include_str!("../languages/csharp.mod")),
    ("php", include_str!("../languages/php.mod")),
    ("cpp", include_str!("../languages/cpp.mod")),
];

// ── Public API ────────────────────────────────────────────────────────────────

/// Load all bundled language modules.
///
/// Parse errors are logged to stderr and skipped — a single malformed module
/// never prevents the others from loading.
pub fn load_bundled() -> Vec<LanguageModule> {
    BUNDLED_MODULES
        .iter()
        .filter_map(|(id, src)| {
            LanguageModule::from_toml(src)
                .map_err(|e| eprintln!("[languages] failed to load module '{}': {}", id, e))
                .ok()
        })
        .collect()
}

/// Load bundled modules and any user-supplied override modules from
/// `~/.automatic/languages/*.mod`.  User modules with the same `id` as a
/// bundled module replace the bundled version; new ids are appended.
pub fn load_all() -> Vec<LanguageModule> {
    let mut modules = load_bundled();

    // Attempt to load user overrides — failure is silent (directory may not exist)
    if let Some(user_dir) = dirs::home_dir().map(|h| h.join(".automatic").join("languages")) {
        if user_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&user_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("mod") {
                        continue;
                    }
                    match std::fs::read_to_string(&path) {
                        Ok(src) => match LanguageModule::from_toml(&src) {
                            Ok(user_mod) => {
                                // Replace existing bundled module with same id, or append
                                if let Some(pos) = modules.iter().position(|m| m.id == user_mod.id)
                                {
                                    modules[pos] = user_mod;
                                } else {
                                    modules.push(user_mod);
                                }
                            }
                            Err(e) => {
                                eprintln!("[languages] failed to parse '{}': {}", path.display(), e)
                            }
                        },
                        Err(e) => {
                            eprintln!("[languages] failed to read '{}': {}", path.display(), e)
                        }
                    }
                }
            }
        }
    }

    modules
}

/// Evaluate all modules against `root` and return those that match.
pub fn detect(root: &Path) -> Vec<LanguageModule> {
    load_all().into_iter().filter(|m| m.matches(root)).collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn bundled_modules_parse_cleanly() {
        let modules = load_bundled();
        // Every bundled module must parse without error
        assert_eq!(
            modules.len(),
            BUNDLED_MODULES.len(),
            "some bundled modules failed to parse"
        );
        for m in &modules {
            assert!(!m.id.is_empty(), "module has empty id");
            assert!(!m.name.is_empty(), "module '{}' has empty name", m.id);
        }
    }

    #[test]
    fn rust_module_matches_cargo_toml() {
        let dir = tmp();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"x\"").unwrap();
        let matched = detect(dir.path());
        assert!(matched.iter().any(|m| m.id == "rust"));
    }

    #[test]
    fn react_module_matches_package_json_with_react_dep() {
        let dir = tmp();
        fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies":{"react":"^18"}}"#,
        )
        .unwrap();
        let matched = detect(dir.path());
        assert!(matched.iter().any(|m| m.id == "react"));
    }

    #[test]
    fn node_module_matches_package_json_without_framework() {
        let dir = tmp();
        fs::write(dir.path().join("package.json"), r#"{"name":"my-cli"}"#).unwrap();
        let matched = detect(dir.path());
        assert!(matched.iter().any(|m| m.id == "node"));
    }

    #[test]
    fn no_match_on_empty_directory() {
        let dir = tmp();
        let matched = detect(dir.path());
        assert!(matched.is_empty());
    }

    #[test]
    fn multi_match_tauri_project() {
        let dir = tmp();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"x\"").unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies":{"react":"^18"}}"#,
        )
        .unwrap();
        let matched = detect(dir.path());
        let ids: Vec<&str> = matched.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"rust"), "expected rust in {:?}", ids);
        assert!(ids.contains(&"react"), "expected react in {:?}", ids);
    }

    #[test]
    fn python_matches_requirements_txt() {
        let dir = tmp();
        fs::write(dir.path().join("requirements.txt"), "flask\n").unwrap();
        let matched = detect(dir.path());
        assert!(matched.iter().any(|m| m.id == "python"));
    }
}
