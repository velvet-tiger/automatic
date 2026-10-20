//! Import of Agent Plugin packages (Agent Plugins 1.0.0,
//! <https://agent-plugins.org/specification>) into Automatic's library.
//!
//! A package is a directory with a root `plugin.json`. Its `skills/`
//! directory becomes library skills and its `mcp.json` becomes library MCP
//! servers. Automatic does not launch servers, so each stdio server is
//! stored with absolute paths that point back into the package folder. The
//! package must therefore stay where it is after import.
//!
//! "Agent package" is used in code instead of "plugin" because Automatic
//! already has unrelated internal app plugins (`app_plugins.rs`,
//! `plugins.rs`).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use super::asset_security::{enforce_text_asset, validate_relative_asset_path, AssetKind};
use super::paths::{get_automatic_dir, home_dir};

pub const AGENT_PACKAGE_PLUGIN_SCHEMA: &str =
    "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json";
/// Both schema URLs are pinned to 1.0.0, so requiring this exact value also
/// enforces the rule that `mcp.json` uses the same version as `plugin.json`.
pub const AGENT_PACKAGE_MCP_SCHEMA: &str =
    "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json";

const ROOT_PLACEHOLDER: &str = "${PLUGIN_ROOT}";
const DATA_PLACEHOLDER: &str = "${PLUGIN_DATA}";
const ROOT_ENV: &str = "PLUGIN_ROOT";
const DATA_ENV: &str = "PLUGIN_DATA";

const PLUGIN_FIELDS: &[&str] = &[
    "$schema",
    "name",
    "version",
    "description",
    "author",
    "homepage",
    "repository",
    "license",
    "keywords",
    "extensions",
];
/// `plugin.json` and `mcp.json` larger than this are refused before reading.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
/// The folders and root files the skill importer copies with a skill.
const SKILL_COMPANION_DIRS: &[&str] = &[
    "scripts",
    "references",
    "docs",
    "assets",
    "examples",
    "templates",
];
const SKILL_COMPANION_FILES: &[&str] = &["LICENSE", "LICENSE.md", "LICENSE.txt", "README.md"];

const STDIO_FIELDS: &[&str] = &["type", "command", "args", "env", "cwd"];
const REMOTE_FIELDS: &[&str] = &["type", "url", "headers"];

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentPackageItemKind {
    Skill,
    McpServer,
}

/// A skill or MCP server that was not imported, and why.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPackageSkippedItem {
    pub kind: AgentPackageItemKind,
    pub name: String,
    pub reason: String,
}

/// A conforming skill directory inside the package's `skills/` folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPackageSkill {
    pub name: String,
    /// Absolute path of the skill directory inside the package.
    pub dir: String,
}

/// A conforming MCP server, already mapped to Automatic's config format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPackageMcpServer {
    pub name: String,
    pub config: Value,
}

/// Everything `read_agent_package` found in a valid package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPackage {
    /// Absolute, symlink-resolved package root.
    pub root: String,
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    /// The directory `${PLUGIN_DATA}` expands to. It is not created by a read.
    pub data_dir: String,
    pub skills: Vec<AgentPackageSkill>,
    pub mcp_servers: Vec<AgentPackageMcpServer>,
    pub invalid: Vec<AgentPackageSkippedItem>,
    pub warnings: Vec<String>,
    /// SHA-256 over `plugin.json`, `mcp.json` and each skill's `SKILL.md`.
    /// An import must be given the digest its preview showed.
    pub digest: String,
}

/// A skill or server the import will add, and the projects that already
/// name it and so receive it on the post-import sync.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPackagePlannedItem {
    pub name: String,
    pub projects: Vec<String>,
}

/// What an import of a package will do. Built by one pure function that
/// both the preview and the import use.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentPackagePlan {
    pub skills: Vec<AgentPackagePlannedItem>,
    pub mcp_servers: Vec<AgentPackagePlannedItem>,
    pub conflicts: Vec<AgentPackageSkippedItem>,
}

/// The read package and its plan, shown before the user confirms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPackagePreview {
    pub package: AgentPackage,
    pub plan: AgentPackagePlan,
}

/// The outcome of `import_agent_package`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPackageImportResult {
    pub package_name: String,
    pub package_root: String,
    pub data_dir: String,
    pub imported_skills: Vec<String>,
    pub imported_mcp_servers: Vec<String>,
    /// Items skipped because the library already has that name.
    pub conflicts: Vec<AgentPackageSkippedItem>,
    /// Items skipped because they do not conform, or failed a security scan.
    pub invalid: Vec<AgentPackageSkippedItem>,
    pub warnings: Vec<String>,
}

/// One valid package found in `~/.agents/plugins`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPackageSummary {
    pub path: String,
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub skill_count: usize,
    pub mcp_server_count: usize,
}

/// A folder in `~/.agents/plugins` that is not a valid package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPackageListError {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstalledAgentPackages {
    pub packages: Vec<AgentPackageSummary>,
    pub errors: Vec<AgentPackageListError>,
}

/// The two directories placeholders expand to.
struct PackagePaths {
    root: PathBuf,
    root_str: String,
    data: PathBuf,
    data_str: String,
}

/// Metadata from a validated `plugin.json`.
struct PluginManifest {
    raw: String,
    name: String,
    version: Option<String>,
    description: Option<String>,
}

// ── Reading ──────────────────────────────────────────────────────────────────

/// True when `path` is a directory with a `plugin.json` file at its root.
/// Used by the UI to decide which import to run. It does not validate.
pub fn is_agent_package_dir(path: &str) -> bool {
    Path::new(path).join("plugin.json").is_file()
}

/// Validate the package at `path` and describe what an import would add.
///
/// Read-only: nothing is written. Returns `Err` when the package is rejected
/// as a whole (no `plugin.json`, invalid `name` or `$schema`, wrong field
/// types). Problems with single skills or servers are listed in `invalid`;
/// problems that disable a whole component are listed in `warnings`.
pub fn read_agent_package(path: &str) -> Result<AgentPackage, String> {
    let root = fs::canonicalize(path)
        .map_err(|e| format!("Cannot open Agent Plugin package '{}': {}", path, e))?;
    if !root.is_dir() {
        return Err(format!(
            "Agent Plugin package '{}' must be a folder",
            root.display()
        ));
    }
    let mut warnings = Vec::new();
    let manifest = read_plugin_manifest(&root, &mut warnings)?;
    let paths = package_paths(root, &manifest.name)?;
    let mut invalid = Vec::new();
    let skills = discover_skills(&paths.root, &mut invalid, &mut warnings);
    let mut mcp_raw = None;
    let mcp_servers = read_mcp_servers(&paths, &mut mcp_raw, &mut invalid, &mut warnings);
    let digest = package_digest(&manifest.raw, mcp_raw.as_deref(), &skills);
    Ok(AgentPackage {
        digest,
        root: paths.root_str,
        name: manifest.name,
        version: manifest.version,
        description: manifest.description,
        data_dir: paths.data_str,
        skills,
        mcp_servers,
        invalid,
        warnings,
    })
}

fn package_paths(root: PathBuf, name: &str) -> Result<PackagePaths, String> {
    let data = agent_package_data_dir(name)?;
    Ok(PackagePaths {
        root_str: path_to_string(&root)?,
        data_str: path_to_string(&data)?,
        root,
        data,
    })
}

/// `<automatic dir>/agent-packages/<name>/data`. `name` has already passed
/// the package name rules, so it is one plain path component.
fn agent_package_data_dir(name: &str) -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?
        .join("agent-packages")
        .join(name)
        .join("data"))
}

fn path_to_string(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| format!("Path '{}' is not valid UTF-8", path.display()))
}

/// Read a manifest file, refusing one over [`MAX_MANIFEST_BYTES`].
fn read_capped(file: &Path, label: &str) -> Result<String, String> {
    let size = fs::metadata(file)
        .map_err(|e| format!("Failed to read {}: {}", label, e))?
        .len();
    if size > MAX_MANIFEST_BYTES {
        return Err(format!(
            "{} is too large ({} bytes; the limit is {} bytes)",
            label, size, MAX_MANIFEST_BYTES
        ));
    }
    fs::read_to_string(file).map_err(|e| format!("Failed to read {}: {}", label, e))
}

// ── plugin.json ──────────────────────────────────────────────────────────────

fn read_plugin_manifest(root: &Path, warnings: &mut Vec<String>) -> Result<PluginManifest, String> {
    let file = root.join("plugin.json");
    if fs::symlink_metadata(&file).is_err() {
        return Err(format!(
            "'{}' is not an Agent Plugin package: it has no plugin.json",
            root.display()
        ));
    }
    let file = resolve_inside(root, "plugin.json", "plugin.json")?;
    if !file.is_file() {
        return Err("plugin.json must be a regular file".to_string());
    }
    let raw = read_capped(&file, "plugin.json")?;
    let value: Value =
        serde_json::from_str(&raw).map_err(|e| format!("plugin.json is not valid JSON: {}", e))?;
    let obj = value
        .as_object()
        .ok_or("plugin.json must contain a JSON object")?;
    let mut manifest = parse_plugin_manifest(obj, warnings)?;
    manifest.raw = raw;
    Ok(manifest)
}

/// Hash every file whose content decides what the import writes, each part
/// length-prefixed so two different packages cannot hash alike. Companion
/// files are not covered; each skill is re-checked just before it is copied.
fn package_digest(plugin_raw: &str, mcp_raw: Option<&str>, skills: &[AgentPackageSkill]) -> String {
    let mut hasher = Sha256::new();
    hash_part(&mut hasher, b"plugin.json", plugin_raw.as_bytes());
    match mcp_raw {
        Some(raw) => hash_part(&mut hasher, b"mcp.json", raw.as_bytes()),
        None => hash_part(&mut hasher, b"no mcp.json", b""),
    }
    for skill in skills {
        // An unreadable SKILL.md hashes as a marker. The import then fails
        // for that skill with the read error, so nothing unseen is written.
        let content = fs::read(Path::new(&skill.dir).join("SKILL.md"))
            .unwrap_or_else(|_| b"unreadable".to_vec());
        hash_part(&mut hasher, skill.name.as_bytes(), &content);
    }
    hex::encode(hasher.finalize())
}

fn hash_part(hasher: &mut Sha256, label: &[u8], content: &[u8]) {
    hasher.update((label.len() as u64).to_le_bytes());
    hasher.update(label);
    hasher.update((content.len() as u64).to_le_bytes());
    hasher.update(content);
}

