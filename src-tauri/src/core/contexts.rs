use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::*;

// ── Contexts ─────────────────────────────────────────────────────────────────
//
// A context is a named collection of sources that agents read on demand
// through the MCP server. Nothing from a context is written into a project.
//
// Storage:
//   ~/.automatic/library/contexts/{slug}.json                    the context
//   ~/.automatic/library/contexts/{slug}/sources/{id}/{path}.md  documentation pages
//
// A context is either local (its sources are defined here) or cloud (a
// pointer to a webapp context, read through the webapp API). The source
// shape (`id`, `kind`, `config`) matches the webapp's so sources can later
// become separate, reusable entities without a data migration.
//
// Projects attach contexts by slug in `Project::contexts`. Groups attach them
// in `ProjectGroup::contexts`, and `reconcile_group_contexts` copies a
// group's contexts into each member project with provenance, the same way
// profiles materialise their entries.

/// Slug grammar shared with the webapp's library and context slugs.
pub const CONTEXT_SLUG_MAX_LENGTH: usize = 128;
pub const CONTEXT_DISPLAY_NAME_MAX_LENGTH: usize = 128;
pub const CONTEXT_DESCRIPTION_MAX_LENGTH: usize = 2000;

/// Documentation page limits, matching the webapp's OKF document limits so a
/// local documentation source can later be pushed to the cloud unchanged.
pub const DOCUMENT_PATH_MAX_LENGTH: usize = 512;
pub const DOCUMENT_PATH_MAX_SEGMENTS: usize = 10;
pub const DOCUMENT_MAX_BYTES: usize = 512_000;
pub const DOCUMENTS_PER_SOURCE_MAX: usize = 2000;

const CLOUD_CONTEXT_ID_PREFIX: &str = "ctx_";
const CLOUD_SOURCE_ID_PREFIX: &str = "src_";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Context {
    pub slug: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(flatten)]
    pub body: ContextBody,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

/// Where a context's sources are defined.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "location", rename_all = "snake_case")]
pub enum ContextBody {
    Local {
        #[serde(default)]
        sources: Vec<ContextSource>,
    },
    /// A webapp context. Its sources live in the cloud and are listed and
    /// read through the webapp API with the signed-in account.
    Cloud { context_id: String },
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ContextSource {
    /// Unique within the context. Uses the slug grammar.
    pub id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(flatten)]
    pub spec: SourceSpec,
}

/// A source's kind and its kind-specific configuration. Serialised as
/// `{"kind": "...", "config": {...}}`, the webapp's source shape.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "kind", content = "config", rename_all = "snake_case")]
pub enum SourceSpec {
    /// Markdown pages stored by Automatic under the context's directory.
    Documentation,
    /// A file or directory on this machine, read when an agent asks for it.
    Local(LocalSourceConfig),
    /// An HTTP(S) resource fetched on demand and cached for `ttl_secs`.
    Url(UrlSourceConfig),
    /// A webapp source, read through the webapp API.
    Cloud(CloudSourceConfig),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct LocalSourceConfig {
    /// Absolute path to a file or directory.
    pub path: String,
    /// Glob patterns, relative to `path`, selecting which files in a
    /// directory are exposed. Empty means the default text-document set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct UrlSourceConfig {
    pub url: String,
    /// Seconds a fetched copy stays fresh. `0` fetches on every read.
    #[serde(default = "default_url_ttl_secs")]
    pub ttl_secs: u64,
}

pub const DEFAULT_URL_TTL_SECS: u64 = 3600;

fn default_url_ttl_secs() -> u64 {
    DEFAULT_URL_TTL_SECS
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CloudSourceConfig {
    /// Webapp source id (`src_...`).
    pub source_id: String,
}

impl Context {
    /// The context's sources, or `None` for a cloud context, whose sources
    /// are only known to the webapp.
    pub fn local_sources(&self) -> Option<&[ContextSource]> {
        match &self.body {
            ContextBody::Local { sources } => Some(sources),
            ContextBody::Cloud { .. } => None,
        }
    }

    pub fn find_source(&self, source_id: &str) -> Option<&ContextSource> {
        self.local_sources()?.iter().find(|s| s.id == source_id)
    }
}

// ── Validation ───────────────────────────────────────────────────────────────

/// `^[a-z0-9][a-z0-9._-]*$`, at most 128 characters. The leading
/// alphanumeric rules out `.` and `..`, so a valid slug is always a safe
/// file-name component.
pub fn is_valid_context_slug(slug: &str) -> bool {
    let mut chars = slug.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    slug.len() <= CONTEXT_SLUG_MAX_LENGTH
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' || c == '-'
        })
}

fn is_valid_cloud_id(id: &str, prefix: &str) -> bool {
    id.len() > prefix.len()
        && id.len() <= 128
        && id.starts_with(prefix)
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub fn validate_context(context: &Context) -> Result<(), String> {
    if !is_valid_context_slug(&context.slug) {
        return Err(format!(
            "Invalid context slug '{}': use lowercase letters, digits, '.', '_' and '-', \
             starting with a letter or digit, at most {} characters",
            context.slug, CONTEXT_SLUG_MAX_LENGTH
        ));
    }
    if context.display_name.chars().count() > CONTEXT_DISPLAY_NAME_MAX_LENGTH {
        return Err(format!(
            "Context '{}': display name is longer than {} characters",
            context.slug, CONTEXT_DISPLAY_NAME_MAX_LENGTH
        ));
    }
    if context.description.chars().count() > CONTEXT_DESCRIPTION_MAX_LENGTH {
        return Err(format!(
            "Context '{}': description is longer than {} characters",
            context.slug, CONTEXT_DESCRIPTION_MAX_LENGTH
        ));
    }
    match &context.body {
        ContextBody::Cloud { context_id } => {
            if !is_valid_cloud_id(context_id, CLOUD_CONTEXT_ID_PREFIX) {
                return Err(format!(
                    "Context '{}': cloud context id '{}' must look like 'ctx_...'",
                    context.slug, context_id
                ));
            }
        }
        ContextBody::Local { sources } => {
            let mut seen = std::collections::HashSet::new();
            for source in sources {
                if !seen.insert(source.id.as_str()) {
                    return Err(format!(
                        "Context '{}': duplicate source id '{}'",
                        context.slug, source.id
                    ));
                }
                validate_source(source)
                    .map_err(|e| format!("Context '{}': {}", context.slug, e))?;
            }
        }
    }
    Ok(())
}

fn validate_source(source: &ContextSource) -> Result<(), String> {
    if !is_valid_context_slug(&source.id) {
        return Err(format!(
            "invalid source id '{}': use the same rules as a context slug",
            source.id
        ));
    }
    if source.display_name.chars().count() > CONTEXT_DISPLAY_NAME_MAX_LENGTH {
        return Err(format!(
            "source '{}': display name is longer than {} characters",
            source.id, CONTEXT_DISPLAY_NAME_MAX_LENGTH
        ));
    }
    match &source.spec {
        SourceSpec::Documentation => Ok(()),
        SourceSpec::Local(config) => validate_local_config(&source.id, config),
        SourceSpec::Url(config) => validate_url_config(&source.id, config),
        SourceSpec::Cloud(config) => {
            if is_valid_cloud_id(&config.source_id, CLOUD_SOURCE_ID_PREFIX) {
                Ok(())
            } else {
                Err(format!(
                    "source '{}': cloud source id '{}' must look like 'src_...'",
                    source.id, config.source_id
                ))
            }
        }
    }
}

fn validate_local_config(source_id: &str, config: &LocalSourceConfig) -> Result<(), String> {
    if !Path::new(&config.path).is_absolute() {
        return Err(format!(
            "source '{}': local path '{}' must be absolute",
            source_id, config.path
        ));
    }
    for pattern in &config.include {
        glob::Pattern::new(pattern).map_err(|e| {
            format!(
                "source '{}': invalid include pattern '{}': {}",
                source_id, pattern, e
            )
        })?;
    }
    Ok(())
}

fn validate_url_config(source_id: &str, config: &UrlSourceConfig) -> Result<(), String> {
    let parsed = url::Url::parse(&config.url)
        .map_err(|e| format!("source '{}': invalid URL '{}': {}", source_id, config.url, e))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(format!(
            "source '{}': URL must use http or https, not '{}'",
            source_id,
            parsed.scheme()
        ));
    }
    // Context files are plain JSON in the library, so a credential in the
    // URL would sit on disk in clear text. Authenticated URLs need a
    // keychain-backed design first.
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(format!(
            "source '{}': URLs with embedded credentials are not supported",
            source_id
        ));
    }
    Ok(())
}

