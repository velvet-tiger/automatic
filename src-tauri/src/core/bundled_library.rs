//! Access to the `automatic-library` content bundle.
//!
//! The library ships as a `.zip` archive packed at build time by
//! `src-tauri/build.rs` from the `automatic-library/` git submodule. This
//! module owns two compile-time artifacts:
//!
//! - `${OUT_DIR}/library.zip` — the packed archive.
//! - `${OUT_DIR}/library_version.txt` — the pinned semver from
//!   `automatic-library/VERSION`.
//!
//! Consumers ask for typed views of the manifest (skills, rules,
//! instructions, subagents, retired rules) and for raw file bytes by
//! archive-relative path. The zip is opened once and every file is
//! extracted into an in-memory cache on first request.

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::OnceLock;

use serde::Deserialize;

/// Semver of the pinned library, e.g. "0.1.0".
pub fn version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION
        .get_or_init(|| {
            include_str!(concat!(env!("OUT_DIR"), "/library_version.txt"))
                .trim()
                .to_string()
        })
        .as_str()
}

/// Raw archive bytes. Callers should prefer `read_file` over touching this
/// directly.
const LIBRARY_ZIP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/library.zip"));

/// A single asset entry from `manifest.json`. Fields are optional because the
/// manifest is heterogeneous across kinds: skills carry `root` and `files`;
/// rules/subagents carry `pack`, `path`, and `sha256`; instructions and hooks
/// carry only `path` and `sha256`.
#[derive(Debug, Clone, Deserialize)]
pub struct RawEntry {
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub pack: Option<String>,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub files: Option<Vec<RawFileEntry>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawFileEntry {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Deserialize)]
pub struct RawManifest {
    pub library_version: String,
    #[allow(dead_code)]
    pub manifest_schema: u32,
    pub assets: Vec<RawEntry>,
}

fn manifest() -> &'static RawManifest {
    static MANIFEST: OnceLock<RawManifest> = OnceLock::new();
    MANIFEST.get_or_init(|| {
        let raw = read_file("manifest.json").expect("library archive missing manifest.json");
        serde_json::from_slice(&raw).expect("library manifest.json failed to parse")
    })
}

fn retired_data() -> &'static Retired {
    static RETIRED: OnceLock<Retired> = OnceLock::new();
    RETIRED.get_or_init(|| {
        let raw = read_file("retired.json").expect("library archive missing retired.json");
        serde_json::from_slice(&raw).expect("library retired.json failed to parse")
    })
}

#[derive(Debug, Deserialize)]
struct Retired {
    retired: Vec<RetiredEntry>,
}

#[derive(Debug, Deserialize)]
struct RetiredEntry {
    kind: String,
    #[allow(dead_code)]
    pack: Option<String>,
    id: String,
    #[allow(dead_code)]
    retired_in: Option<String>,
    #[allow(dead_code)]
    reason: Option<String>,
}

/// Return the bytes at `archive_path` (a POSIX-style path relative to the
/// library root, e.g. `rules/automatic/code.md`). Fails if the entry is
/// missing.
pub fn read_file(archive_path: &str) -> Result<Vec<u8>, String> {
    static CACHE: OnceLock<HashMap<String, Vec<u8>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| {
        let mut map = HashMap::new();
        let reader = Cursor::new(LIBRARY_ZIP);
        let mut zip =
            zip::ZipArchive::new(reader).expect("bundled library archive failed to open");
        for i in 0..zip.len() {
            let mut file = zip
                .by_index(i)
                .expect("bundled library archive entry unreadable");
            if !file.is_file() {
                continue;
            }
            let name = file.name().to_string();
            let mut buf = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut buf)
                .expect("bundled library archive entry read failed");
            map.insert(name, buf);
        }
        map
    });
    cache
        .get(archive_path)
        .cloned()
        .ok_or_else(|| format!("bundled library archive missing entry: {}", archive_path))
}

/// Return `read_file` bytes decoded as UTF-8.
pub fn read_file_string(archive_path: &str) -> Result<String, String> {
    let bytes = read_file(archive_path)?;
    String::from_utf8(bytes).map_err(|e| format!("{} is not UTF-8: {}", archive_path, e))
}

// -------- typed views of the manifest ---------------------------------------

/// One skill in the library. A skill is a directory of one or more files
/// rooted at `root` (POSIX-style, e.g. `skills/automatic-debugging`).
#[derive(Debug, Clone)]
pub struct SkillEntry {
    pub id: String,
    pub root: String,
    pub files: Vec<RawFileEntry>,
}

/// One rule in the library. `id` is the filename stem (e.g. `code`); the
/// caller composes a compound machine name from `pack` and `id` if required.
#[derive(Debug, Clone)]
pub struct RuleEntry {
    pub id: String,
    pub pack: String,
    pub path: String,
}

/// One instruction template. `id` is the filename stem (spaces allowed).
#[derive(Debug, Clone)]
pub struct InstructionEntry {
    pub id: String,
    pub path: String,
}

/// One subagent. `id` is the filename stem; `pack` is the parent directory.
#[derive(Debug, Clone)]
pub struct SubagentEntry {
    pub id: String,
    pub pack: String,
    pub path: String,
}