fn parse_plugin_manifest(
    obj: &Map<String, Value>,
    warnings: &mut Vec<String>,
) -> Result<PluginManifest, String> {
    for key in obj.keys() {
        if !PLUGIN_FIELDS.contains(&key.as_str()) {
            warnings.push(format!(
                "plugin.json has an unknown field '{}'. It was ignored.",
                key
            ));
        }
    }
    match obj.get("$schema") {
        Some(Value::String(s)) if s == AGENT_PACKAGE_PLUGIN_SCHEMA => {}
        Some(_) => {
            return Err(format!(
                "plugin.json '$schema' must be '{}'",
                AGENT_PACKAGE_PLUGIN_SCHEMA
            ))
        }
        None => return Err("plugin.json has no '$schema'".to_string()),
    }
    let name = match obj.get("name") {
        Some(Value::String(name)) => name.clone(),
        Some(_) => return Err("plugin.json 'name' must be a string".to_string()),
        None => return Err("plugin.json has no 'name'".to_string()),
    };
    validate_package_name(&name)?;
    validate_optional_fields(obj, warnings)?;
    Ok(PluginManifest {
        raw: String::new(),
        name,
        version: optional_string(obj, "version")?,
        description: optional_string(obj, "description")?,
    })
}

/// Check the types of the optional metadata fields. A wrong type rejects
/// the package, except `extensions`, which is ignored with a warning.
fn validate_optional_fields(
    obj: &Map<String, Value>,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    for key in [
        "version",
        "description",
        "homepage",
        "repository",
        "license",
    ] {
        optional_string(obj, key)?;
    }
    if let Some(keywords) = obj.get("keywords") {
        let all_strings = keywords
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_string));
        if !all_strings {
            return Err("plugin.json 'keywords' must be an array of strings".to_string());
        }
    }
    if let Some(author) = obj.get("author") {
        validate_author(author)?;
    }
    if obj.get("extensions").is_some_and(|ext| !ext.is_object()) {
        warnings.push("plugin.json 'extensions' is not an object. It was ignored.".to_string());
    }
    Ok(())
}

fn optional_string(obj: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("plugin.json '{}' must be a string", key)),
    }
}

fn validate_author(author: &Value) -> Result<(), String> {
    let obj = author
        .as_object()
        .ok_or("plugin.json 'author' must be an object")?;
    for (key, value) in obj {
        if !["name", "email", "url"].contains(&key.as_str()) {
            return Err(format!(
                "plugin.json 'author' has an unknown field '{}'",
                key
            ));
        }
        if !value.is_string() {
            return Err(format!("plugin.json 'author.{}' must be a string", key));
        }
    }
    Ok(())
}

/// Package names: 1-64 characters from `[a-z0-9.-]`, starting and ending
/// with a letter or digit, with no `--` or `..`.
fn validate_package_name(name: &str) -> Result<(), String> {
    let allowed = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-');
    let edge_ok = |c: Option<char>| c.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let valid = (1..=64).contains(&name.len())
        && allowed
        && edge_ok(name.chars().next())
        && edge_ok(name.chars().last())
        && !name.contains("--")
        && !name.contains("..");
    if valid {
        return Ok(());
    }
    Err(format!(
        "plugin.json name '{}' is invalid: use 1-64 lowercase letters, digits, '.' or '-', start and end with a letter or digit, and do not use '--' or '..'",
        name.escape_debug()
    ))
}

// ── Paths ────────────────────────────────────────────────────────────────────

/// Resolve `rel` against the canonical `root` and require that the result,
/// after following symlinks, stays inside `root`. The target need not exist:
/// the deepest existing ancestor is resolved and the rest appended.
fn resolve_inside(root: &Path, rel: &str, label: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Ok(root.to_path_buf());
    }
    validate_relative_asset_path(rel, label)?;
    let joined = root.join(rel);
    let existing = joined
        .ancestors()
        .find(|p| fs::symlink_metadata(p).is_ok())
        .unwrap_or(root);
    let resolved = fs::canonicalize(existing)
        .map_err(|e| format!("Failed to resolve {} '{}': {}", label, rel, e))?;
    if !resolved.starts_with(root) {
        return Err(format!(
            "{} '{}' points outside the package folder",
            label, rel
        ));
    }
    let rest = joined
        .strip_prefix(existing)
        .map_err(|e| format!("Failed to resolve {} '{}': {}", label, rel, e))?;
    // `join("")` would add a trailing separator, which makes `is_file` fail.
    if rest.as_os_str().is_empty() {
        return Ok(resolved);
    }
    Ok(resolved.join(rest))
}

/// Replace `${PLUGIN_ROOT}` and `${PLUGIN_DATA}` in one left-to-right pass.
/// Text produced by a replacement is never scanned again.
fn expand_placeholders(input: &str, paths: &PackagePaths) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(c) = rest.chars().next() {
        if let Some(tail) = rest.strip_prefix(ROOT_PLACEHOLDER) {
            out.push_str(&paths.root_str);
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix(DATA_PLACEHOLDER) {
            out.push_str(&paths.data_str);
            rest = tail;
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

// ── Skills ───────────────────────────────────────────────────────────────────

/// Find each immediate child of `skills/` that holds a regular `SKILL.md`.
/// There is no recursive search. A missing `skills/` is not an error.
fn discover_skills(
    root: &Path,
    invalid: &mut Vec<AgentPackageSkippedItem>,
    warnings: &mut Vec<String>,
) -> Vec<AgentPackageSkill> {
    let skills_path = root.join("skills");
    if fs::symlink_metadata(&skills_path).is_err() {
        return Vec::new();
    }
    let dir = match resolve_inside(root, "skills", "skills folder") {
        Ok(dir) if dir.is_dir() => dir,
        Ok(_) => {
            warnings.push("'skills' is not a folder. No skills were read.".to_string());
            return Vec::new();
        }
        Err(e) => {
            warnings.push(format!("{}. No skills were read.", e));
            return Vec::new();
        }
    };
    let mut found = Vec::new();
    for (name, path) in sorted_children(&dir, warnings) {
        match check_skill_dir(root, &name, &path) {
            Ok(Some(skill)) => found.push(skill),
            Ok(None) => {}
            Err(reason) => invalid.push(skipped(AgentPackageItemKind::Skill, &name, reason)),
        }
    }
    found
}

fn sorted_children(dir: &Path, warnings: &mut Vec<String>) -> Vec<(String, PathBuf)> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            warnings.push(format!("Failed to read '{}': {}", dir.display(), e));
            return Vec::new();
        }
    };
    let mut children: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().to_string(),
                entry.path(),
            )
        })
        .collect();
    children.sort();
    children
}

/// `Ok(None)` for a plain file in `skills/`, which is not a skill and not
/// worth reporting. `Err` for a directory that is not a conforming skill.
fn check_skill_dir(
    root: &Path,
    name: &str,
    path: &Path,
) -> Result<Option<AgentPackageSkill>, String> {
    let rel = format!("skills/{}", name);
    let dir = resolve_inside(root, &rel, "skill folder")?;
    if !dir.is_dir() {
        return Ok(None);
    }
    super::remote_sources::ensure_resource_name("skill", name)?;
    if !is_agent_skill_name(name) {
        return Err(
            "the folder name is not a valid Agent Skills name (lowercase letters, digits and single hyphens, up to 64 characters)"
                .to_string(),
        );
    }
    if !has_exact_child(&dir, "SKILL.md") {
        return Err("the folder has no SKILL.md file".to_string());
    }
    let skill_md = resolve_inside(root, &format!("{}/SKILL.md", rel), "SKILL.md")?;
    let is_symlink = fs::symlink_metadata(dir.join("SKILL.md"))
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(true);
    if is_symlink || !skill_md.is_file() {
        return Err("SKILL.md must be a regular file, not a symlink".to_string());
    }
    ensure_children_inside(root, &dir)?;
    ensure_no_companion_symlinks(&dir)?;
    Ok(Some(AgentPackageSkill {
        name: name.to_string(),
        dir: path_to_string(path)?,
    }))
}

/// Compare names exactly. `Path::exists` would accept `skill.md` on a
/// case-insensitive file system.
fn has_exact_child(dir: &Path, name: &str) -> bool {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|e| e.file_name() == name)
        })
        .unwrap_or(false)
}

/// Every entry of a skill folder must resolve inside the package. The skill
/// importer copies companion folders by following symlinks at the top level.
fn ensure_children_inside(root: &Path, dir: &Path) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read the folder: {}", e))?;
    for entry in entries.filter_map(Result::ok) {
        let resolved = fs::canonicalize(entry.path()).map_err(|e| {
            format!(
                "Failed to resolve '{}': {}",
                entry.file_name().to_string_lossy(),
                e
            )
        })?;
        if !resolved.starts_with(root) {
            return Err(format!(
                "'{}' points outside the package folder",
                entry.file_name().to_string_lossy()
            ));
        }
    }
    Ok(())
}

/// The skill importer refuses symlinks in the files it copies. Finding them
/// here reports the skill as invalid before anything is written.
fn ensure_no_companion_symlinks(dir: &Path) -> Result<(), String> {
    for name in SKILL_COMPANION_FILES {
        if fs::symlink_metadata(dir.join(name)).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(format!(
                "'{}' is a symlink, which skills cannot contain",
                name
            ));
        }
    }
    for name in SKILL_COMPANION_DIRS {
        let path = dir.join(name);
        match fs::symlink_metadata(&path) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(format!(
                    "'{}' is a symlink, which skills cannot contain",
                    name
                ))
            }
            Ok(m) if m.is_dir() => reject_symlinks_below(&path, name)?,
            _ => {}
        }
    }
    Ok(())
}

fn reject_symlinks_below(dir: &Path, label: &str) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read '{}': {}", label, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read '{}': {}", label, e))?;
        let child = format!("{}/{}", label, entry.file_name().to_string_lossy());
        let file_type = entry
            .file_type()
            .map_err(|e| format!("Failed to inspect '{}': {}", child, e))?;
        if file_type.is_symlink() {
            return Err(format!(
                "'{}' is a symlink, which skills cannot contain",
                child
            ));
        }
        if file_type.is_dir() {
            reject_symlinks_below(&entry.path(), &child)?;
        }
    }
    Ok(())
}

fn skipped(kind: AgentPackageItemKind, name: &str, reason: String) -> AgentPackageSkippedItem {
    AgentPackageSkippedItem {
        kind,
        name: name.to_string(),
        reason,
    }
}

// ── mcp.json ─────────────────────────────────────────────────────────────────