/// Check a documentation page path against the webapp's OKF path rules:
/// relative, `/`-separated, ending in `.md`, each segment starting with an
/// alphanumeric and using only alphanumerics, `.`, `_` and `-`.
pub fn validate_document_path(path: &str) -> Result<(), String> {
    if path.is_empty() || path.len() > DOCUMENT_PATH_MAX_LENGTH {
        return Err(format!(
            "Document path must be 1 to {} characters",
            DOCUMENT_PATH_MAX_LENGTH
        ));
    }
    if !path.ends_with(".md") {
        return Err(format!("Document path '{}' must end in .md", path));
    }
    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() > DOCUMENT_PATH_MAX_SEGMENTS {
        return Err(format!(
            "Document path '{}' has more than {} segments",
            path, DOCUMENT_PATH_MAX_SEGMENTS
        ));
    }
    for segment in segments {
        let mut chars = segment.chars();
        let valid = chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-');
        if !valid || segment.contains("..") {
            return Err(format!(
                "Document path '{}' has an invalid segment '{}'",
                path, segment
            ));
        }
    }
    Ok(())
}

// ── Storage ──────────────────────────────────────────────────────────────────

pub fn get_contexts_dir() -> Result<PathBuf, String> {
    Ok(get_library_dir()?.join("contexts"))
}

fn context_file(dir: &Path, slug: &str) -> PathBuf {
    dir.join(format!("{}.json", slug))
}

/// Directory holding everything a context stores besides its JSON file.
fn context_data_dir(dir: &Path, slug: &str) -> PathBuf {
    dir.join(slug)
}

fn documentation_dir(dir: &Path, slug: &str, source_id: &str) -> PathBuf {
    context_data_dir(dir, slug).join("sources").join(source_id)
}

pub fn list_contexts() -> Result<Vec<String>, String> {
    let dir = get_contexts_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(&dir)
        .map_err(|e| format!("Failed to read contexts dir {}: {}", dir.display(), e))?;
    let mut slugs: Vec<String> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| path.file_stem().and_then(|s| s.to_str()).map(String::from))
        .filter(|stem| is_valid_context_slug(stem))
        .collect();
    slugs.sort();
    Ok(slugs)
}

pub fn context_exists(slug: &str) -> Result<bool, String> {
    if !is_valid_context_slug(slug) {
        return Ok(false);
    }
    Ok(context_file(&get_contexts_dir()?, slug).is_file())
}

pub fn read_context(slug: &str) -> Result<Context, String> {
    if !is_valid_context_slug(slug) {
        return Err(format!("Invalid context slug '{}'", slug));
    }
    let path = context_file(&get_contexts_dir()?, slug);
    if !path.is_file() {
        return Err(format!("Context '{}' not found", slug));
    }
    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read context '{}': {}", slug, e))?;
    let context: Context = serde_json::from_str(&raw)
        .map_err(|e| format!("Context '{}' is not valid: {}", slug, e))?;
    if context.slug != slug {
        return Err(format!(
            "Context file '{}.json' declares slug '{}'",
            slug, context.slug
        ));
    }
    Ok(context)
}

/// Validate and write a context. Sets `updated_at`, and `created_at` when it
/// is empty. Documentation pages for sources the context no longer lists
/// are left on disk; `delete_context` removes them with the context.
pub fn save_context(mut context: Context) -> Result<Context, String> {
    validate_context(&context)?;
    let now = chrono::Utc::now().to_rfc3339();
    if context.created_at.is_empty() {
        context.created_at = now.clone();
    }
    context.updated_at = now;

    let dir = get_contexts_dir()?;
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create contexts dir {}: {}", dir.display(), e))?;
    let pretty = serde_json::to_string_pretty(&context).map_err(|e| e.to_string())?;
    fs::write(context_file(&dir, &context.slug), pretty)
        .map_err(|e| format!("Failed to write context '{}': {}", context.slug, e))?;
    Ok(context)
}

/// Delete a context, its stored documentation pages, and every reference to
/// it from projects and groups. Returns the projects and groups that
/// changed.
pub fn delete_context(slug: &str) -> Result<ContextReferences, String> {
    if !is_valid_context_slug(slug) {
        return Err(format!("Invalid context slug '{}'", slug));
    }
    let dir = get_contexts_dir()?;
    let file = context_file(&dir, slug);
    if file.exists() {
        fs::remove_file(&file)
            .map_err(|e| format!("Failed to delete context '{}': {}", slug, e))?;
    }
    let data = context_data_dir(&dir, slug);
    if data.exists() {
        fs::remove_dir_all(&data)
            .map_err(|e| format!("Failed to delete stored pages for '{}': {}", slug, e))?;
    }
    rewrite_context_references(slug, None)
}

/// Rename a context's slug, move its stored pages, and update every project
/// and group that references it.
pub fn rename_context(old_slug: &str, new_slug: &str) -> Result<ContextReferences, String> {
    if old_slug == new_slug {
        return Ok(ContextReferences::default());
    }
    if !is_valid_context_slug(new_slug) {
        return Err(format!("Invalid context slug '{}'", new_slug));
    }
    let mut context = read_context(old_slug)?;
    let dir = get_contexts_dir()?;
    if context_file(&dir, new_slug).exists() || context_data_dir(&dir, new_slug).exists() {
        return Err(format!("A context named '{}' already exists", new_slug));
    }

    let old_data = context_data_dir(&dir, old_slug);
    if old_data.exists() {
        fs::rename(&old_data, context_data_dir(&dir, new_slug))
            .map_err(|e| format!("Failed to move stored pages for '{}': {}", old_slug, e))?;
    }
    context.slug = new_slug.to_string();
    save_context(context)?;
    fs::remove_file(context_file(&dir, old_slug))
        .map_err(|e| format!("Failed to remove old context file '{}': {}", old_slug, e))?;
    rewrite_context_references(old_slug, Some(new_slug))
}