pub fn skills() -> Vec<SkillEntry> {
    manifest()
        .assets
        .iter()
        .filter(|e| e.kind == "skill")
        .map(|e| SkillEntry {
            id: e.id.clone(),
            root: e
                .root
                .clone()
                .unwrap_or_else(|| panic!("skill {} missing root", e.id)),
            files: e.files.clone().unwrap_or_default(),
        })
        .collect()
}

pub fn rules() -> Vec<RuleEntry> {
    manifest()
        .assets
        .iter()
        .filter(|e| e.kind == "rule")
        .map(|e| RuleEntry {
            id: e.id.clone(),
            pack: e
                .pack
                .clone()
                .unwrap_or_else(|| panic!("rule {} missing pack", e.id)),
            path: e
                .path
                .clone()
                .unwrap_or_else(|| panic!("rule {} missing path", e.id)),
        })
        .collect()
}

pub fn instructions() -> Vec<InstructionEntry> {
    manifest()
        .assets
        .iter()
        .filter(|e| e.kind == "instruction")
        .map(|e| InstructionEntry {
            id: e.id.clone(),
            path: e
                .path
                .clone()
                .unwrap_or_else(|| panic!("instruction {} missing path", e.id)),
        })
        .collect()
}

pub fn subagents() -> Vec<SubagentEntry> {
    manifest()
        .assets
        .iter()
        .filter(|e| e.kind == "subagent")
        .map(|e| SubagentEntry {
            id: e.id.clone(),
            pack: e
                .pack
                .clone()
                .unwrap_or_else(|| panic!("subagent {} missing pack", e.id)),
            path: e
                .path
                .clone()
                .unwrap_or_else(|| panic!("subagent {} missing path", e.id)),
        })
        .collect()
}

/// Machine names of rules retired in the library. Returned in the shape the
/// old REMOVED_DEFAULT_RULES constant used: compound name = `{pack}-{id}`
/// when a pack is present.
pub fn retired_rules() -> Vec<String> {
    retired_data()
        .retired
        .iter()
        .filter(|e| e.kind == "rule")
        .map(|e| match &e.pack {
            Some(p) => format!("{}-{}", p, e.id),
            None => e.id.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_non_empty_semver() {
        let v = version();
        assert!(!v.is_empty(), "library version is empty");
        assert_eq!(v.split('.').count(), 3, "not semver: {}", v);
    }

    #[test]
    fn manifest_has_assets() {
        let m = manifest();
        assert!(!m.assets.is_empty(), "manifest has no assets");
    }

    #[test]
    fn skills_present() {
        assert!(!skills().is_empty(), "no skills in bundled library");
    }

    #[test]
    fn rules_present() {
        assert!(!rules().is_empty(), "no rules in bundled library");
    }

    #[test]
    fn instructions_present() {
        assert!(
            !instructions().is_empty(),
            "no instructions in bundled library"
        );
    }

    #[test]
    fn subagents_present() {
        assert!(!subagents().is_empty(), "no subagents in bundled library");
    }

    #[test]
    fn manifest_json_extracts() {
        let raw = read_file("manifest.json").expect("manifest.json readable");
        assert!(!raw.is_empty());
    }

    #[test]
    fn retired_rules_are_prefixed_and_populated() {
        let retired = retired_rules();
        assert!(!retired.is_empty(), "retired.json has no rule entries");
        assert!(
            retired.iter().any(|n| n == "automatic-commands"),
            "expected automatic-commands in retired list: {:?}",
            retired
        );
        assert!(
            retired.iter().any(|n| n == "automatic-code-style"),
            "expected automatic-code-style in retired list: {:?}",
            retired
        );
    }

    #[test]
    fn rule_files_readable() {
        for rule in rules() {
            let bytes =
                read_file(&rule.path).unwrap_or_else(|e| panic!("rule {} unreadable: {}", rule.id, e));
            assert!(!bytes.is_empty(), "rule {} is empty", rule.id);
        }
    }

    /// End-to-end parity: for every file in every kind, the bytes extracted
    /// from the compiled-in archive must match the bytes on disk in the
    /// `automatic-library/` submodule (the source of truth for `build.rs`).
    /// If this ever fails, the packing pipeline dropped or altered content.
    #[test]
    fn archive_matches_submodule_source() {
        use std::path::Path;

        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace parent")
            .join("automatic-library");

        let mut checked = 0usize;
        for asset in manifest().assets.iter() {
            let mut paths: Vec<String> = Vec::new();
            if let Some(p) = &asset.path {
                paths.push(p.clone());
            }
            if let Some(files) = &asset.files {
                for f in files {
                    paths.push(f.path.clone());
                }
            }
            for path in paths {
                let on_disk = root.join(&path);
                let disk_bytes = std::fs::read(&on_disk)
                    .unwrap_or_else(|e| panic!("submodule missing {}: {}", path, e));
                let archive_bytes = read_file(&path)
                    .unwrap_or_else(|e| panic!("archive missing {}: {}", path, e));
                assert_eq!(
                    disk_bytes, archive_bytes,
                    "content mismatch for {}: {} bytes on disk vs {} in archive",
                    path,
                    disk_bytes.len(),
                    archive_bytes.len()
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "no files checked");
    }
}