/// Read and map `mcp.json`. A file-level problem disables MCP for the
/// package with a warning; a bad entry skips only that entry.
/// `raw` receives the file's text when it could be read, for the digest.
fn read_mcp_servers(
    paths: &PackagePaths,
    raw: &mut Option<String>,
    invalid: &mut Vec<AgentPackageSkippedItem>,
    warnings: &mut Vec<String>,
) -> Vec<AgentPackageMcpServer> {
    if fs::symlink_metadata(paths.root.join("mcp.json")).is_err() {
        return Vec::new();
    }
    let servers = match read_mcp_file(&paths.root, raw) {
        Ok(servers) => servers,
        Err(e) => {
            warnings.push(format!("{}. No MCP servers were read.", e));
            return Vec::new();
        }
    };
    let mut mapped = Vec::new();
    for (name, entry) in servers {
        match map_mcp_entry(&name, &entry, paths) {
            Ok(config) => mapped.push(AgentPackageMcpServer { name, config }),
            Err(reason) => invalid.push(skipped(AgentPackageItemKind::McpServer, &name, reason)),
        }
    }
    mapped
}

fn read_mcp_file(root: &Path, raw_out: &mut Option<String>) -> Result<Map<String, Value>, String> {
    let file = resolve_inside(root, "mcp.json", "mcp.json")?;
    if !file.is_file() {
        return Err("mcp.json is not a regular file".to_string());
    }
    let raw = raw_out.insert(read_capped(&file, "mcp.json")?);
    let value: Value =
        serde_json::from_str(raw).map_err(|e| format!("mcp.json is not valid JSON: {}", e))?;
    let obj = value
        .as_object()
        .ok_or("mcp.json must contain a JSON object")?;
    if let Some(key) = obj.keys().find(|k| *k != "$schema" && *k != "mcpServers") {
        return Err(format!("mcp.json has an unknown field '{}'", key));
    }
    if obj.get("$schema").and_then(Value::as_str) != Some(AGENT_PACKAGE_MCP_SCHEMA) {
        return Err(format!(
            "mcp.json '$schema' must be '{}', the same version as plugin.json",
            AGENT_PACKAGE_MCP_SCHEMA
        ));
    }
    match obj.get("mcpServers") {
        Some(Value::Object(servers)) => Ok(servers.clone()),
        Some(_) => Err("mcp.json 'mcpServers' must be an object".to_string()),
        None => Err("mcp.json has no 'mcpServers'".to_string()),
    }
}

/// Validate one `mcpServers` entry and map it to Automatic's format.
fn map_mcp_entry(name: &str, entry: &Value, paths: &PackagePaths) -> Result<Value, String> {
    super::remote_sources::ensure_resource_name("MCP server", name)?;
    if !is_package_server_name(name) {
        return Err(
            "the server name must use only ASCII letters, digits, '.', '_' and '-'".to_string(),
        );
    }
    let obj = entry.as_object().ok_or("the entry must be an object")?;
    let transport = obj
        .get("type")
        .and_then(Value::as_str)
        .ok_or("the entry has no 'type'")?;
    let config = match transport {
        "stdio" => {
            ensure_only_fields(obj, STDIO_FIELDS, transport)?;
            map_stdio(obj, paths)?
        }
        "streamable-http" | "sse" => {
            ensure_only_fields(obj, REMOTE_FIELDS, transport)?;
            map_remote(obj, transport)?
        }
        other => return Err(format!("unknown type '{}'", other)),
    };
    if let Some(reason) = super::mcp_docker::mcp_sync_block_reason(&config) {
        return Err(reason);
    }
    Ok(config)
}

fn ensure_only_fields(
    obj: &Map<String, Value>,
    allowed: &[&str],
    transport: &str,
) -> Result<(), String> {
    match obj.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(key) => Err(format!(
            "field '{}' is not allowed for type '{}'",
            key, transport
        )),
        None => Ok(()),
    }
}

fn map_stdio(obj: &Map<String, Value>, paths: &PackagePaths) -> Result<Value, String> {
    let command = obj
        .get("command")
        .and_then(Value::as_str)
        .ok_or("'command' is required and must be a string")?;
    let command = resolve_command(command, &paths.root)?;
    let raw_args = string_array(obj.get("args"), "args")?;
    for arg in &raw_args {
        ensure_only_package_placeholders(arg, "args")?;
    }
    let args = raw_args
        .iter()
        .map(|arg| Value::String(expand_placeholders(arg, paths)))
        .collect::<Vec<_>>();
    let env = map_env(obj.get("env"), paths)?;
    let cwd = match obj.get("cwd") {
        None => paths.root.clone(),
        Some(Value::String(raw)) => {
            ensure_only_package_placeholders(raw, "cwd")?;
            resolve_cwd(raw, paths)?
        }
        Some(_) => return Err("'cwd' must be a string".to_string()),
    };
    let mut config = Map::new();
    config.insert("type".to_string(), Value::String("stdio".to_string()));
    config.insert("command".to_string(), Value::String(command));
    config.insert("args".to_string(), Value::Array(args));
    config.insert("env".to_string(), Value::Object(env));
    config.insert("cwd".to_string(), Value::String(path_to_string(&cwd)?));
    Ok(Value::Object(config))
}

/// A command is one token: a bare executable name looked up on `PATH`, or a
/// `./` path inside the package. A `./` path becomes absolute so the agent
/// can start it from any working directory.
fn resolve_command(command: &str, root: &Path) -> Result<String, String> {
    if command.is_empty() || command.chars().any(char::is_whitespace) {
        return Err("'command' must be one token with no spaces".to_string());
    }
    if command == "." || command == ".." || command.contains("${") {
        return Err(format!(
            "command '{}' must be an executable name or a './' path, with no placeholder",
            command
        ));
    }
    if let Some(rel) = command.strip_prefix("./") {
        let path = resolve_inside(root, rel, "command")?;
        if !path.is_file() {
            return Err(format!(
                "command '{}' is not a file in the package",
                command
            ));
        }
        return path_to_string(&path);
    }
    if command.contains('/') || command.contains('\\') {
        return Err(format!(
            "command '{}' must be a bare name or a './' path inside the package",
            command
        ));
    }
    Ok(command.to_string())
}

fn string_array(value: Option<&Value>, label: &str) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .map(|v| v.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| format!("'{}' must be an array of strings", label))
}

/// Expand placeholders in env values, then set `PLUGIN_ROOT` and
/// `PLUGIN_DATA` last so they always hold the client's values.
fn map_env(value: Option<&Value>, paths: &PackagePaths) -> Result<Map<String, Value>, String> {
    let mut env = Map::new();
    if let Some(value) = value {
        let obj = value.as_object().ok_or("'env' must be an object")?;
        for (key, val) in obj {
            if key == ROOT_ENV || key == DATA_ENV {
                return Err(format!("'env' must not set '{}'", key));
            }
            if !is_env_name(key) {
                return Err(format!(
                    "env name '{}' must be letters, digits and '_', and not start with a digit",
                    key.escape_debug()
                ));
            }
            let text = val
                .as_str()
                .ok_or_else(|| format!("env value '{}' must be a string", key))?;
            ensure_only_package_placeholders(text, &format!("env value '{}'", key))?;
            env.insert(key.clone(), Value::String(expand_placeholders(text, paths)));
        }
    }
    env.insert(ROOT_ENV.to_string(), Value::String(paths.root_str.clone()));
    env.insert(DATA_ENV.to_string(), Value::String(paths.data_str.clone()));
    Ok(env)
}

/// `[A-Za-z_][A-Za-z0-9_]*`, the names every agent and shell accepts.
fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The spec leaves any `${...}` other than its two placeholders as literal
/// text. Automatic's sync would later expand `${workspaceFolder}`,
/// `${userHome}` and `${env:...}`, and agents rewrite `${VAR}` in headers,
/// so such a value cannot be stored as the package wrote it.
fn ensure_only_package_placeholders(raw: &str, label: &str) -> Result<(), String> {
    let rest = raw
        .replace(ROOT_PLACEHOLDER, "")
        .replace(DATA_PLACEHOLDER, "");
    if rest.contains("${") {
        return Err(format!(
            "{} '{}' uses a placeholder other than ${{PLUGIN_ROOT}} or ${{PLUGIN_DATA}}, which Automatic cannot keep as literal text",
            label, raw
        ));
    }
    Ok(())
}

fn resolve_cwd(raw: &str, paths: &PackagePaths) -> Result<PathBuf, String> {
    let under = |prefix: &str| {
        if raw == prefix {
            Some("")
        } else {
            raw.strip_prefix(prefix).and_then(|r| r.strip_prefix('/'))
        }
    };
    if let Some(rel) = under(ROOT_PLACEHOLDER) {
        return resolve_inside(&paths.root, rel, "cwd");
    }
    if let Some(rel) = under(DATA_PLACEHOLDER) {
        if rel.is_empty() {
            return Ok(paths.data.clone());
        }
        validate_relative_asset_path(rel, "cwd")?;
        return Ok(paths.data.join(rel));
    }
    if let Some(rel) = raw.strip_prefix("./") {
        return resolve_inside(&paths.root, rel, "cwd");
    }
    Err(format!(
        "cwd '{}' must be a './' path, or start with ${{PLUGIN_ROOT}} or ${{PLUGIN_DATA}}",
        raw
    ))
}

fn map_remote(obj: &Map<String, Value>, transport: &str) -> Result<Value, String> {
    let url = obj
        .get("url")
        .and_then(Value::as_str)
        .ok_or("'url' is required and must be a string")?;
    validate_remote_url(url)?;
    if url.contains("${") {
        return Err(format!("url '{}' must not contain a placeholder", url));
    }
    let kind = if transport == "sse" { "sse" } else { "http" };
    let mut config = Map::new();
    config.insert("type".to_string(), Value::String(kind.to_string()));
    config.insert("url".to_string(), Value::String(url.to_string()));
    if let Some(headers) = obj.get("headers") {
        config.insert(
            "headers".to_string(),
            Value::Object(validate_headers(headers)?),
        );
    }
    Ok(Value::Object(config))
}