// ── Documentation pages ──────────────────────────────────────────────────────

fn documentation_source_dir(slug: &str, source_id: &str) -> Result<PathBuf, String> {
    let context = read_context(slug)?;
    match context.find_source(source_id).map(|s| &s.spec) {
        Some(SourceSpec::Documentation) => {
            Ok(documentation_dir(&get_contexts_dir()?, slug, source_id))
        }
        Some(_) => Err(format!(
            "Source '{}' in context '{}' is not a documentation source",
            source_id, slug
        )),
        None => Err(format!(
            "Context '{}' has no source '{}'",
            slug, source_id
        )),
    }
}

/// Every page path in a documentation source, sorted.
pub fn list_documentation_pages(slug: &str, source_id: &str) -> Result<Vec<String>, String> {
    let root = documentation_source_dir(slug, source_id)?;
    list_pages_in(&root)
}

fn list_pages_in(root: &Path) -> Result<Vec<String>, String> {
    let mut pages = Vec::new();
    if root.is_dir() {
        collect_pages(root, root, 0, &mut pages)?;
    }
    pages.sort();
    Ok(pages)
}

fn collect_pages(root: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) -> Result<(), String> {
    if depth >= DOCUMENT_PATH_MAX_SEGMENTS {
        return Ok(());
    }
    let entries =
        fs::read_dir(dir).map_err(|e| format!("Failed to read {}: {}", dir.display(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect_pages(root, &path, depth + 1, out)?;
        } else if file_type.is_file() {
            let Some(relative) = relative_slash_path(root, &path) else {
                continue;
            };
            // Files dropped in by hand that break the path rules are not
            // pages; listing them would offer paths that reads then refuse.
            if validate_document_path(&relative).is_ok() {
                out.push(relative);
            }
        }
    }
    Ok(())
}

fn relative_slash_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let parts: Option<Vec<&str>> = relative
        .components()
        .map(|c| match c {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect();
    Some(parts?.join("/"))
}

pub fn read_documentation_page(slug: &str, source_id: &str, path: &str) -> Result<String, String> {
    validate_document_path(path)?;
    let file = documentation_source_dir(slug, source_id)?.join(path);
    if !file.is_file() {
        return Err(format!(
            "Page '{}' not found in source '{}' of context '{}'",
            path, source_id, slug
        ));
    }
    fs::read_to_string(&file).map_err(|e| format!("Failed to read page '{}': {}", path, e))
}

pub fn write_documentation_page(
    slug: &str,
    source_id: &str,
    path: &str,
    content: &str,
) -> Result<(), String> {
    validate_document_path(path)?;
    if content.len() > DOCUMENT_MAX_BYTES {
        return Err(format!(
            "Page '{}' is {} bytes; the limit is {}",
            path,
            content.len(),
            DOCUMENT_MAX_BYTES
        ));
    }
    let root = documentation_source_dir(slug, source_id)?;
    let file = root.join(path);
    if !file.exists() && list_pages_in(&root)?.len() >= DOCUMENTS_PER_SOURCE_MAX {
        return Err(format!(
            "Source '{}' already holds the maximum of {} pages",
            source_id, DOCUMENTS_PER_SOURCE_MAX
        ));
    }
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
    }
    fs::write(&file, content).map_err(|e| format!("Failed to write page '{}': {}", path, e))
}

pub fn delete_documentation_page(slug: &str, source_id: &str, path: &str) -> Result<(), String> {
    validate_document_path(path)?;
    let root = documentation_source_dir(slug, source_id)?;
    let file = root.join(path);
    if file.exists() {
        fs::remove_file(&file).map_err(|e| format!("Failed to delete page '{}': {}", path, e))?;
    }
    remove_empty_folders(&root, &file);
    Ok(())
}

/// Move or rename a page within one documentation source. Folders exist
/// only while they hold a page, so missing folders at `to` are created and
/// folders left empty at `from` are removed. Refuses to overwrite a page.
pub fn move_documentation_page(slug: &str, source_id: &str, from: &str, to: &str) -> Result<(), String> {
    validate_document_path(from)?;
    validate_document_path(to)?;
    if from == to {
        return Ok(());
    }
    let root = documentation_source_dir(slug, source_id)?;
    let source = root.join(from);
    let target = root.join(to);
    if !source.is_file() {
        return Err(format!("Page '{}' not found in source '{}'", from, source_id));
    }
    if target.exists() {
        return Err(format!("A page already exists at '{}'", to));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
    }
    fs::rename(&source, &target)
        .map_err(|e| format!("Failed to move page '{}' to '{}': {}", from, to, e))?;
    remove_empty_folders(&root, &source);
    Ok(())
}

/// Remove the now-empty folders between `removed`'s parent and `root`.
/// Best effort: a folder that still holds anything, or cannot be removed,
/// stops the walk and is left in place.
fn remove_empty_folders(root: &Path, removed: &Path) {
    let mut dir = removed.parent();
    while let Some(current) = dir {
        if current == root || !current.starts_with(root) {
            break;
        }
        if fs::remove_dir(current).is_err() {
            break;
        }
        dir = current.parent();
    }
}

// ── Agent-facing writes ──────────────────────────────────────────────────────
//
// Agents create contexts and write pages through MCP by title and folder,
// the way the app's UI does. They never add linked material (folders,
// files, URLs, cloud sources): which paths and addresses get read stays the
// user's choice.

/// The id of the pages source a new context gets.
pub const PAGES_SOURCE_ID: &str = "pages";

/// Turn a title into a page path segment (`Setup guide` → `setup-guide`).
/// Mirrors `segmentFromTitle` in the frontend's `pageTree.ts`.
pub fn segment_from_title(title: &str) -> String {
    let lower = title.to_lowercase();
    let mut out = String::new();
    let mut in_gap = false;
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' || c == '-' {
            if in_gap && !out.is_empty() {
                out.push('-');
            }
            in_gap = false;
            out.push(c);
        } else {
            in_gap = true;
        }
    }
    let trimmed = out.trim_start_matches(|c: char| !c.is_ascii_alphanumeric());
    let mut segment = trimmed.trim_end_matches('-').to_string();
    while segment.contains("..") {
        segment = segment.replace("..", ".");
    }
    let segment: String = segment.chars().take(64).collect();
    let segment = segment.trim_end_matches('-').to_string();
    if segment.is_empty() {
        "untitled".to_string()
    } else {
        segment
    }
}

/// `folder/segment.md` for a title. `folder` may use display names
/// ("Guides/Troubleshooting"); each part becomes a segment.
pub fn page_path_for(folder: Option<&str>, title: &str) -> String {
    let mut parts: Vec<String> = folder
        .unwrap_or("")
        .split('/')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(segment_from_title)
        .collect();
    parts.push(format!("{}.md", segment_from_title(title)));
    parts.join("/")
}

/// Frontmatter the webapp expects, plus a body, for a new page.
pub fn new_page_content(title: &str, body: &str) -> String {
    let title = title.replace(['\r', '\n'], " ");
    let title = title.trim();
    let quoted = serde_json::to_string(if title.is_empty() { "Untitled" } else { title })
        .unwrap_or_else(|_| "\"Untitled\"".to_string());
    format!("---\ntype: page\ntitle: {}\n---\n\n{}", quoted, body)
}