/// Absolute http(s) URL with no userinfo or fragment. Plain http is only
/// allowed for `localhost` or a loopback IP literal.
fn validate_remote_url(raw: &str) -> Result<(), String> {
    let url = url::Url::parse(raw).map_err(|e| format!("url '{}' is invalid: {}", raw, e))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(format!("url '{}' must use http or https", raw));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(format!(
            "url '{}' must not contain a user name or password",
            raw
        ));
    }
    if url.fragment().is_some() {
        return Err(format!("url '{}' must not contain a fragment", raw));
    }
    let host = url
        .host()
        .ok_or_else(|| format!("url '{}' has no host", raw))?;
    let is_loopback = match host {
        url::Host::Domain(domain) => domain.eq_ignore_ascii_case("localhost"),
        url::Host::Ipv4(ip) => ip.is_loopback(),
        url::Host::Ipv6(ip) => ip.is_loopback(),
    };
    if url.scheme() == "http" && !is_loopback {
        return Err(format!(
            "url '{}' must use https; http is only allowed for localhost",
            raw
        ));
    }
    Ok(())
}

fn validate_headers(value: &Value) -> Result<Map<String, Value>, String> {
    let obj = value.as_object().ok_or("'headers' must be an object")?;
    let mut seen = std::collections::HashSet::new();
    for (key, val) in obj {
        let text = val
            .as_str()
            .ok_or_else(|| format!("header '{}' must be a string", key))?;
        if key.is_empty() || !key.chars().all(is_header_token_char) {
            return Err(format!(
                "header name '{}' is not a valid HTTP header name",
                key.escape_debug()
            ));
        }
        if text.chars().any(|c| c.is_control() && c != '\t') {
            return Err(format!(
                "header '{}' has a control character in its value",
                key
            ));
        }
        if text.contains("${") {
            return Err(format!(
                "header '{}' uses a placeholder, which Automatic cannot keep as literal text",
                key
            ));
        }
        if !seen.insert(key.to_ascii_lowercase()) {
            return Err(format!(
                "header '{}' appears more than once with different letter case",
                key
            ));
        }
    }
    Ok(obj.clone())
}

/// RFC 7230 `tchar`.
fn is_header_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)
}

// ── Import ───────────────────────────────────────────────────────────────────

/// Library names an import must not reuse. Read before anything is written,
/// so a failed read leaves the library untouched.
pub(crate) struct LibraryNames {
    skills: Vec<String>,
    servers: Vec<String>,
}

fn read_library_names() -> Result<LibraryNames, String> {
    let mut skills = library_skill_names()?;
    skills.push(super::mcp_servers::AUTOMATIC_SKILL_NAME.to_string());
    let mut servers = super::mcp_servers::list_mcp_server_configs()?;
    servers.push(super::mcp_servers::AUTOMATIC_SERVER_NAME.to_string());
    Ok(LibraryNames { skills, servers })
}

/// Every folder in the library's skills directory, with or without a
/// `SKILL.md`, because the import writes into a folder of that name.
fn library_skill_names() -> Result<Vec<String>, String> {
    let dir = super::paths::get_library_skills_dir()?;
    match fs::read_dir(&dir) {
        Ok(entries) => Ok(entries
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("Failed to read the skills library: {}", e)),
    }
}

/// The skill and server names one project uses.
pub(crate) struct ProjectReferences {
    project: String,
    skills: Vec<String>,
    servers: Vec<String>,
}

/// Read every project's skill and server names. A project that cannot be
/// read is skipped, as the post-import sync skips it, so it receives
/// nothing from the import either.
fn read_project_references() -> Result<Vec<ProjectReferences>, String> {
    let mut references = Vec::new();
    for ident in super::list_project_idents()? {
        let Ok(raw) = super::read_project(&ident) else {
            continue;
        };
        let Ok(project) = serde_json::from_str::<super::Project>(&raw) else {
            continue;
        };
        references.push(ProjectReferences {
            project: project.name,
            skills: project.skills,
            servers: project.mcp_servers,
        });
    }
    Ok(references)
}

/// Equal, or equal once NFC-normalised and lowercased. Library files can sit
/// on a file system that ignores letter case and Unicode normalisation
/// (APFS, NTFS), where both names reach the same file.
fn same_name(a: &str, b: &str) -> bool {
    a == b || fold_name(a) == fold_name(b)
}

fn fold_name(name: &str) -> String {
    name.nfc().collect::<String>().to_lowercase()
}

/// Automatic's rule for package server names. The spec sets none, but APFS
/// folds some non-ASCII letters onto ASCII ones (`ſ` opens `s`), so a
/// non-ASCII name could reach a project's `<name>.json` reference without the
/// preview's case check seeing it.
fn is_package_server_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The Agent Skills name rule: 1-64 of `a-z`, `0-9` and single hyphens, not
/// at either end. https://agentskills.io/specification
fn is_agent_skill_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// Decide which skills and servers an import adds. Pure: the preview and
/// the import both call it with names read beforehand, so they agree.
fn plan_agent_package(
    package: &AgentPackage,
    library: &LibraryNames,
    projects: &[ProjectReferences],
) -> AgentPackagePlan {
    let mut plan = AgentPackagePlan::default();
    let skill_refs = project_refs(projects, |p| &p.skills);
    let server_refs = project_refs(projects, |p| &p.servers);
    let skills = package.skills.iter().map(|s| s.name.as_str());
    let (planned, conflicts) = plan_kind(
        AgentPackageItemKind::Skill,
        skills,
        &library.skills,
        &skill_refs,
    );
    plan.skills = planned;
    plan.conflicts.extend(conflicts);
    let servers = package.mcp_servers.iter().map(|s| s.name.as_str());
    let (planned, conflicts) = plan_kind(
        AgentPackageItemKind::McpServer,
        servers,
        &library.servers,
        &server_refs,
    );
    plan.mcp_servers = planned;
    plan.conflicts.extend(conflicts);
    plan
}

/// `(project name, referenced name)` pairs for one kind.
fn project_refs<'a>(
    projects: &'a [ProjectReferences],
    names: impl Fn(&'a ProjectReferences) -> &'a Vec<String>,
) -> Vec<(&'a str, &'a str)> {
    projects
        .iter()
        .flat_map(|p| {
            names(p)
                .iter()
                .map(move |n| (p.project.as_str(), n.as_str()))
        })
        .collect()
}

fn plan_kind<'a>(
    kind: AgentPackageItemKind,
    names: impl Iterator<Item = &'a str>,
    taken: &[String],
    refs: &[(&str, &str)],
) -> (Vec<AgentPackagePlannedItem>, Vec<AgentPackageSkippedItem>) {
    let mut planned: Vec<AgentPackagePlannedItem> = Vec::new();
    let mut conflicts = Vec::new();
    for name in names {
        match plan_conflict(name, taken, &planned, refs) {
            Some(reason) => conflicts.push(skipped(kind, name, reason)),
            None => planned.push(AgentPackagePlannedItem {
                name: name.to_string(),
                projects: refs
                    .iter()
                    .filter(|(_, r)| *r == name)
                    .map(|(p, _)| p.to_string())
                    .collect(),
            }),
        }
    }
    (planned, conflicts)
}

/// Why `name` cannot be imported, or `None` when it can.
fn plan_conflict(
    name: &str,
    taken: &[String],
    planned: &[AgentPackagePlannedItem],
    refs: &[(&str, &str)],
) -> Option<String> {
    if taken.iter().any(|t| same_name(t, name)) {
        return Some(conflict_reason());
    }
    if planned.iter().any(|p| same_name(&p.name, name)) {
        return Some(
            "another item in this package has the same name in different letter case".to_string(),
        );
    }
    refs.iter()
        .find(|(_, r)| *r != name && same_name(r, name))
        .map(|(project, r)| {
            format!(
                "the name differs only in letter case from '{}' used by project {}",
                r, project
            )
        })
}

/// Read a package and plan its import without changing anything.
pub fn preview_agent_package(path: &str) -> Result<AgentPackagePreview, String> {
    let package = read_agent_package(path)?;
    let plan = plan_agent_package(
        &package,
        &read_library_names()?,
        &read_project_references()?,
    );
    Ok(AgentPackagePreview { package, plan })
}

/// Read the package at `path`, plan the import as the preview did, then add
/// the planned skills and servers. Refuses when the package's digest is not
/// `expected_digest`, the one its preview showed. Every write is
/// create-only, so nothing in the library is ever replaced. Creates the
/// package's `${PLUGIN_DATA}` folder; never deletes it.
pub fn import_agent_package(
    path: &str,
    expected_digest: &str,
) -> Result<AgentPackageImportResult, String> {
    let package = read_agent_package(path)?;
    if package.digest != expected_digest {
        return Err("The package changed after the preview. Preview it again.".to_string());
    }
    let plan = plan_agent_package(
        &package,
        &read_library_names()?,
        &read_project_references()?,
    );
    fs::create_dir_all(&package.data_dir).map_err(|e| {
        format!(
            "Failed to create the data folder '{}': {}",
            package.data_dir, e
        )
    })?;
    let mut result = AgentPackageImportResult {
        package_name: package.name.clone(),
        package_root: package.root.clone(),
        data_dir: package.data_dir.clone(),
        imported_skills: Vec::new(),
        imported_mcp_servers: Vec::new(),
        conflicts: plan.conflicts.clone(),
        invalid: package.invalid.clone(),
        warnings: package.warnings.clone(),
    };
    apply_plan(&package, &plan, &mut result);
    assign_skill_collection(&package.name, &mut result);
    Ok(result)
}

fn apply_plan(
    package: &AgentPackage,
    plan: &AgentPackagePlan,
    result: &mut AgentPackageImportResult,
) {
    let root = PathBuf::from(&package.root);
    for item in &plan.skills {
        if let Some(skill) = package.skills.iter().find(|s| s.name == item.name) {
            import_one_skill(&root, skill, result);
        }
    }
    for item in &plan.mcp_servers {
        if let Some(server) = package.mcp_servers.iter().find(|s| s.name == item.name) {
            import_one_server(server, result);
        }
    }
}

/// Why a skill write did not happen.
#[derive(Debug)]
enum SkillWriteError {
    /// The library folder already exists, perhaps under another letter case
    /// or Unicode normalisation of the name.
    Conflict,
    Failed(String),
}

impl From<String> for SkillWriteError {
    fn from(message: String) -> Self {
        SkillWriteError::Failed(message)
    }
}

impl std::fmt::Display for SkillWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillWriteError::Conflict => f.write_str(&conflict_reason()),
            SkillWriteError::Failed(message) => f.write_str(message),
        }
    }
}

fn import_one_skill(root: &Path, skill: &AgentPackageSkill, result: &mut AgentPackageImportResult) {
    let kind = AgentPackageItemKind::Skill;
    // An import is the user's write, so no source owns the new skill.
    match super::remote_sources::as_user_write("skill", &skill.name, || {
        import_skill_files(root, skill)
    }) {
        Ok(()) => result.imported_skills.push(skill.name.clone()),
        Err(SkillWriteError::Conflict) => {
            result
                .conflicts
                .push(skipped(kind, &skill.name, conflict_reason()))
        }
        Err(SkillWriteError::Failed(e)) => result.invalid.push(skipped(kind, &skill.name, e)),
    }
}