/// Split `---\n...\n---\n` frontmatter from the rest of a page.
fn split_frontmatter(content: &str) -> (&str, &str) {
    if let Some(rest) = content.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            let split = 4 + end + 5;
            return (&content[..split], &content[split..]);
        }
    }
    ("", content)
}

/// Create a local context from a name, with a pages source ready. The slug
/// is derived from the name and made unique.
pub fn create_local_context(name: &str, description: &str) -> Result<Context, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the context a name".to_string());
    }
    let existing = list_contexts()?;
    let derived = segment_from_title(name).replace('.', "-");
    let base = if is_valid_context_slug(&derived) && derived != "untitled" {
        derived
    } else {
        "context".to_string()
    };
    let mut slug = base.clone();
    let mut i = 2;
    while existing.contains(&slug) {
        slug = format!("{}-{}", base, i);
        i += 1;
    }
    save_context(Context {
        slug,
        display_name: name.to_string(),
        description: description.trim().to_string(),
        body: ContextBody::Local {
            sources: vec![ContextSource {
                id: PAGES_SOURCE_ID.to_string(),
                display_name: String::new(),
                description: String::new(),
                spec: SourceSpec::Documentation,
            }],
        },
        created_at: String::new(),
        updated_at: String::new(),
    })
}

/// The context's pages source, adding one when it has none. Cloud contexts
/// have no local pages.
pub fn ensure_pages_source(slug: &str) -> Result<String, String> {
    let mut context = read_context(slug)?;
    let ContextBody::Local { sources } = &mut context.body else {
        return Err(format!(
            "Context '{}' lives in the Automatic cloud; edit its pages in the web app",
            slug
        ));
    };
    if let Some(existing) = sources.iter().find(|s| s.spec == SourceSpec::Documentation) {
        return Ok(existing.id.clone());
    }
    let mut id = PAGES_SOURCE_ID.to_string();
    let mut i = 2;
    while sources.iter().any(|s| s.id == id) {
        id = format!("{}-{}", PAGES_SOURCE_ID, i);
        i += 1;
    }
    sources.push(ContextSource {
        id: id.clone(),
        display_name: String::new(),
        description: String::new(),
        spec: SourceSpec::Documentation,
    });
    save_context(context)?;
    Ok(id)
}

/// Where a page write goes: an existing path, or a title in a folder.
pub enum PageTarget<'a> {
    Path(&'a str),
    Title { folder: Option<&'a str>, title: &'a str },
}

/// Create or replace a page and return its path. `body` is Markdown; the
/// webapp frontmatter is added for new pages and kept for existing ones,
/// unless `body` brings its own.
pub fn write_context_page(slug: &str, target: PageTarget, body: &str) -> Result<String, String> {
    let source_id = ensure_pages_source(slug)?;
    let (path, title) = match target {
        PageTarget::Path(path) => {
            let stem = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".md");
            (path.to_string(), stem.replace(['-', '_'], " "))
        }
        PageTarget::Title { folder, title } => (page_path_for(folder, title), title.to_string()),
    };
    validate_document_path(&path)?;
    let content = if body.starts_with("---\n") {
        body.to_string()
    } else {
        match read_documentation_page(slug, &source_id, &path) {
            Ok(existing) => {
                let (frontmatter, _) = split_frontmatter(&existing);
                if frontmatter.is_empty() {
                    body.to_string()
                } else {
                    format!("{}\n{}", frontmatter, body)
                }
            }
            Err(_) => new_page_content(&title, body),
        }
    };
    write_documentation_page(slug, &source_id, &path, &content)?;
    Ok(path)
}

/// Move or rename a page within the context's pages. Returns the new path.
pub fn move_context_page(
    slug: &str,
    from: &str,
    to_folder: Option<&str>,
    to_title: Option<&str>,
) -> Result<String, String> {
    let source_id = ensure_pages_source(slug)?;
    let stem = from.rsplit('/').next().unwrap_or(from).trim_end_matches(".md");
    let current_folder = from.rsplit_once('/').map(|(f, _)| f).unwrap_or("");
    let folder = to_folder.unwrap_or(current_folder);
    let to = page_path_for(Some(folder), to_title.unwrap_or(stem));
    move_documentation_page(slug, &source_id, from, &to)?;
    Ok(to)
}

pub fn delete_context_page(slug: &str, path: &str) -> Result<(), String> {
    let source_id = ensure_pages_source(slug)?;
    if read_documentation_page(slug, &source_id, path).is_err() {
        return Err(format!("Context '{}' has no page '{}'", slug, path));
    }
    delete_documentation_page(slug, &source_id, path)
}

// ── Attachment ───────────────────────────────────────────────────────────────

/// Attach a context to a project's own list. Returns `true` when the list
/// changed.
pub fn attach_context_to_project(project: &mut Project, slug: &str) -> bool {
    if project.contexts.iter().any(|c| c == slug) {
        return false;
    }
    project.contexts.push(slug.to_string());
    true
}

/// Detach a context from a project. Refused while a group provides it,
/// because the next reconcile would put it back. Returns `true` when the
/// list changed.
pub fn detach_context_from_project(project: &mut Project, slug: &str) -> Result<bool, String> {
    if let Some(group) = group_providing(project, slug) {
        return Err(format!(
            "Context '{}' is provided by group '{}'. Detach it from the group instead.",
            slug, group
        ));
    }
    let before = project.contexts.len();
    project.contexts.retain(|c| c != slug);
    Ok(project.contexts.len() != before)
}

pub fn group_providing(project: &Project, slug: &str) -> Option<String> {
    let mut groups: Vec<&String> = project
        .group_context_contributions
        .iter()
        .filter(|(_, slugs)| slugs.iter().any(|s| s == slug))
        .map(|(group, _)| group)
        .collect();
    groups.sort();
    groups.first().map(|g| g.to_string())
}

pub fn attach_context_to_group(group: &mut ProjectGroup, slug: &str) -> bool {
    if group.contexts.iter().any(|c| c == slug) {
        return false;
    }
    group.contexts.push(slug.to_string());
    true
}

pub fn detach_context_from_group(group: &mut ProjectGroup, slug: &str) -> bool {
    let before = group.contexts.len();
    group.contexts.retain(|c| c != slug);
    group.contexts.len() != before
}

/// What a context is attached to or detached from.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type", content = "name", rename_all = "snake_case")]
pub enum ContextTarget {
    Project(String),
    Group(String),
}

/// Attach an existing context to a project or group and persist the change.
/// Attaching to a group also brings every member project in step. Returns
/// `false` when the context was already attached.
pub fn attach_context(target: &ContextTarget, slug: &str) -> Result<bool, String> {
    if !context_exists(slug)? {
        return Err(format!("Context '{}' does not exist", slug));
    }
    match target {
        ContextTarget::Project(name) => {
            let mut project = read_project_parsed(name)?;
            reconcile_group_contexts(&mut project, &groups_for_project(name));
            let changed = attach_context_to_project(&mut project, slug);
            write_project(name, &project)?;
            Ok(changed)
        }
        ContextTarget::Group(name) => update_group(name, |group| attach_context_to_group(group, slug)),
    }
}

/// Detach a context from a project or group and persist the change.
/// Detaching from a group removes it from member projects unless another of
/// their groups still provides it. Returns `false` when it was not attached.
pub fn detach_context(target: &ContextTarget, slug: &str) -> Result<bool, String> {
    match target {
        ContextTarget::Project(name) => {
            let mut project = read_project_parsed(name)?;
            reconcile_group_contexts(&mut project, &groups_for_project(name));
            let changed = detach_context_from_project(&mut project, slug)?;
            if changed {
                write_project(name, &project)?;
            }
            Ok(changed)
        }
        ContextTarget::Group(name) => update_group(name, |group| detach_context_from_group(group, slug)),
    }
}

fn update_group(name: &str, apply: impl FnOnce(&mut ProjectGroup) -> bool) -> Result<bool, String> {
    let raw = read_group(name)?;
    let mut group: ProjectGroup =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid group '{}': {}", name, e))?;
    if !apply(&mut group) {
        return Ok(false);
    }
    group.updated_at = chrono::Utc::now().to_rfc3339();
    let data = serde_json::to_string_pretty(&group).map_err(|e| e.to_string())?;
    save_group(name, &data)?;
    reconcile_group_contexts_for_projects(&group.projects)?;
    Ok(true)
}

/// Save a group, then reconcile the contexts of every project that was a
/// member before or is a member after. Returns the projects that changed.
pub fn save_group_reconciling_contexts(name: &str, data: &str) -> Result<Vec<String>, String> {
    let mut affected = group_member_names(name);
    save_group(name, data)?;
    affected.extend(group_member_names(name));
    reconcile_group_contexts_for_projects(&affected)
}

/// Delete a group, then release the contexts it provided to its members.
/// Returns the projects that changed.
pub fn delete_group_reconciling_contexts(name: &str) -> Result<Vec<String>, String> {
    let members = group_member_names(name);
    delete_group(name)?;
    reconcile_group_contexts_for_projects(&members)
}

// ── Group reconcile ──────────────────────────────────────────────────────────

/// Bring a project's contexts in step with the groups it belongs to.
///
/// `member_groups` must be exactly the groups that list the project. The
/// rules mirror `reconcile_project_profiles`:
/// - A recorded group that is no longer in `member_groups` releases every
///   context it provided.
/// - A member group releases contexts it no longer lists, and records every
///   context it lists: added when missing, adopted when the project already
///   had it.
/// - A context another member group already records stays with that group.
///   A released context that another member group still lists is kept, and
///   its record moves to that group.
///
/// Only touches the in-memory project. Returns `true` when anything changed.
pub fn reconcile_group_contexts(project: &mut Project, member_groups: &[ProjectGroup]) -> bool {
    let mut changed = false;
    let member_names: Vec<&str> = member_groups.iter().map(|g| g.name.as_str()).collect();

    let departed: Vec<String> = project
        .group_context_contributions
        .keys()
        .filter(|name| !member_names.contains(&name.as_str()))
        .cloned()
        .collect();
    for name in departed {
        let Some(slugs) = project.group_context_contributions.remove(&name) else {
            continue;
        };
        for slug in slugs {
            release_group_context(project, member_groups, &slug, &name);
        }
        changed = true;
    }

    for group in member_groups {
        let prev = project
            .group_context_contributions
            .get(&group.name)
            .cloned()
            .unwrap_or_default();
        let mut next: Vec<String> = Vec::new();

        for slug in &prev {
            if !group.contexts.contains(slug) {
                release_group_context(project, member_groups, slug, &group.name);
                changed = true;
            }
        }

        for slug in &group.contexts {
            if next.contains(slug) {
                continue;
            }
            if !project.contexts.contains(slug) {
                project.contexts.push(slug.clone());
                next.push(slug.clone());
                changed = true;
                continue;
            }
            let owned_elsewhere = project
                .group_context_contributions
                .iter()
                .any(|(name, slugs)| name != &group.name && slugs.contains(slug));
            if !owned_elsewhere {
                next.push(slug.clone());
            }
        }

        if next != prev {
            changed = true;
        }
        if next.is_empty() {
            project.group_context_contributions.remove(&group.name);
        } else {
            project
                .group_context_contributions
                .insert(group.name.clone(), next);
        }
    }

    changed
}

/// Drop `slug` from the project because group `from` no longer provides it.
/// When another member group still lists it, it stays and the record moves.
fn release_group_context(project: &mut Project, member_groups: &[ProjectGroup], slug: &str, from: &str) {
    let other = member_groups
        .iter()
        .find(|g| g.name != from && g.contexts.iter().any(|c| c == slug));
    if let Some(other) = other {
        let entry = project
            .group_context_contributions
            .entry(other.name.clone())
            .or_default();
        if !entry.iter().any(|c| c == slug) {
            entry.push(slug.to_string());
        }
        return;
    }
    project.contexts.retain(|c| c != slug);
}

/// Re-read each named project, reconcile its group contexts against the
/// groups on disk, and save the ones that changed. Call after a group is
/// saved, deleted or renamed, with its members from before and after the
/// change. Returns the projects that were saved.
///
/// Per-project failures are logged and skipped so one unreadable project
/// does not leave the rest out of step.
pub fn reconcile_group_contexts_for_projects(project_names: &[String]) -> Result<Vec<String>, String> {
    let mut saved = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in project_names {
        if !seen.insert(name.as_str()) {
            continue;
        }
        let mut project = match read_project_parsed(name) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("contexts reconcile: skipping project '{}': {}", name, e);
                continue;
            }
        };
        let groups = groups_for_project(name);
        if !reconcile_group_contexts(&mut project, &groups) {
            continue;
        }
        if let Err(e) = write_project(name, &project) {
            eprintln!("contexts reconcile: failed to save project '{}': {}", name, e);
            continue;
        }
        saved.push(name.clone());
    }
    Ok(saved)
}

// ── Reference maintenance ────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct ContextReferences {
    pub projects: Vec<String>,
    pub groups: Vec<String>,
}

/// Remove (`new_slug = None`) or rename every reference to `old_slug` in
/// projects and groups. Failures on individual files are logged and skipped:
/// the context itself has already been deleted or renamed, so stopping
/// half-way would leave more dangling references, not fewer.
fn rewrite_context_references(
    old_slug: &str,
    new_slug: Option<&str>,
) -> Result<ContextReferences, String> {
    let mut changes = ContextReferences::default();

    for name in list_groups()? {
        let mut group = match read_group(&name)
            .and_then(|raw| serde_json::from_str::<ProjectGroup>(&raw).map_err(|e| e.to_string()))
        {
            Ok(g) => g,
            Err(e) => {
                eprintln!("contexts: skipping unreadable group '{}': {}", name, e);
                continue;
            }
        };
        if !rewrite_slug_list(&mut group.contexts, old_slug, new_slug) {
            continue;
        }
        group.updated_at = chrono::Utc::now().to_rfc3339();
        let saved = serde_json::to_string_pretty(&group)
            .map_err(|e| e.to_string())
            .and_then(|data| save_group(&name, &data));
        match saved {
            Ok(()) => changes.groups.push(name),
            Err(e) => eprintln!("contexts: failed to save group '{}': {}", name, e),
        }
    }

    for name in list_projects()? {
        let mut project = match read_project_parsed(&name) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("contexts: skipping unreadable project '{}': {}", name, e);
                continue;
            }
        };
        let mut changed = rewrite_slug_list(&mut project.contexts, old_slug, new_slug);
        for slugs in project.group_context_contributions.values_mut() {
            changed |= rewrite_slug_list(slugs, old_slug, new_slug);
        }
        project
            .group_context_contributions
            .retain(|_, slugs| !slugs.is_empty());
        if !changed {
            continue;
        }
        match write_project(&name, &project) {
            Ok(()) => changes.projects.push(name),
            Err(e) => eprintln!("contexts: failed to save project '{}': {}", name, e),
        }
    }

    changes.projects.sort();
    changes.groups.sort();
    Ok(changes)
}