/// Check the skill again just before copying, because the package folder
/// can change after it was read. The library folder is created with
/// `create_dir`, never `create_dir_all`, so an existing folder the file
/// system treats as the same name is never written into. If the copy fails
/// part way, the folder and its scan record are removed.
fn import_skill_files(root: &Path, skill: &AgentPackageSkill) -> Result<(), SkillWriteError> {
    let source = Path::new(&skill.dir);
    check_skill_dir(root, &skill.name, source)
        .map_err(SkillWriteError::Failed)?
        .ok_or_else(|| SkillWriteError::Failed("the skill folder is no longer a folder".into()))?;
    let library = super::paths::get_library_skills_dir().map_err(SkillWriteError::Failed)?;
    fs::create_dir_all(&library).map_err(|e| {
        SkillWriteError::Failed(format!("Failed to create the skills library: {}", e))
    })?;
    let target = library.join(&skill.name);
    match fs::create_dir(&target) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(SkillWriteError::Conflict)
        }
        Err(e) => {
            return Err(SkillWriteError::Failed(format!(
                "Failed to create the skill folder: {}",
                e
            )))
        }
    }
    let skill_md = source.join("SKILL.md");
    super::skills::import_skill_from_local_path(&skill_md.to_string_lossy())
        .map(|_| ())
        .map_err(|e| SkillWriteError::Failed(remove_partial_skill(&target, &skill.name, e)))
}

/// Remove a skill folder this import created, and its scan record. Returns
/// the error to report, extended when the cleanup itself failed.
fn remove_partial_skill(target: &Path, name: &str, error: String) -> String {
    let mut message = error;
    if let Err(e) = fs::remove_dir_all(target) {
        if e.kind() != std::io::ErrorKind::NotFound {
            message.push_str(&format!(
                ". The partly written skill at '{}' could not be removed: {}",
                target.display(),
                e
            ));
        }
    }
    if let Err(e) = super::skills::remove_skill_scan_state(name) {
        message.push_str(&format!(". Its scan record could not be removed: {}", e));
    }
    message
}

fn import_one_server(server: &AgentPackageMcpServer, result: &mut AgentPackageImportResult) {
    let kind = AgentPackageItemKind::McpServer;
    match super::remote_sources::as_user_write("mcp_server", &server.name, || save_server(server)) {
        Ok(()) => result.imported_mcp_servers.push(server.name.clone()),
        Err(e) => result.invalid.push(skipped(kind, &server.name, e)),
    }
}

/// Same steps as a remote-source install: security scan, blank anything
/// that looks like ciphertext, then a registry write that encrypts env. The
/// write is create-only, so it can never replace an existing server.
fn save_server(server: &AgentPackageMcpServer) -> Result<(), String> {
    let content = serde_json::to_string_pretty(&server.config)
        .map_err(|e| format!("Failed to serialise MCP server '{}': {}", server.name, e))?;
    enforce_text_asset(
        AssetKind::Template,
        &format!("MCP server '{}'", server.name),
        &content,
    )?;
    let content = super::mcp_servers::blank_ciphertext_like_env_values(&content);
    super::mcp_servers::create_mcp_server_config(&server.name, &content)
}

fn conflict_reason() -> String {
    "the library already has an item with this name. It was not changed.".to_string()
}

/// Group the imported skills under a collection named after the package, as
/// a `skill.json` import does.
fn assign_skill_collection(package_name: &str, result: &mut AgentPackageImportResult) {
    if result.imported_skills.is_empty() {
        return;
    }
    if let Err(e) = super::skills::set_skills_collection(&result.imported_skills, package_name) {
        result
            .warnings
            .push(format!("The skills were imported but not grouped: {}", e));
    }
}

// ── Installed packages ──────────────────────────────────────────────────────

/// List the packages in `~/.agents/plugins`, one level deep. Read-only.
/// A folder that is not a valid package is listed in `errors`.
pub fn list_installed_agent_packages() -> Result<InstalledAgentPackages, String> {
    let dir = home_dir()?.join(".agents").join("plugins");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(InstalledAgentPackages::default())
        }
        Err(e) => return Err(format!("Failed to read '{}': {}", dir.display(), e)),
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    paths.sort();
    let mut installed = InstalledAgentPackages::default();
    for path in paths {
        let path_str = path.to_string_lossy().to_string();
        match read_agent_package(&path_str) {
            Ok(package) => installed.packages.push(summarise(path_str, package)),
            Err(error) => installed.errors.push(AgentPackageListError {
                path: path_str,
                error,
            }),
        }
    }
    Ok(installed)
}