/// Replace or remove `old` in `list`, dropping a duplicate when `new` is
/// already present. Returns `true` when the list changed.
fn rewrite_slug_list(list: &mut Vec<String>, old: &str, new: Option<&str>) -> bool {
    if !list.iter().any(|s| s == old) {
        return false;
    }
    match new {
        Some(new) if !list.iter().any(|s| s == new) => {
            for slug in list.iter_mut() {
                if slug == old {
                    *slug = new.to_string();
                }
            }
        }
        _ => list.retain(|s| s != old),
    }
    true
}

/// Every project and group that lists `slug`. Read-only; unreadable files
/// are skipped.
pub fn find_context_references(slug: &str) -> Result<ContextReferences, String> {
    let mut refs = ContextReferences::default();
    for name in list_groups()? {
        let listed = read_group(&name)
            .ok()
            .and_then(|raw| serde_json::from_str::<ProjectGroup>(&raw).ok())
            .is_some_and(|g| g.contexts.iter().any(|c| c == slug));
        if listed {
            refs.groups.push(name);
        }
    }
    for name in list_projects()? {
        let listed = read_project_parsed(&name).is_ok_and(|p| p.contexts.iter().any(|c| c == slug));
        if listed {
            refs.projects.push(name);
        }
    }
    refs.projects.sort();
    refs.groups.sort();
    Ok(refs)
}

/// The member project names of a group, or an empty list when the group
/// does not exist yet. Used to reconcile members before and after a group
/// changes.
pub fn group_member_names(name: &str) -> Vec<String> {
    read_group(name)
        .ok()
        .and_then(|raw| serde_json::from_str::<ProjectGroup>(&raw).ok())
        .map(|g| g.projects)
        .unwrap_or_default()
}

fn read_project_parsed(name: &str) -> Result<Project, String> {
    let raw = read_project(name)?;
    serde_json::from_str::<Project>(&raw).map_err(|e| format!("Invalid project '{}': {}", name, e))
}

fn write_project(name: &str, project: &Project) -> Result<(), String> {
    let data = serde_json::to_string_pretty(project).map_err(|e| e.to_string())?;
    save_project(name, &data)
}

/// Contexts attached to a project, split by where they came from. Used by
/// MCP and the editor to show provenance.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ProjectContextEntry {
    pub slug: String,
    /// Group that provides the context, or `None` for the project's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

pub fn project_context_entries(project: &Project) -> Vec<ProjectContextEntry> {
    project
        .contexts
        .iter()
        .map(|slug| ProjectContextEntry {
            slug: slug.clone(),
            group: group_providing(project, slug),
        })
        .collect()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn local_context(slug: &str, sources: Vec<ContextSource>) -> Context {
        Context {
            slug: slug.to_string(),
            display_name: String::new(),
            description: String::new(),
            body: ContextBody::Local { sources },
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn doc_source(id: &str) -> ContextSource {
        ContextSource {
            id: id.to_string(),
            display_name: String::new(),
            description: String::new(),
            spec: SourceSpec::Documentation,
        }
    }

    fn group(name: &str, projects: &[&str], contexts: &[&str]) -> ProjectGroup {
        ProjectGroup {
            name: name.to_string(),
            projects: projects.iter().map(|p| p.to_string()).collect(),
            contexts: contexts.iter().map(|c| c.to_string()).collect(),
            ..Default::default()
        }
    }

    fn project(name: &str, contexts: &[&str]) -> Project {
        Project {
            name: name.to_string(),
            contexts: contexts.iter().map(|c| c.to_string()).collect(),
            ..Default::default()
        }
    }

    fn with_home<T>(test: impl FnOnce() -> T) -> T {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let result = with_test_home(home, test);
        drop(tmp);
        result
    }

    // ── Serialisation ────────────────────────────────────────────────────────

    #[test]
    fn local_context_round_trips_with_webapp_source_shape() {
        let raw = r#"{
            "slug": "payments",
            "location": "local",
            "sources": [
                {"id": "spec", "kind": "documentation"},
                {"id": "adr", "kind": "local", "config": {"path": "/tmp/adr", "include": ["**/*.md"]}},
                {"id": "api", "kind": "url", "config": {"url": "https://example.com/api.md"}},
                {"id": "team", "kind": "cloud", "config": {"source_id": "src_1"}}
            ]
        }"#;
        let context: Context = serde_json::from_str(raw).expect("parse");
        let sources = context.local_sources().expect("local");
        assert_eq!(sources.len(), 4);
        assert_eq!(sources[0].spec, SourceSpec::Documentation);
        assert_eq!(
            sources[2].spec,
            SourceSpec::Url(UrlSourceConfig {
                url: "https://example.com/api.md".into(),
                ttl_secs: DEFAULT_URL_TTL_SECS,
            })
        );

        let value = serde_json::to_value(&context).expect("serialise");
        assert_eq!(value["location"], "local");
        assert_eq!(value["sources"][1]["kind"], "local");
        assert_eq!(value["sources"][1]["config"]["path"], "/tmp/adr");
        let again: Context = serde_json::from_value(value).expect("reparse");
        assert_eq!(again, context);
    }

    #[test]
    fn cloud_context_round_trips() {
        let raw = r#"{"slug": "team", "location": "cloud", "context_id": "ctx_abc"}"#;
        let context: Context = serde_json::from_str(raw).expect("parse");
        assert_eq!(
            context.body,
            ContextBody::Cloud {
                context_id: "ctx_abc".into()
            }
        );
        assert!(context.local_sources().is_none());
        let again: Context =
            serde_json::from_str(&serde_json::to_string(&context).unwrap()).expect("reparse");
        assert_eq!(again, context);
    }

    // ── Validation ───────────────────────────────────────────────────────────

    #[test]
    fn slug_grammar_matches_webapp() {
        for ok in ["a", "payments", "a.b_c-d", "0day"] {
            assert!(is_valid_context_slug(ok), "{ok}");
        }
        for bad in ["", ".", "..", "-a", "A", "a/b", "a b", &"a".repeat(129)] {
            assert!(!is_valid_context_slug(bad), "{bad}");
        }
    }

    #[test]
    fn validate_rejects_bad_sources() {
        let dup = local_context("c", vec![doc_source("a"), doc_source("a")]);
        assert!(validate_context(&dup).unwrap_err().contains("duplicate"));

        let relative = local_context(
            "c",
            vec![ContextSource {
                spec: SourceSpec::Local(LocalSourceConfig {
                    path: "docs".into(),
                    include: vec![],
                }),
                ..doc_source("a")
            }],
        );
        assert!(validate_context(&relative).unwrap_err().contains("absolute"));

        for (url, expect) in [
            ("file:///etc/passwd", "http or https"),
            ("https://user:pw@example.com/x", "credentials"),
            ("not a url", "invalid URL"),
        ] {
            let context = local_context(
                "c",
                vec![ContextSource {
                    spec: SourceSpec::Url(UrlSourceConfig {
                        url: url.into(),
                        ttl_secs: 0,
                    }),
                    ..doc_source("a")
                }],
            );
            let err = validate_context(&context).unwrap_err();
            assert!(err.contains(expect), "{url}: {err}");
        }

        let cloud = Context {
            body: ContextBody::Cloud {
                context_id: "src_1".into(),
            },
            ..local_context("c", vec![])
        };
        assert!(validate_context(&cloud).is_err());
    }

    #[test]
    fn document_paths_follow_okf_rules() {
        for ok in ["index.md", "guides/setup.md", "a/b/c-d_e.v2.md"] {
            assert!(validate_document_path(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "notes.txt",
            "../x.md",
            "a/../x.md",
            "/abs.md",
            ".hidden.md",
            "a\\b.md",
            "a%2e.md",
            "a//b.md",
            "a/b/c/d/e/f/g/h/i/j/k.md",
        ] {
            assert!(validate_document_path(bad).is_err(), "{bad}");
        }
    }

    // ── Storage ──────────────────────────────────────────────────────────────

    #[test]
    fn save_read_list_and_documentation_pages() {
        with_home(|| {
            let saved = save_context(local_context("payments", vec![doc_source("spec")]))
                .expect("save");
            assert!(!saved.created_at.is_empty());
            assert_eq!(list_contexts().unwrap(), vec!["payments"]);
            assert_eq!(read_context("payments").unwrap().slug, "payments");

            write_documentation_page("payments", "spec", "guides/setup.md", "# Setup").unwrap();
            write_documentation_page("payments", "spec", "index.md", "# Index").unwrap();
            assert_eq!(
                list_documentation_pages("payments", "spec").unwrap(),
                vec!["guides/setup.md", "index.md"]
            );
            assert_eq!(
                read_documentation_page("payments", "spec", "guides/setup.md").unwrap(),
                "# Setup"
            );
            assert!(write_documentation_page("payments", "nope", "x.md", "").is_err());
            assert!(write_documentation_page(
                "payments",
                "spec",
                "big.md",
                &"x".repeat(DOCUMENT_MAX_BYTES + 1)
            )
            .is_err());

            move_documentation_page("payments", "spec", "guides/setup.md", "how-to/deep/setup.md").unwrap();
            assert_eq!(
                list_documentation_pages("payments", "spec").unwrap(),
                vec!["how-to/deep/setup.md", "index.md"]
            );
            let source_dir = get_contexts_dir().unwrap().join("payments/sources/spec");
            assert!(!source_dir.join("guides").exists(), "empty folder removed");
            let err = move_documentation_page("payments", "spec", "index.md", "how-to/deep/setup.md").unwrap_err();
            assert!(err.contains("already exists"));
            assert!(move_documentation_page("payments", "spec", "ghost.md", "x.md").unwrap_err().contains("not found"));
            assert!(move_documentation_page("payments", "spec", "index.md", "../x.md").is_err());
            move_documentation_page("payments", "spec", "how-to/deep/setup.md", "guides/setup.md").unwrap();
            assert!(!source_dir.join("how-to").exists(), "nested empty folders removed");

            delete_documentation_page("payments", "spec", "index.md").unwrap();
            assert_eq!(
                list_documentation_pages("payments", "spec").unwrap(),
                vec!["guides/setup.md"]
            );
        });
    }

    #[test]
    fn read_rejects_mismatched_slug_and_missing_context() {
        with_home(|| {
            assert!(read_context("ghost").unwrap_err().contains("not found"));
            save_context(local_context("real", vec![])).unwrap();
            let dir = get_contexts_dir().unwrap();
            fs::copy(dir.join("real.json"), dir.join("copy.json")).unwrap();
            assert!(read_context("copy").unwrap_err().contains("declares slug"));
        });
    }

    // ── Attachment ───────────────────────────────────────────────────────────

    #[test]
    fn detach_is_refused_for_group_provided_context() {
        let mut p = project("app", &["a", "b"]);
        p.group_context_contributions
            .insert("team".into(), vec!["a".into()]);
        let err = detach_context_from_project(&mut p, "a").unwrap_err();
        assert!(err.contains("group 'team'"));
        assert!(detach_context_from_project(&mut p, "b").unwrap());
        assert_eq!(p.contexts, vec!["a"]);
        assert!(!attach_context_to_project(&mut p, "a"));
        assert!(attach_context_to_project(&mut p, "c"));
    }

    // ── Group reconcile ──────────────────────────────────────────────────────

    #[test]
    fn reconcile_adds_and_records_group_contexts() {
        let mut p = project("app", &["own"]);
        let groups = vec![group("team", &["app"], &["shared", "own"])];
        assert!(reconcile_group_contexts(&mut p, &groups));
        assert_eq!(p.contexts, vec!["own", "shared"]);
        // "own" is adopted: the group lists it, so the group owns it now.
        assert_eq!(
            p.group_context_contributions.get("team").unwrap(),
            &vec!["shared".to_string(), "own".to_string()]
        );
        assert!(!reconcile_group_contexts(&mut p, &groups), "idempotent");
    }

    #[test]
    fn reconcile_releases_when_project_leaves_group() {
        let mut p = project("app", &[]);
        let groups = vec![group("team", &["app"], &["shared"])];
        reconcile_group_contexts(&mut p, &groups);
        assert!(reconcile_group_contexts(&mut p, &[]));
        assert!(p.contexts.is_empty());
        assert!(p.group_context_contributions.is_empty());
    }

    #[test]
    fn reconcile_hands_shared_context_to_remaining_group() {
        let mut p = project("app", &[]);
        let both = vec![
            group("a", &["app"], &["shared"]),
            group("b", &["app"], &["shared"]),
        ];
        reconcile_group_contexts(&mut p, &both);
        assert_eq!(p.contexts, vec!["shared"]);
        assert_eq!(p.group_context_contributions.len(), 1);

        let only_b = vec![group("b", &["app"], &["shared"])];
        reconcile_group_contexts(&mut p, &only_b);
        assert_eq!(p.contexts, vec!["shared"]);
        assert_eq!(
            p.group_context_contributions.get("b").unwrap(),
            &vec!["shared".to_string()]
        );
        assert!(!p.group_context_contributions.contains_key("a"));
    }

    #[test]
    fn reconcile_drops_context_a_group_stops_listing() {
        let mut p = project("app", &[]);
        reconcile_group_contexts(&mut p, &[group("team", &["app"], &["x", "y"])]);
        reconcile_group_contexts(&mut p, &[group("team", &["app"], &["y"])]);
        assert_eq!(p.contexts, vec!["y"]);
        assert_eq!(
            p.group_context_contributions.get("team").unwrap(),
            &vec!["y".to_string()]
        );
    }

    // ── Reference maintenance ────────────────────────────────────────────────

    fn save_group_value(g: &ProjectGroup) {
        save_group(&g.name, &serde_json::to_string(g).unwrap()).unwrap();
    }

    #[test]
    fn delete_and_rename_rewrite_projects_and_groups() {
        with_home(|| {
            save_context(local_context("old", vec![doc_source("spec")])).unwrap();
            write_documentation_page("old", "spec", "index.md", "# Hi").unwrap();

            let mut p = project("app", &["old", "keep"]);
            p.group_context_contributions
                .insert("team".into(), vec!["old".into()]);
            write_project("app", &p).unwrap();
            save_group_value(&group("team", &["app"], &["old"]));

            let changes = rename_context("old", "new").unwrap();
            assert_eq!(changes.projects, vec!["app"]);
            assert_eq!(changes.groups, vec!["team"]);
            assert!(!context_exists("old").unwrap());
            assert_eq!(
                read_documentation_page("new", "spec", "index.md").unwrap(),
                "# Hi"
            );
            let p = read_project_parsed("app").unwrap();
            assert_eq!(p.contexts, vec!["new", "keep"]);
            assert_eq!(
                p.group_context_contributions.get("team").unwrap(),
                &vec!["new".to_string()]
            );

            let changes = delete_context("new").unwrap();
            assert_eq!(changes.projects, vec!["app"]);
            let p = read_project_parsed("app").unwrap();
            assert_eq!(p.contexts, vec!["keep"]);
            assert!(p.group_context_contributions.is_empty());
            assert!(!get_contexts_dir().unwrap().join("new").exists());
        });
    }

    #[test]
    fn attach_and_detach_through_group_reach_members() {
        with_home(|| {
            save_context(local_context("shared", vec![])).unwrap();
            write_project("app", &project("app", &[])).unwrap();
            save_group_value(&group("team", &["app"], &[]));
            let team = ContextTarget::Group("team".into());

            assert!(attach_context(&team, "shared").unwrap());
            assert!(!attach_context(&team, "shared").unwrap(), "idempotent");
            let p = read_project_parsed("app").unwrap();
            assert_eq!(p.contexts, vec!["shared"]);
            assert_eq!(group_providing(&p, "shared").as_deref(), Some("team"));

            let app = ContextTarget::Project("app".into());
            assert!(detach_context(&app, "shared").unwrap_err().contains("group 'team'"));

            assert!(detach_context(&team, "shared").unwrap());
            assert!(read_project_parsed("app").unwrap().contexts.is_empty());

            assert!(attach_context(&app, "ghost").unwrap_err().contains("does not exist"));
            assert!(attach_context(&app, "shared").unwrap());
            assert_eq!(find_context_references("shared").unwrap().projects, vec!["app"]);
        });
    }

    #[test]
    fn saving_group_membership_reconciles_old_and_new_members() {
        with_home(|| {
            write_project("a", &project("a", &[])).unwrap();
            write_project("b", &project("b", &[])).unwrap();
            let json = |g: &ProjectGroup| serde_json::to_string(g).unwrap();

            save_group_reconciling_contexts("team", &json(&group("team", &["a"], &["x"]))).unwrap();
            assert_eq!(read_project_parsed("a").unwrap().contexts, vec!["x"]);

            let changed =
                save_group_reconciling_contexts("team", &json(&group("team", &["b"], &["x"]))).unwrap();
            assert_eq!(changed, vec!["a", "b"]);
            assert!(read_project_parsed("a").unwrap().contexts.is_empty());
            assert_eq!(read_project_parsed("b").unwrap().contexts, vec!["x"]);

            delete_group_reconciling_contexts("team").unwrap();
            assert!(read_project_parsed("b").unwrap().contexts.is_empty());
        });
    }

    #[test]
    fn titles_become_page_paths() {
        assert_eq!(segment_from_title("Setup guide!"), "setup-guide");
        assert_eq!(segment_from_title("  --Hello World--  "), "hello-world");
        assert_eq!(segment_from_title("!!!"), "untitled");
        assert_eq!(segment_from_title("v1..2"), "v1.2");
        assert_eq!(page_path_for(Some("Guides/Trouble Shooting"), "DNS"), "guides/trouble-shooting/dns.md");
        assert_eq!(page_path_for(None, "Overview"), "overview.md");
        assert!(validate_document_path(&page_path_for(Some("a/b"), "c")).is_ok());
    }

    #[test]
    fn agent_writes_create_context_and_pages() {
        with_home(|| {
            let ctx = create_local_context("Coding standards", "How we write code").unwrap();
            assert_eq!(ctx.slug, "coding-standards");
            assert_eq!(create_local_context("Coding standards", "").unwrap().slug, "coding-standards-2");

            let path = write_context_page(
                "coding-standards",
                PageTarget::Title { folder: Some("Guides"), title: "Code review" },
                "# Code review\nBe kind.\n",
            )
            .unwrap();
            assert_eq!(path, "guides/code-review.md");
            let content = read_documentation_page("coding-standards", PAGES_SOURCE_ID, &path).unwrap();
            assert!(content.starts_with("---\ntype: page\ntitle: \"Code review\"\n---\n"));

            // Replacing keeps the existing frontmatter.
            write_context_page("coding-standards", PageTarget::Path(&path), "# Code review\nBe very kind.\n").unwrap();
            let content = read_documentation_page("coding-standards", PAGES_SOURCE_ID, &path).unwrap();
            assert!(content.contains("title: \"Code review\""));
            assert!(content.ends_with("Be very kind.\n"));

            let moved = move_context_page("coding-standards", &path, Some(""), Some("Reviews")).unwrap();
            assert_eq!(moved, "reviews.md");
            delete_context_page("coding-standards", &moved).unwrap();
            assert!(delete_context_page("coding-standards", &moved).unwrap_err().contains("no page"));
        });
    }

    #[test]
    fn agent_writes_add_a_pages_source_and_refuse_cloud_contexts() {
        with_home(|| {
            save_context(local_context("bare", vec![])).unwrap();
            assert_eq!(ensure_pages_source("bare").unwrap(), PAGES_SOURCE_ID);
            assert_eq!(ensure_pages_source("bare").unwrap(), PAGES_SOURCE_ID, "idempotent");
            save_context(Context {
                body: ContextBody::Cloud { context_id: "ctx_1".into() },
                ..local_context("remote", vec![])
            })
            .unwrap();
            assert!(ensure_pages_source("remote").unwrap_err().contains("cloud"));
        });
    }

    #[test]
    fn rename_refuses_existing_target() {
        with_home(|| {
            save_context(local_context("a", vec![])).unwrap();
            save_context(local_context("b", vec![])).unwrap();
            assert!(rename_context("a", "b").unwrap_err().contains("already exists"));
        });
    }

    #[test]
    fn reconcile_for_projects_reads_groups_from_disk() {
        with_home(|| {
            write_project("app", &project("app", &[])).unwrap();
            save_group_value(&group("team", &["app"], &["shared"]));
            let saved = reconcile_group_contexts_for_projects(&["app".into()]).unwrap();
            assert_eq!(saved, vec!["app"]);
            assert_eq!(read_project_parsed("app").unwrap().contexts, vec!["shared"]);

            delete_group("team").unwrap();
            reconcile_group_contexts_for_projects(&["app".into()]).unwrap();
            assert!(read_project_parsed("app").unwrap().contexts.is_empty());
        });
    }
}