fn summarise(path: String, package: AgentPackage) -> AgentPackageSummary {
    AgentPackageSummary {
        path,
        name: package.name,
        version: package.version,
        description: package.description,
        skill_count: package.skills.len(),
        mcp_server_count: package.mcp_servers.len(),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::with_test_home;
    use serde_json::json;
    use tempfile::TempDir;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn plugin_json(name: &str) -> Value {
        json!({ "$schema": AGENT_PACKAGE_PLUGIN_SCHEMA, "name": name, "version": "1.0.0" })
    }

    fn write_plugin(root: &Path, manifest: &Value) {
        write(&root.join("plugin.json"), &manifest.to_string());
    }

    fn write_mcp(root: &Path, servers: Value) {
        let doc = json!({ "$schema": AGENT_PACKAGE_MCP_SCHEMA, "mcpServers": servers });
        write(&root.join("mcp.json"), &doc.to_string());
    }

    fn write_skill(root: &Path, name: &str) {
        write(
            &root.join("skills").join(name).join("SKILL.md"),
            &format!(
                "---\nname: {}\ndescription: Test skill.\n---\n\nDo the thing.\n",
                name
            ),
        );
    }

    /// A temp home plus a package folder, with the home installed for the
    /// duration of `test`.
    fn with_package<T>(test: impl FnOnce(&Path) -> T) -> T {
        let home = TempDir::new().unwrap();
        let pkg = TempDir::new().unwrap();
        let root = fs::canonicalize(pkg.path()).unwrap();
        with_test_home(home.path().to_path_buf(), || test(&root))
    }

    fn read(root: &Path) -> Result<AgentPackage, String> {
        read_agent_package(root.to_str().unwrap())
    }

    /// Import as the dialog does: with the digest a fresh read reports.
    fn import(root: &Path) -> Result<AgentPackageImportResult, String> {
        let digest = read(root)?.digest;
        import_agent_package(root.to_str().unwrap(), &digest)
    }

    fn server<'a>(package: &'a AgentPackage, name: &str) -> &'a Value {
        &package
            .mcp_servers
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("server {} missing: {:?}", name, package.invalid))
            .config
    }

    fn invalid_names(package: &AgentPackage) -> Vec<String> {
        package.invalid.iter().map(|i| i.name.clone()).collect()
    }

    fn write_full_package(root: &Path) {
        write_plugin(root, &plugin_json("demo-pkg"));
        write_skill(root, "alpha");
        write_skill(root, "beta");
        write(&root.join("bin/server"), "#!/bin/sh\n");
        write_mcp(
            root,
            json!({
                "local": {
                    "type": "stdio",
                    "command": "./bin/server",
                    "args": ["--root", "${PLUGIN_ROOT}/conf", "--data=${PLUGIN_DATA}"],
                    "env": { "MODE": "${PLUGIN_ROOT}" }
                },
                "remote": {
                    "type": "streamable-http",
                    "url": "https://example.com/mcp",
                    "headers": { "X-Key": "abc" }
                },
                "events": { "type": "sse", "url": "https://example.com/sse" }
            }),
        );
    }

    #[test]
    fn valid_package_reads_skills_and_maps_servers() {
        with_package(|root| {
            write_full_package(root);
            let package = read(root).unwrap();
            let root_str = root.to_str().unwrap();
            assert_eq!(package.name, "demo-pkg");
            let names: Vec<_> = package.skills.iter().map(|s| s.name.as_str()).collect();
            assert_eq!(names, vec!["alpha", "beta"]);
            assert!(package.invalid.is_empty(), "{:?}", package.invalid);

            let local = server(&package, "local");
            assert_eq!(local["type"], "stdio");
            assert_eq!(local["command"], format!("{}/bin/server", root_str));
            assert_eq!(local["args"][1], format!("{}/conf", root_str));
            assert_eq!(local["args"][2], format!("--data={}", package.data_dir));
            assert_eq!(local["cwd"], root_str);
            assert_eq!(local["env"]["MODE"], root_str);
            assert_eq!(local["env"]["PLUGIN_ROOT"], root_str);
            assert_eq!(local["env"]["PLUGIN_DATA"], package.data_dir.as_str());
            assert!(package.data_dir.ends_with("agent-packages/demo-pkg/data"));

            let remote = server(&package, "remote");
            assert_eq!(
                remote,
                &json!({"type": "http", "url": "https://example.com/mcp", "headers": {"X-Key": "abc"}})
            );
            assert_eq!(
                server(&package, "events"),
                &json!({"type": "sse", "url": "https://example.com/sse"})
            );
        });
    }

    #[test]
    fn an_import_takes_its_names_away_from_a_source() {
        with_package(|root| {
            write_full_package(root);
            // Stale records, as a source leaves them for names it no longer
            // has files under.
            crate::core::remote_sources::record_provenance("skill", "alpha", "acme/tools").unwrap();
            crate::core::remote_sources::record_provenance("mcp_server", "local", "acme/tools")
                .unwrap();

            let result = import(root).unwrap();

            assert!(result.imported_skills.contains(&"alpha".to_string()));
            assert_eq!(
                crate::core::remote_sources::get_provenance("skill", "alpha"),
                Ok(None)
            );
            assert_eq!(
                crate::core::remote_sources::get_provenance("mcp_server", "local"),
                Ok(None)
            );
        });
    }

    #[test]
    fn an_import_that_conflicts_keeps_the_sources_record() {
        with_package(|root| {
            write_full_package(root);
            crate::core::skills::save_skill("alpha", "---\nname: alpha\n---\nsource\n").unwrap();
            crate::core::remote_sources::record_provenance("skill", "alpha", "acme/tools").unwrap();

            let result = import(root).unwrap();

            assert!(!result.imported_skills.contains(&"alpha".to_string()));
            assert_eq!(
                crate::core::remote_sources::get_provenance("skill", "alpha"),
                Ok(Some("acme/tools".to_string()))
            );
        });
    }

    #[test]
    fn import_adds_skills_and_servers_and_creates_data_dir() {
        with_package(|root| {
            write_full_package(root);
            let result = import(root).unwrap();
            assert_eq!(result.imported_skills, vec!["alpha", "beta"]);
            let mut servers = result.imported_mcp_servers.clone();
            servers.sort();
            assert_eq!(servers, vec!["events", "local", "remote"]);
            assert!(result.conflicts.is_empty() && result.invalid.is_empty());
            assert!(Path::new(&result.data_dir).is_dir());
            assert!(crate::core::skills::skill_exists("alpha"));
            let saved: Value = serde_json::from_str(
                &crate::core::mcp_servers::read_mcp_server_config("local").unwrap(),
            )
            .unwrap();
            assert_eq!(saved["env"]["PLUGIN_ROOT"], root.to_str().unwrap());
        });
    }

    #[test]
    fn an_imported_server_with_a_ciphertext_like_env_value_is_stored_readably() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_mcp(
                root,
                json!({"planted": {"type": "stdio", "command": "npx", "env": {"API_KEY": "enc:v1:AAAA", "REGION": "eu"}}}),
            );
            let result = import(root).unwrap();
            assert_eq!(
                result.imported_mcp_servers,
                vec!["planted"],
                "{:?}",
                result.invalid
            );

            let saved: Value = serde_json::from_str(
                &crate::core::mcp_servers::read_mcp_server_config("planted")
                    .expect("the imported entry must be readable"),
            )
            .unwrap();
            assert_eq!(saved["env"]["API_KEY"], "");
            assert_eq!(saved["env"]["REGION"], "eu");
        });
    }

    #[test]
    fn existing_library_names_are_skipped_not_overwritten() {
        with_package(|root| {
            write_full_package(root);
            crate::core::skills::save_skill("alpha", "---\nname: alpha\n---\noriginal\n").unwrap();
            crate::core::mcp_servers::save_mcp_server_config(
                "remote",
                r#"{"type":"http","url":"https://keep.example/mcp"}"#,
            )
            .unwrap();

            let result = import(root).unwrap();
            let conflicts: Vec<_> = result.conflicts.iter().map(|c| c.name.as_str()).collect();
            assert_eq!(conflicts, vec!["alpha", "remote"]);
            assert_eq!(result.imported_skills, vec!["beta"]);
            assert!(crate::core::skills::read_skill_raw("alpha")
                .unwrap()
                .contains("original"));
            let kept = crate::core::mcp_servers::read_mcp_server_config("remote").unwrap();
            assert!(kept.contains("keep.example"));
        });
    }

    #[test]
    fn missing_or_wrong_schema_rejects_package() {
        with_package(|root| {
            write(&root.join("plugin.json"), r#"{"name":"demo"}"#);
            assert!(read(root).unwrap_err().contains("$schema"));
            let mut manifest = plugin_json("demo");
            manifest["$schema"] =
                json!("https://agent-plugins.org/schemas/2.0.0/plugin.schema.json");
            write_plugin(root, &manifest);
            assert!(read(root).unwrap_err().contains("$schema"));
        });
    }

    #[test]
    fn missing_plugin_json_rejects_package() {
        with_package(|root| {
            write_skill(root, "alpha");
            assert!(read(root).unwrap_err().contains("no plugin.json"));
        });
    }

    #[test]
    fn invalid_names_reject_package() {
        with_package(|root| {
            let long = "a".repeat(65);
            for name in ["A", "-a", "a-", "a--b", "a..b", "", "a_b", long.as_str()] {
                write_plugin(root, &plugin_json(name));
                assert!(read(root).is_err(), "name {:?} should be rejected", name);
            }
            let max = "a".repeat(64);
            for name in ["a", "a.b-c", "9lives", max.as_str()] {
                write_plugin(root, &plugin_json(name));
                assert!(read(root).is_ok(), "name {:?} should be accepted", name);
            }
        });
    }

    #[test]
    fn wrong_field_types_reject_package() {
        with_package(|root| {
            let cases = [
                ("version", json!(1)),
                ("keywords", json!(["ok", 2])),
                ("author", json!("someone")),
                ("author", json!({"name": "a", "twitter": "b"})),
            ];
            for (key, value) in cases {
                let mut manifest = plugin_json("demo");
                manifest[key] = value;
                write_plugin(root, &manifest);
                assert!(read(root).is_err(), "{} should be rejected", key);
            }
        });
    }

    #[test]
    fn unknown_fields_and_bad_extensions_warn_and_still_import() {
        with_package(|root| {
            let mut manifest = plugin_json("demo");
            manifest["colour"] = json!("blue");
            manifest["extensions"] = json!("nope");
            write_plugin(root, &manifest);
            write_skill(root, "alpha");
            let result = import(root).unwrap();
            assert_eq!(result.imported_skills, vec!["alpha"]);
            assert!(result.warnings.iter().any(|w| w.contains("'colour'")));
            assert!(result.warnings.iter().any(|w| w.contains("extensions")));
        });
    }

    #[test]
    fn inline_mcp_servers_in_plugin_json_are_ignored() {
        with_package(|root| {
            let mut manifest = plugin_json("demo");
            manifest["mcpServers"] = json!({"x": {"type": "stdio", "command": "node"}});
            write_plugin(root, &manifest);
            let package = read(root).unwrap();
            assert!(package.mcp_servers.is_empty());
            assert!(package.warnings.iter().any(|w| w.contains("'mcpServers'")));
        });
    }

    #[test]
    fn nested_skills_are_not_discovered() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write(&root.join("skills/a/b/SKILL.md"), "---\nname: b\n---\n");
            let package = read(root).unwrap();
            assert!(package.skills.is_empty());
            assert_eq!(invalid_names(&package), vec!["a"]);
        });
    }

    #[test]
    fn lowercase_skill_md_is_not_a_skill() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write(&root.join("skills/a/skill.md"), "x");
            let package = read(root).unwrap();
            assert!(package.skills.is_empty());
        });
    }

    #[test]
    fn skills_as_file_warns_and_mcp_still_loads() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write(&root.join("skills"), "not a folder");
            write_mcp(
                root,
                json!({"r": {"type": "sse", "url": "https://e.com/s"}}),
            );
            let package = read(root).unwrap();
            assert!(package.skills.is_empty());
            assert!(package.warnings.iter().any(|w| w.contains("'skills'")));
            assert_eq!(package.mcp_servers.len(), 1);
        });
    }

    #[test]
    fn mcp_schema_mismatch_disables_mcp_only() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_skill(root, "alpha");
            let doc = json!({
                "$schema": "https://agent-plugins.org/schemas/1.1.0/mcp.schema.json",
                "mcpServers": {"r": {"type": "sse", "url": "https://e.com/s"}}
            });
            write(&root.join("mcp.json"), &doc.to_string());
            let package = read(root).unwrap();
            assert!(package.mcp_servers.is_empty());
            assert_eq!(package.skills.len(), 1);
            assert!(package.warnings.iter().any(|w| w.contains("mcp.json")));
        });
    }

    #[test]
    fn mcp_extra_field_or_bad_json_disables_mcp() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            let doc = json!({"$schema": AGENT_PACKAGE_MCP_SCHEMA, "mcpServers": {}, "x": 1});
            write(&root.join("mcp.json"), &doc.to_string());
            assert!(read(root)
                .unwrap()
                .warnings
                .iter()
                .any(|w| w.contains("'x'")));
            write(&root.join("mcp.json"), "{not json");
            assert!(read(root)
                .unwrap()
                .warnings
                .iter()
                .any(|w| w.contains("valid JSON")));
        });
    }

    #[cfg(unix)]
    #[test]
    fn command_paths_must_stay_inside_package() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            let outside = TempDir::new().unwrap();
            write(&outside.path().join("evil"), "#!/bin/sh\n");
            fs::create_dir_all(root.join("bin")).unwrap();
            std::os::unix::fs::symlink(outside.path().join("evil"), root.join("bin/link")).unwrap();
            write_mcp(
                root,
                json!({
                    "up": {"type": "stdio", "command": "./../x"},
                    "link": {"type": "stdio", "command": "./bin/link"},
                    "abs": {"type": "stdio", "command": "/usr/bin/env"},
                    "two": {"type": "stdio", "command": "node server.js"},
                    "bare": {"type": "stdio", "command": "node"}
                }),
            );
            let package = read(root).unwrap();
            let mut bad = invalid_names(&package);
            bad.sort();
            assert_eq!(bad, vec!["abs", "link", "two", "up"]);
            assert_eq!(server(&package, "bare")["command"], "node");
        });
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_skill_folder_escaping_root_is_rejected() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            let outside = TempDir::new().unwrap();
            write(&outside.path().join("SKILL.md"), "---\nname: x\n---\n");
            fs::create_dir_all(root.join("skills")).unwrap();
            std::os::unix::fs::symlink(outside.path(), root.join("skills/escape")).unwrap();
            let package = read(root).unwrap();
            assert!(package.skills.is_empty());
            assert_eq!(invalid_names(&package), vec!["escape"]);
        });
    }

    #[test]
    fn env_must_not_set_plugin_variables() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_mcp(
                root,
                json!({
                    "a": {"type": "stdio", "command": "node", "env": {"PLUGIN_ROOT": "/x"}},
                    "b": {"type": "stdio", "command": "node", "env": {"PLUGIN_DATA": "/x"}},
                    "c": {"type": "stdio", "command": "node", "env": {"OK": "1"}}
                }),
            );
            let package = read(root).unwrap();
            assert_eq!(invalid_names(&package), vec!["a", "b"]);
            assert_eq!(server(&package, "c")["env"]["OK"], "1");
        });
    }

    #[test]
    fn cross_variant_unknown_type_and_unknown_fields_skip_entry() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_mcp(
                root,
                json!({
                    "mixed": {"type": "stdio", "command": "node", "url": "https://e.com"},
                    "mixed2": {"type": "sse", "url": "https://e.com", "args": []},
                    "odd": {"type": "websocket", "url": "wss://e.com"},
                    "notype": {"command": "node"}
                }),
            );
            let package = read(root).unwrap();
            assert!(package.mcp_servers.is_empty());
            assert_eq!(package.invalid.len(), 4);
        });
    }

    #[test]
    fn remote_url_rules() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_mcp(
                root,
                json!({
                    "plain": {"type": "streamable-http", "url": "http://example.com/mcp"},
                    "local": {"type": "streamable-http", "url": "http://localhost:3000/mcp"},
                    "loop4": {"type": "streamable-http", "url": "http://127.0.0.1:3000/mcp"},
                    "loop6": {"type": "sse", "url": "http://[::1]:3000/sse"},
                    "user": {"type": "streamable-http", "url": "https://me:pw@example.com/mcp"},
                    "frag": {"type": "streamable-http", "url": "https://example.com/mcp#x"},
                    "rel": {"type": "streamable-http", "url": "/mcp"},
                    "ftp": {"type": "streamable-http", "url": "ftp://example.com/mcp"}
                }),
            );
            let package = read(root).unwrap();
            let mut bad = invalid_names(&package);
            bad.sort();
            assert_eq!(bad, vec!["frag", "ftp", "plain", "rel", "user"]);
            assert_eq!(package.mcp_servers.len(), 3);
        });
    }

    #[test]
    fn duplicate_headers_by_case_are_rejected() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write(
                &root.join("mcp.json"),
                &format!(
                    r#"{{"$schema":"{}","mcpServers":{{"h":{{"type":"sse","url":"https://e.com","headers":{{"X-Key":"a","x-key":"b"}}}}}}}}"#,
                    AGENT_PACKAGE_MCP_SCHEMA
                ),
            );
            let package = read(root).unwrap();
            assert_eq!(invalid_names(&package), vec!["h"]);
        });
    }

    #[test]
    fn placeholder_expansion_is_single_and_not_recursive() {
        let paths = PackagePaths {
            root: PathBuf::from("/pkg/${PLUGIN_DATA}"),
            root_str: "/pkg/${PLUGIN_DATA}".to_string(),
            data: PathBuf::from("/data"),
            data_str: "/data".to_string(),
        };
        assert_eq!(
            expand_placeholders("${PLUGIN_ROOT}:${PLUGIN_DATA}:${OTHER}", &paths),
            "/pkg/${PLUGIN_DATA}:/data:${OTHER}"
        );
        assert_eq!(
            expand_placeholders("$${PLUGIN_ROOT}}", &paths),
            "$/pkg/${PLUGIN_DATA}}"
        );
    }

    #[test]
    fn cwd_forms_resolve_and_escapes_are_rejected() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            fs::create_dir_all(root.join("work")).unwrap();
            write_mcp(
                root,
                json!({
                    "dot": {"type": "stdio", "command": "node", "cwd": "./work"},
                    "root": {"type": "stdio", "command": "node", "cwd": "${PLUGIN_ROOT}/work"},
                    "data": {"type": "stdio", "command": "node", "cwd": "${PLUGIN_DATA}/cache"},
                    "up": {"type": "stdio", "command": "node", "cwd": "${PLUGIN_ROOT}/../x"},
                    "abs": {"type": "stdio", "command": "node", "cwd": "/tmp"}
                }),
            );
            let package = read(root).unwrap();
            let work = format!("{}/work", root.to_str().unwrap());
            assert_eq!(server(&package, "dot")["cwd"], work.as_str());
            assert_eq!(server(&package, "root")["cwd"], work.as_str());
            assert_eq!(
                server(&package, "data")["cwd"],
                format!("{}/cache", package.data_dir).as_str()
            );
            let mut bad = invalid_names(&package);
            bad.sort();
            assert_eq!(bad, vec!["abs", "up"]);
        });
    }

    #[test]
    fn installed_packages_are_listed_from_home() {
        let home = TempDir::new().unwrap();
        with_test_home(home.path().to_path_buf(), || {
            assert!(list_installed_agent_packages().unwrap().packages.is_empty());
            let plugins = home.path().join(".agents/plugins");
            write_plugin(&plugins.join("good"), &plugin_json("good"));
            write_skill(&plugins.join("good"), "alpha");
            fs::create_dir_all(plugins.join("empty")).unwrap();
            let listed = list_installed_agent_packages().unwrap();
            assert_eq!(listed.packages.len(), 1);
            assert_eq!(listed.packages[0].name, "good");
            assert_eq!(listed.packages[0].skill_count, 1);
            assert_eq!(listed.errors.len(), 1);
            assert!(listed.errors[0].path.ends_with("empty"));
        });
    }

    fn library_skill_dir(name: &str) -> PathBuf {
        crate::core::paths::get_library_skills_dir()
            .unwrap()
            .join(name)
    }

    fn server_file(name: &str) -> PathBuf {
        crate::core::mcp_servers::get_mcp_servers_dir()
            .unwrap()
            .join(format!("{}.json", name))
    }

    fn conflict_names(result: &AgentPackageImportResult) -> Vec<String> {
        result.conflicts.iter().map(|c| c.name.clone()).collect()
    }

    #[test]
    fn case_variant_of_an_existing_server_is_a_conflict() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_mcp(
                root,
                json!({"GitHub": {"type": "sse", "url": "https://new.example/s"}}),
            );
            crate::core::mcp_servers::save_mcp_server_config(
                "github",
                r#"{"type":"http","url":"https://keep.example/mcp"}"#,
            )
            .unwrap();
            let before = fs::read_to_string(server_file("github")).unwrap();
            let result = import(root).unwrap();
            assert_eq!(conflict_names(&result), vec!["GitHub"]);
            assert!(result.imported_mcp_servers.is_empty());
            assert_eq!(fs::read_to_string(server_file("github")).unwrap(), before);
        });
    }

    #[test]
    fn case_variant_of_the_builtin_server_and_skill_is_a_conflict() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            // Skill folders must be lowercase under the Agent Skills rule, so
            // the builtin skill can only collide by its exact name.
            write_skill(root, "automatic");
            write_mcp(
                root,
                json!({
                    "automatic": {"type": "sse", "url": "https://e.com/a"},
                    "Automatic": {"type": "sse", "url": "https://e.com/b"}
                }),
            );
            let result = import(root).unwrap();
            assert_eq!(
                conflict_names(&result),
                vec!["automatic", "Automatic", "automatic"]
            );
            assert!(result.imported_mcp_servers.is_empty());
            assert!(result.imported_skills.is_empty());
            assert!(!server_file("automatic").exists());
        });
    }

    #[test]
    fn two_case_variants_in_one_package_import_once() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_mcp(
                root,
                json!({
                    "FOO": {"type": "sse", "url": "https://e.com/upper"},
                    "foo": {"type": "sse", "url": "https://e.com/lower"}
                }),
            );
            let result = import(root).unwrap();
            assert_eq!(result.imported_mcp_servers, vec!["FOO"]);
            assert_eq!(conflict_names(&result), vec!["foo"]);
            let saved = crate::core::mcp_servers::read_mcp_server_config("FOO").unwrap();
            assert!(saved.contains("upper"));
        });
    }

    #[test]
    fn create_only_save_refuses_an_existing_file() {
        with_package(|_| {
            crate::core::mcp_servers::save_mcp_server_config(
                "srv",
                r#"{"type":"sse","url":"https://keep.example"}"#,
            )
            .unwrap();
            let err = crate::core::mcp_servers::create_mcp_server_config(
                "srv",
                r#"{"type":"sse","url":"https://new.example"}"#,
            )
            .unwrap_err();
            assert!(err.contains("already exists"));
            assert!(fs::read_to_string(server_file("srv"))
                .unwrap()
                .contains("keep.example"));
        });
    }

    #[cfg(unix)]
    #[test]
    fn symlink_in_companion_folder_marks_skill_invalid_before_writing() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_skill(root, "x");
            write(&root.join("notes.md"), "inside the package");
            fs::create_dir_all(root.join("skills/x/scripts/nested")).unwrap();
            std::os::unix::fs::symlink(
                root.join("notes.md"),
                root.join("skills/x/scripts/nested/link.md"),
            )
            .unwrap();
            let package = read(root).unwrap();
            assert_eq!(invalid_names(&package), vec!["x"]);
            let result = import(root).unwrap();
            assert!(result.imported_skills.is_empty());
            assert!(!library_skill_dir("x").exists());
        });
    }

    #[test]
    fn failed_companion_copy_leaves_no_partial_skill() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_skill(root, "x");
            write(
                &root.join("skills/x/scripts/key.md"),
                "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----",
            );
            let result = import(root).unwrap();
            assert!(result.imported_skills.is_empty());
            assert_eq!(result.invalid.len(), 1);
            assert!(!library_skill_dir("x").exists());
            // Re-importing still reports it as invalid, not as a conflict.
            let again = import(root).unwrap();
            assert!(again.conflicts.is_empty());
        });
    }

    #[cfg(unix)]
    #[test]
    fn skill_md_swapped_for_a_symlink_after_reading_is_not_imported() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_skill(root, "x");
            let package = read(root).unwrap();
            let skill_md = root.join("skills/x/SKILL.md");
            write(&root.join("other.md"), "---\nname: x\n---\nswapped\n");
            fs::remove_file(&skill_md).unwrap();
            std::os::unix::fs::symlink(root.join("other.md"), &skill_md).unwrap();
            let skill = &package.skills[0];
            assert!(import_skill_files(root, skill).is_err());
            assert!(!library_skill_dir("x").exists());
        });
    }

    #[test]
    fn oversized_manifests_are_refused() {
        with_package(|root| {
            let mut manifest = plugin_json("demo");
            manifest["description"] = json!("a".repeat(MAX_MANIFEST_BYTES as usize));
            write_plugin(root, &manifest);
            assert!(read(root).unwrap_err().contains("too large"));
            write_plugin(root, &plugin_json("demo"));
            let big = "x".repeat(MAX_MANIFEST_BYTES as usize);
            write_mcp(
                root,
                json!({"r": {"type": "sse", "url": "https://e.com/s", "headers": {"X-Pad": big}}}),
            );
            assert!(read(root)
                .unwrap()
                .warnings
                .iter()
                .any(|w| w.contains("too large")));
        });
    }

    #[test]
    fn env_names_and_header_names_and_values_are_checked() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_mcp(
                root,
                json!({
                    "empty": {"type": "stdio", "command": "node", "env": {"": "x"}},
                    "equals": {"type": "stdio", "command": "node", "env": {"A=B": "x"}},
                    "newline": {"type": "stdio", "command": "node", "env": {"A\nB": "x"}},
                    "digit": {"type": "stdio", "command": "node", "env": {"1A": "x"}},
                    "goodenv": {"type": "stdio", "command": "node", "env": {"_A1": "x"}},
                    "crlf": {"type": "sse", "url": "https://e.com", "headers": {"X-A": "a\r\nX-B: b"}},
                    "space": {"type": "sse", "url": "https://e.com", "headers": {"X A": "a"}},
                    "colon": {"type": "sse", "url": "https://e.com", "headers": {"X:A": "a"}},
                    "goodhdr": {"type": "sse", "url": "https://e.com", "headers": {"X-Api_Key": "a\tb"}}
                }),
            );
            let package = read(root).unwrap();
            let mut bad = invalid_names(&package);
            bad.sort();
            assert_eq!(
                bad,
                vec!["colon", "crlf", "digit", "empty", "equals", "newline", "space"]
            );
            assert_eq!(package.mcp_servers.len(), 2);
        });
    }

    #[test]
    fn foreign_placeholders_skip_the_entry() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_mcp(
                root,
                json!({
                    "arg": {"type": "stdio", "command": "node", "args": ["${workspaceFolder}"]},
                    "env": {"type": "stdio", "command": "node", "env": {"A": "${env:HOME}"}},
                    "home": {"type": "stdio", "command": "node", "cwd": "${userHome}"},
                    "hdr": {"type": "sse", "url": "https://e.com", "headers": {"A": "Bearer ${TOKEN}"}},
                    "ok": {"type": "stdio", "command": "node", "args": ["${PLUGIN_ROOT}", "$HOME", "{x}"]}
                }),
            );
            let package = read(root).unwrap();
            let mut bad = invalid_names(&package);
            bad.sort();
            assert_eq!(bad, vec!["arg", "env", "hdr", "home"]);
            assert_eq!(server(&package, "ok")["args"][1], "$HOME");
        });
    }

    #[test]
    fn dot_names_and_placeholders_are_not_commands() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_mcp(
                root,
                json!({
                    "dot": {"type": "stdio", "command": "."},
                    "dotdot": {"type": "stdio", "command": ".."},
                    "ph": {"type": "stdio", "command": "${PLUGIN_ROOT}"}
                }),
            );
            let package = read(root).unwrap();
            let mut bad = invalid_names(&package);
            bad.sort();
            assert_eq!(bad, vec!["dot", "dotdot", "ph"]);
        });
    }

    #[test]
    fn plugin_data_cwd_cannot_escape() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_mcp(
                root,
                json!({"up": {"type": "stdio", "command": "node", "cwd": "${PLUGIN_DATA}/../x"}}),
            );
            assert_eq!(invalid_names(&read(root).unwrap()), vec!["up"]);
        });
    }

    #[cfg(unix)]
    #[test]
    fn manifests_symlinked_outside_the_package_are_refused() {
        with_package(|root| {
            let outside = TempDir::new().unwrap();
            write(
                &outside.path().join("plugin.json"),
                &plugin_json("demo").to_string(),
            );
            std::os::unix::fs::symlink(
                outside.path().join("plugin.json"),
                root.join("plugin.json"),
            )
            .unwrap();
            assert!(read(root).unwrap_err().contains("outside"));

            fs::remove_file(root.join("plugin.json")).unwrap();
            write_plugin(root, &plugin_json("demo"));
            let mcp = json!({"$schema": AGENT_PACKAGE_MCP_SCHEMA, "mcpServers": {}});
            write(&outside.path().join("mcp.json"), &mcp.to_string());
            std::os::unix::fs::symlink(outside.path().join("mcp.json"), root.join("mcp.json"))
                .unwrap();
            let package = read(root).unwrap();
            assert!(package.warnings.iter().any(|w| w.contains("outside")));
        });
    }

    #[test]
    fn full_author_object_is_accepted() {
        with_package(|root| {
            let mut manifest = plugin_json("demo");
            manifest["author"] =
                json!({"name": "A", "email": "a@example.com", "url": "https://a.example"});
            manifest["keywords"] = json!(["x", "y"]);
            write_plugin(root, &manifest);
            assert!(read(root).is_ok());
        });
    }

    /// True when the file system under `dir` treats `a` and `b` as one name.
    fn names_collide(dir: &Path, a: &str, b: &str) -> bool {
        let probe = dir.join(format!("probe-{}", a));
        fs::create_dir_all(&probe).unwrap();
        let collide = dir.join(format!("probe-{}", b)).exists();
        fs::remove_dir_all(&probe).unwrap();
        collide
    }

    fn save_project_named(name: &str, servers: &[&str], skills: &[&str]) {
        let dir = crate::core::paths::home_dir()
            .unwrap()
            .join(format!("proj-{}", name));
        fs::create_dir_all(&dir).unwrap();
        let project = crate::core::Project {
            name: name.to_string(),
            directory: dir.to_str().unwrap().to_string(),
            mcp_servers: servers.iter().map(|s| s.to_string()).collect(),
            skills: skills.iter().map(|s| s.to_string()).collect(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            ..Default::default()
        };
        crate::core::save_project(name, &serde_json::to_string(&project).unwrap()).unwrap();
    }

    #[test]
    fn non_ascii_server_names_are_invalid() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_mcp(
                root,
                json!({
                    "äbc": {"type": "sse", "url": "https://e.com/a"},
                    "caf\u{e9}": {"type": "sse", "url": "https://e.com/b"},
                    "po\u{17f}tgres": {"type": "sse", "url": "https://e.com/c"},
                    "ok.name_1-x": {"type": "sse", "url": "https://e.com/d"}
                }),
            );
            let package = read(root).unwrap();
            let names: Vec<&str> = package
                .mcp_servers
                .iter()
                .map(|s| s.name.as_str())
                .collect();
            assert_eq!(names, vec!["ok.name_1-x"]);
            let mut invalid = invalid_names(&package);
            invalid.sort();
            let mut expected = vec!["äbc", "caf\u{e9}", "po\u{17f}tgres"];
            expected.sort();
            assert_eq!(invalid, expected);
        });
    }

    #[test]
    fn skill_write_is_create_only_on_the_file_system() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_skill(root, "abc");
            crate::core::skills::save_skill("Abc", "---\nname: Abc\n---\noriginal\n").unwrap();
            let library = crate::core::paths::get_library_skills_dir().unwrap();
            let insensitive = names_collide(&library, "Abc", "abc");
            // Bypasses the plan to reach the write itself.
            let skill = read(root).unwrap().skills.remove(0);
            let outcome = import_skill_files(root, &skill);
            if insensitive {
                assert!(matches!(outcome, Err(SkillWriteError::Conflict)));
                assert!(crate::core::skills::read_skill_raw("Abc")
                    .unwrap()
                    .contains("original"));
            } else {
                assert!(outcome.is_ok());
            }
        });
    }

    #[test]
    fn skill_names_must_follow_the_agent_skills_rule() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            for name in ["Upper", "äbc", "a--b", "-a", "a-", "a_b"] {
                write_skill(root, name);
            }
            write_skill(root, "good-name-2");
            let package = read(root).unwrap();
            let names: Vec<&str> = package.skills.iter().map(|s| s.name.as_str()).collect();
            assert_eq!(names, vec!["good-name-2"]);
            let mut invalid = invalid_names(&package);
            invalid.sort();
            let mut expected = vec!["-a", "Upper", "a-", "a--b", "a_b", "äbc"];
            expected.sort();
            assert_eq!(invalid, expected);
        });
    }

    #[test]
    fn project_reference_differing_only_in_case_is_a_conflict() {
        with_package(|root| {
            save_project_named("site", &["github"], &["Alpha"]);
            write_plugin(root, &plugin_json("demo"));
            write_skill(root, "alpha");
            write_mcp(
                root,
                json!({"GitHub": {"type": "sse", "url": "https://e.com/s"}}),
            );
            let result = import(root).unwrap();
            assert_eq!(conflict_names(&result), vec!["alpha", "GitHub"]);
            assert!(result
                .conflicts
                .iter()
                .all(|c| c.reason.contains("project site")));
            assert!(result.imported_mcp_servers.is_empty() && result.imported_skills.is_empty());
        });
    }

    #[test]
    fn preview_plans_conflicts_and_lists_projects_only_for_new_items() {
        with_package(|root| {
            save_project_named("site", &["remote", "fresh"], &[]);
            crate::core::mcp_servers::save_mcp_server_config(
                "remote",
                r#"{"type":"http","url":"https://keep.example/mcp"}"#,
            )
            .unwrap();
            write_plugin(root, &plugin_json("demo"));
            write_mcp(
                root,
                json!({
                    "remote": {"type": "sse", "url": "https://e.com/a"},
                    "fresh": {"type": "sse", "url": "https://e.com/b"}
                }),
            );
            let preview = preview_agent_package(root.to_str().unwrap()).unwrap();
            assert_eq!(
                preview.plan.mcp_servers,
                vec![AgentPackagePlannedItem {
                    name: "fresh".into(),
                    projects: vec!["site".into()]
                }]
            );
            let conflicts: Vec<_> = preview
                .plan
                .conflicts
                .iter()
                .map(|c| c.name.as_str())
                .collect();
            assert_eq!(conflicts, vec!["remote"]);
            let result =
                import_agent_package(root.to_str().unwrap(), &preview.package.digest).unwrap();
            assert_eq!(result.imported_mcp_servers, vec!["fresh"]);
            assert_eq!(conflict_names(&result), vec!["remote"]);
        });
    }

    #[test]
    fn failed_skill_import_removes_its_scan_record() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_skill(root, "x");
            write(
                &root.join("skills/x/scripts/key.md"),
                "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----",
            );
            let result = import(root).unwrap();
            assert_eq!(result.invalid.len(), 1);
            assert!(crate::core::skills::get_skill_scan_state("x")
                .unwrap()
                .is_none());
        });
    }

    #[test]
    fn a_change_after_the_preview_is_refused() {
        with_package(|root| {
            write_plugin(root, &plugin_json("demo"));
            write_skill(root, "alpha");
            write_mcp(
                root,
                json!({"r": {"type": "sse", "url": "https://e.com/s"}}),
            );
            let digest = read(root).unwrap().digest;

            write(
                &root.join("skills/alpha/SKILL.md"),
                "---\nname: alpha\n---\nchanged\n",
            );
            let err = import_agent_package(root.to_str().unwrap(), &digest).unwrap_err();
            assert!(err.contains("changed after the preview"));

            let digest = read(root).unwrap().digest;
            write_mcp(
                root,
                json!({"r": {"type": "sse", "url": "https://other.example/s"}}),
            );
            assert!(import_agent_package(root.to_str().unwrap(), &digest).is_err());

            let digest = read(root).unwrap().digest;
            assert_eq!(read(root).unwrap().digest, digest, "the digest is stable");
            assert!(import_agent_package(root.to_str().unwrap(), &digest).is_ok());
        });
    }
}
