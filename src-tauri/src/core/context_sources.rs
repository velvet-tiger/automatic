use std::fs;
use std::io::Read;
use std::net::{IpAddr, SocketAddr};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::*;

// ── Context source readers ───────────────────────────────────────────────────
//
// Every source kind answers the same two questions: which entries does it
// expose, and what is the text of one entry. Agents call these through MCP,
// so every reader enforces its own limits and never reads outside what the
// source names.
//
//   documentation  pages stored by Automatic (see `contexts.rs`)
//   local          files under an absolute path, filtered by globs,
//                  confined to that path (symlinks may not escape it)
//   url            one entry, fetched over HTTP(S) and cached for ttl_secs
//   cloud          a webapp source's documents, read through the webapp API

/// Largest local file or URL body an agent can read through a source.
pub const SOURCE_ENTRY_MAX_BYTES: usize = 512_000;
/// Most entries a local directory lists before the listing is truncated.
pub const LOCAL_ENTRIES_MAX: usize = 2000;
/// Deepest directory level a local listing walks.
const LOCAL_MAX_DEPTH: usize = 16;
/// Used when a local source has no `include` patterns.
pub const DEFAULT_LOCAL_INCLUDE: &[&str] = &["**/*.md", "**/*.mdx", "**/*.txt"];
/// The single entry a URL source exposes.
pub const URL_ENTRY_PATH: &str = "content";

const URL_FETCH_TIMEOUT_SECS: u64 = 30;
const URL_MAX_REDIRECTS: usize = 5;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SourceEntry {
    /// Pass this back to `read_source_entry`.
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct SourceListing {
    pub entries: Vec<SourceEntry>,
    /// `true` when the source holds more entries than the listing limit.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// A source as an agent sees it, whether it is defined locally or in the
/// webapp.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SourceSummary {
    pub id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
}

// ── Dispatch ─────────────────────────────────────────────────────────────────

/// The sources of a context. For a cloud context this asks the webapp and
/// lists only the sources the signed-in account can still read.
pub async fn list_context_sources(context: &Context) -> Result<Vec<SourceSummary>, String> {
    match &context.body {
        ContextBody::Local { sources } => Ok(sources
            .iter()
            .map(|s| SourceSummary {
                id: s.id.clone(),
                kind: source_kind_name(&s.spec).to_string(),
                display_name: s.display_name.clone(),
                description: s.description.clone(),
            })
            .collect()),
        ContextBody::Cloud { context_id } => {
            let detail = cloud::read_context(context_id).await?;
            Ok(detail
                .sources
                .into_iter()
                .filter_map(|link| link.source)
                .map(|s| SourceSummary {
                    id: s.id,
                    kind: s.kind,
                    display_name: s.display_name,
                    description: s.description.unwrap_or_default(),
                })
                .collect())
        }
    }
}

pub async fn list_source_entries(context: &Context, source_id: &str) -> Result<SourceListing, String> {
    match resolve_source(context, source_id).await? {
        ResolvedSource::Documentation => {
            let pages = list_documentation_pages(&context.slug, source_id)?;
            Ok(SourceListing {
                entries: pages
                    .into_iter()
                    .map(|path| SourceEntry {
                        path,
                        title: None,
                        size: None,
                    })
                    .collect(),
                truncated: false,
            })
        }
        ResolvedSource::Local(config) => list_local_entries(&config),
        ResolvedSource::Url(_) => Ok(SourceListing {
            entries: vec![SourceEntry {
                path: URL_ENTRY_PATH.to_string(),
                title: None,
                size: None,
            }],
            truncated: false,
        }),
        ResolvedSource::Cloud(cloud_source_id) => {
            let detail = cloud::read_source(&cloud_source_id).await?;
            Ok(SourceListing {
                entries: detail
                    .documents
                    .into_iter()
                    .map(|d| SourceEntry {
                        path: d.path,
                        title: d.title,
                        size: None,
                    })
                    .collect(),
                truncated: false,
            })
        }
    }
}

pub async fn read_source_entry(context: &Context, source_id: &str, path: &str) -> Result<String, String> {
    match resolve_source(context, source_id).await? {
        ResolvedSource::Documentation => read_documentation_page(&context.slug, source_id, path),
        ResolvedSource::Local(config) => read_local_entry(&config, path),
        ResolvedSource::Url(config) => {
            if path != URL_ENTRY_PATH {
                return Err(format!(
                    "URL source '{}' has one entry, '{}'",
                    source_id, URL_ENTRY_PATH
                ));
            }
            read_url_source(&config).await
        }
        ResolvedSource::Cloud(cloud_source_id) => {
            cloud::read_source_document(&cloud_source_id, path).await
        }
    }
}

pub fn source_kind_name(spec: &SourceSpec) -> &'static str {
    match spec {
        SourceSpec::Documentation => "documentation",
        SourceSpec::Local(_) => "local",
        SourceSpec::Url(_) => "url",
        SourceSpec::Cloud(_) => "cloud",
    }
}

enum ResolvedSource {
    Documentation,
    Local(LocalSourceConfig),
    Url(UrlSourceConfig),
    /// A webapp source id.
    Cloud(String),
}

/// Find `source_id` in the context. In a cloud context the id is a webapp
/// source id, and it must be one of that context's resolved members: an
/// agent reading a context should not reach sources outside it.
async fn resolve_source(context: &Context, source_id: &str) -> Result<ResolvedSource, String> {
    match &context.body {
        ContextBody::Local { sources } => {
            let source = sources.iter().find(|s| s.id == source_id).ok_or_else(|| {
                format!("Context '{}' has no source '{}'", context.slug, source_id)
            })?;
            Ok(match &source.spec {
                SourceSpec::Documentation => ResolvedSource::Documentation,
                SourceSpec::Local(c) => ResolvedSource::Local(c.clone()),
                SourceSpec::Url(c) => ResolvedSource::Url(c.clone()),
                SourceSpec::Cloud(c) => ResolvedSource::Cloud(c.source_id.clone()),
            })
        }
        ContextBody::Cloud { context_id } => {
            let detail = cloud::read_context(context_id).await?;
            let member = detail
                .sources
                .iter()
                .any(|link| link.source_id == source_id && link.source.is_some());
            if member {
                Ok(ResolvedSource::Cloud(source_id.to_string()))
            } else {
                Err(format!(
                    "Context '{}' has no readable source '{}'",
                    context.slug, source_id
                ))
            }
        }
    }
}

// ── Local ────────────────────────────────────────────────────────────────────

fn compile_patterns(config: &LocalSourceConfig) -> Result<Vec<glob::Pattern>, String> {
    let raw: Vec<&str> = if config.include.is_empty() {
        DEFAULT_LOCAL_INCLUDE.to_vec()
    } else {
        config.include.iter().map(String::as_str).collect()
    };
    raw.into_iter()
        .map(|p| glob::Pattern::new(p).map_err(|e| format!("invalid include pattern '{}': {}", p, e)))
        .collect()
}

fn matches_any(patterns: &[glob::Pattern], relative: &str) -> bool {
    let options = glob::MatchOptions {
        case_sensitive: true,
        require_literal_separator: true,
        require_literal_leading_dot: true,
    };
    patterns.iter().any(|p| p.matches_with(relative, options))
}

fn canonical_root(config: &LocalSourceConfig) -> Result<PathBuf, String> {
    fs::canonicalize(&config.path)
        .map_err(|e| format!("Local source path '{}' is not readable: {}", config.path, e))
}

pub fn list_local_entries(config: &LocalSourceConfig) -> Result<SourceListing, String> {
    let root = canonical_root(config)?;
    if root.is_file() {
        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("Local source path '{}' has no file name", config.path))?;
        let size = fs::metadata(&root).ok().map(|m| m.len());
        return Ok(SourceListing {
            entries: vec![SourceEntry {
                path: name.to_string(),
                title: None,
                size,
            }],
            truncated: false,
        });
    }
    let patterns = compile_patterns(config)?;
    let mut listing = SourceListing::default();
    walk_local(&root, &root, 0, &patterns, &mut listing)?;
    listing.entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(listing)
}

fn walk_local(
    root: &Path,
    dir: &Path,
    depth: usize,
    patterns: &[glob::Pattern],
    listing: &mut SourceListing,
) -> Result<(), String> {
    if depth >= LOCAL_MAX_DEPTH {
        return Ok(());
    }
    let entries =
        fs::read_dir(dir).map_err(|e| format!("Failed to read {}: {}", dir.display(), e))?;
    for entry in entries.flatten() {
        if listing.truncated {
            return Ok(());
        }
        let name = entry.file_name();
        // Hidden entries (.git, .env, ...) are never exposed.
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        // `file_type` does not follow symlinks, so linked files and
        // directories are skipped rather than walked out of the root.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            walk_local(root, &path, depth + 1, patterns, listing)?;
        } else if file_type.is_file() {
            let Some(relative) = slash_relative(root, &path) else {
                continue;
            };
            if !matches_any(patterns, &relative) {
                continue;
            }
            if listing.entries.len() >= LOCAL_ENTRIES_MAX {
                listing.truncated = true;
                return Ok(());
            }
            listing.entries.push(SourceEntry {
                path: relative,
                title: None,
                size: entry.metadata().ok().map(|m| m.len()),
            });
        }
    }
    Ok(())
}

fn slash_relative(root: &Path, path: &Path) -> Option<String> {
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

/// Read one entry of a local source. The requested path must be relative,
/// must not climb out of the root, must match the include patterns, and
/// must still be inside the root once symlinks are resolved.
pub fn read_local_entry(config: &LocalSourceConfig, path: &str) -> Result<String, String> {
    let root = canonical_root(config)?;

    let target = if root.is_file() {
        let name = root.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if path != name {
            return Err(format!("Local source '{}' has one entry, '{}'", config.path, name));
        }
        root.clone()
    } else {
        let relative = Path::new(path);
        let plain = relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
        if path.is_empty() || !plain {
            return Err(format!(
                "Path '{}' must be relative to the source and must not contain '..'",
                path
            ));
        }
        if path.split('/').any(|segment| segment.starts_with('.')) {
            return Err(format!("Path '{}' names a hidden file", path));
        }
        if !matches_any(&compile_patterns(config)?, path) {
            return Err(format!(
                "Path '{}' is not included by this source's patterns",
                path
            ));
        }
        let resolved = fs::canonicalize(root.join(relative))
            .map_err(|e| format!("Entry '{}' is not readable: {}", path, e))?;
        if !resolved.starts_with(&root) {
            return Err(format!("Entry '{}' resolves outside the source", path));
        }
        resolved
    };

    read_text_file_capped(&target, path)
}

fn read_text_file_capped(file: &Path, label: &str) -> Result<String, String> {
    let meta = fs::metadata(file).map_err(|e| format!("Entry '{}' is not readable: {}", label, e))?;
    if !meta.is_file() {
        return Err(format!("Entry '{}' is not a file", label));
    }
    if meta.len() > SOURCE_ENTRY_MAX_BYTES as u64 {
        return Err(format!(
            "Entry '{}' is {} bytes; the limit is {}",
            label,
            meta.len(),
            SOURCE_ENTRY_MAX_BYTES
        ));
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    fs::File::open(file)
        .and_then(|f| f.take(SOURCE_ENTRY_MAX_BYTES as u64 + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("Failed to read entry '{}': {}", label, e))?;
    String::from_utf8(bytes).map_err(|_| format!("Entry '{}' is not UTF-8 text", label))
}

// ── URL ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
struct CachedUrl {
    url: String,
    /// Unix seconds.
    fetched_at: i64,
    body: String,
}

fn url_cache_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join("cache").join("contexts"))
}

fn url_cache_file(dir: &Path, url: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(url.as_bytes());
    let name: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
    dir.join(format!("{}.json", name))
}

fn read_url_cache(dir: &Path, url: &str) -> Option<CachedUrl> {
    let raw = fs::read_to_string(url_cache_file(dir, url)).ok()?;
    let cached: CachedUrl = serde_json::from_str(&raw).ok()?;
    // A hash collision would hand back another URL's body.
    (cached.url == url).then_some(cached)
}

fn write_url_cache(dir: &Path, cached: &CachedUrl) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|e| format!("Failed to create cache dir {}: {}", dir.display(), e))?;
    let data = serde_json::to_string(cached).map_err(|e| e.to_string())?;
    fs::write(url_cache_file(dir, &cached.url), data)
        .map_err(|e| format!("Failed to write URL cache: {}", e))
}

fn is_fresh(cached: &CachedUrl, ttl_secs: u64, now: i64) -> bool {
    ttl_secs > 0 && now.saturating_sub(cached.fetched_at) < ttl_secs as i64
}

/// Serve a URL source from cache while fresh, otherwise fetch and cache it.
/// A failed fetch is an error even when a stale copy exists, so an agent is
/// never handed out-of-date content without knowing.
async fn read_url_source(config: &UrlSourceConfig) -> Result<String, String> {
    let dir = url_cache_dir()?;
    let now = chrono::Utc::now().timestamp();
    if let Some(cached) = read_url_cache(&dir, &config.url) {
        if is_fresh(&cached, config.ttl_secs, now) {
            return Ok(cached.body);
        }
    }
    let body = fetch_url(&config.url).await?;
    let cached = CachedUrl {
        url: config.url.clone(),
        fetched_at: now,
        body,
    };
    if config.ttl_secs > 0 {
        if let Err(e) = write_url_cache(&dir, &cached) {
            // The content was fetched; a cache failure only costs a refetch.
            eprintln!("contexts: {}", e);
        }
    }
    Ok(cached.body)
}

fn is_text_content_type(content_type: &str) -> bool {
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    mime.starts_with("text/")
        || matches!(
            mime.as_str(),
            "application/json"
                | "application/xml"
                | "application/yaml"
                | "application/x-yaml"
                | "application/markdown"
        )
        || mime.ends_with("+json")
        || mime.ends_with("+xml")
}

/// Fetch a URL source, following redirects by hand.
///
/// The URL itself is the user's choice, so it may point at a local or
/// private address (an intranet wiki, a local docs server). A redirect is
/// the remote site's choice, so it may not move from a public address to a
/// non-public one: otherwise any page could steer the fetch at the user's
/// machine, LAN, or a cloud metadata endpoint and hand the reply to an agent.
async fn fetch_url(url: &str) -> Result<String, String> {
    fetch_url_with(url, |addr| is_public_ip(addr.ip())).await
}

/// `fetch_url` with the address classifier injected, so tests can mark a
/// loopback test server as "public".
async fn fetch_url_with(url: &str, is_public: impl Fn(&SocketAddr) -> bool) -> Result<String, String> {
    let mut current = url::Url::parse(url).map_err(|e| format!("Invalid URL {}: {}", url, e))?;
    let mut previous_was_public: Option<bool> = None;

    for _ in 0..=URL_MAX_REDIRECTS {
        let addrs = resolve_url_host(&current).await?;
        let public = addrs.iter().all(&is_public);
        if !redirect_allowed(previous_was_public, public) {
            return Err(format!(
                "{} redirected to {}, which is a private or local address. \
                 Redirects from public pages to private addresses are blocked.",
                url, current
            ));
        }

        let resp = pinned_client(&current, &addrs)?
            .get(current.clone())
            .send()
            .await
            .map_err(|e| format!("Failed to fetch {}: {}", url, e))?;

        if resp.status().is_redirection() {
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| format!("{} sent a redirect with no usable Location header", url))?;
            current = next_hop(&current, location)?;
            previous_was_public = Some(public);
            continue;
        }
        return read_text_response(resp, url).await;
    }
    Err(format!("{} redirected more than {} times", url, URL_MAX_REDIRECTS))
}

/// A redirect may not move from a public address to a non-public one. The
/// first request (`previous` is `None`) is always allowed.
fn redirect_allowed(previous_was_public: Option<bool>, next_is_public: bool) -> bool {
    !(previous_was_public == Some(true) && !next_is_public)
}

/// Resolve a redirect's `Location` against the current URL. Only http and
/// https are followed, and URLs carrying credentials are refused.
fn next_hop(current: &url::Url, location: &str) -> Result<url::Url, String> {
    let next = current
        .join(location)
        .map_err(|e| format!("{} redirected to an invalid address '{}': {}", current, location, e))?;
    if next.scheme() != "http" && next.scheme() != "https" {
        return Err(format!("{} redirected to a non-web address: {}", current, next));
    }
    if !next.username().is_empty() || next.password().is_some() {
        return Err(format!("{} redirected to an address with embedded credentials", current));
    }
    Ok(next)
}

async fn resolve_url_host(url: &url::Url) -> Result<Vec<SocketAddr>, String> {
    let port = url
        .port_or_known_default()
        .ok_or_else(|| format!("{} has no port", url))?;
    let addrs: Vec<SocketAddr> = match url.host() {
        Some(url::Host::Ipv4(ip)) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
        Some(url::Host::Ipv6(ip)) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
        Some(url::Host::Domain(domain)) => tokio::net::lookup_host((domain, port))
            .await
            .map_err(|e| format!("Couldn't look up {}: {}", domain, e))?
            .collect(),
        None => return Err(format!("{} has no host", url)),
    };
    if addrs.is_empty() {
        return Err(format!("{} did not resolve to any address", url));
    }
    Ok(addrs)
}

/// A client that follows no redirects and connects only to the addresses
/// that were just checked, so a second DNS answer cannot swap them.
fn pinned_client(url: &url::Url, addrs: &[SocketAddr]) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(URL_FETCH_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(url::Host::Domain(domain)) = url.host() {
        builder = builder.resolve_to_addrs(domain, addrs);
    }
    builder
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e))
}

/// True for addresses on the public internet. Loopback, private, link-local,
/// shared (CGNAT), unspecified, multicast, broadcast, documentation,
/// benchmarking and reserved ranges are not public.
fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                || a == 0
                || (a == 100 && (64..128).contains(&b))
                || (a == 198 && (b == 18 || b == 19))
                || a >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(mapped));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || (first == 0x2001 && v6.segments()[1] == 0x0db8))
        }
    }
}

async fn read_text_response(mut resp: reqwest::Response, url: &str) -> Result<String, String> {
    if !resp.status().is_success() {
        return Err(format!("Fetching {} returned HTTP {}", url, resp.status()));
    }
    // A missing content type is allowed; the UTF-8 check below still
    // rejects binary bodies.
    if let Some(content_type) = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        if !is_text_content_type(content_type) {
            return Err(format!(
                "{} returned '{}', which is not a text format",
                url, content_type
            ));
        }
    }
    let mut bytes: Vec<u8> = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("Failed to read {}: {}", url, e))?
    {
        if bytes.len() + chunk.len() > SOURCE_ENTRY_MAX_BYTES {
            return Err(format!(
                "{} is larger than the {} byte limit",
                url, SOURCE_ENTRY_MAX_BYTES
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| format!("{} is not UTF-8 text", url))
}

// ── Cloud ────────────────────────────────────────────────────────────────────

/// Typed views of the webapp's context and source wire shapes
/// (`automatic-webapp/src/lib/context-wire.ts`, `source-wire.ts`). Only the
/// fields Automatic uses are declared; the rest are ignored.
pub mod cloud {
    use serde::de::DeserializeOwned;
    use serde::Deserialize;

    use super::super::cloud_sync::webapp_authorized_get;

    #[derive(Debug, Deserialize, Clone)]
    pub struct CloudContextDetail {
        pub id: String,
        pub slug: String,
        pub display_name: String,
        #[serde(default)]
        pub description: Option<String>,
        #[serde(default)]
        pub sources: Vec<CloudContextSourceLink>,
    }

    #[derive(Debug, Deserialize, Clone)]
    pub struct CloudContextSourceLink {
        pub source_id: String,
        /// Absent when the source is no longer visible to the caller
        /// (`status: "inaccessible"`).
        #[serde(default)]
        pub source: Option<CloudSourceSummary>,
    }

    #[derive(Debug, Deserialize, Clone)]
    pub struct CloudSourceSummary {
        pub id: String,
        pub kind: String,
        pub display_name: String,
        #[serde(default)]
        pub description: Option<String>,
    }

    #[derive(Debug, Deserialize, Clone)]
    pub struct CloudSourceDetail {
        #[serde(default)]
        pub documents: Vec<CloudDocumentSummary>,
    }

    #[derive(Debug, Deserialize, Clone)]
    pub struct CloudDocumentSummary {
        pub path: String,
        #[serde(default)]
        pub title: Option<String>,
    }

    #[derive(Debug, Deserialize, Clone)]
    struct CloudDocument {
        content: String,
    }

    async fn get_json<T: DeserializeOwned>(path: &str, what: &str) -> Result<T, String> {
        let resp = webapp_authorized_get(path).await?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(format!("{} not found in the cloud", what));
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(format!("This account cannot read {}", what));
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Reading {} failed (HTTP {}): {}", what, status, text));
        }
        resp.json::<T>()
            .await
            .map_err(|e| format!("Unexpected response for {}: {}", what, e))
    }

    pub async fn read_context(context_id: &str) -> Result<CloudContextDetail, String> {
        get_json(
            &format!("/api/contexts/{}", urlencoding::encode(context_id)),
            &format!("cloud context '{}'", context_id),
        )
        .await
    }

    pub async fn read_source(source_id: &str) -> Result<CloudSourceDetail, String> {
        get_json(
            &format!("/api/sources/{}", urlencoding::encode(source_id)),
            &format!("cloud source '{}'", source_id),
        )
        .await
    }

    pub async fn read_source_document(source_id: &str, path: &str) -> Result<String, String> {
        super::validate_document_path(path)?;
        let encoded: Vec<String> = path
            .split('/')
            .map(|segment| urlencoding::encode(segment).into_owned())
            .collect();
        let doc: CloudDocument = get_json(
            &format!(
                "/api/sources/{}/documents/{}",
                urlencoding::encode(source_id),
                encoded.join("/")
            ),
            &format!("document '{}' in cloud source '{}'", path, source_id),
        )
        .await?;
        Ok(doc.content)
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn local(path: &Path, include: &[&str]) -> LocalSourceConfig {
        LocalSourceConfig {
            path: path.to_string_lossy().into_owned(),
            include: include.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn paths(listing: &SourceListing) -> Vec<&str> {
        listing.entries.iter().map(|e| e.path.as_str()).collect()
    }

    #[test]
    fn local_listing_uses_default_patterns_and_skips_hidden() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("README.md"), "# r");
        write(&root.join("docs/guide.md"), "# g");
        write(&root.join("notes.txt"), "n");
        write(&root.join("main.rs"), "fn main() {}");
        write(&root.join(".git/HEAD.md"), "hidden");
        write(&root.join("docs/.draft.md"), "hidden");

        let listing = list_local_entries(&local(root, &[])).unwrap();
        assert_eq!(paths(&listing), vec!["README.md", "docs/guide.md", "notes.txt"]);

        let listing = list_local_entries(&local(root, &["**/*.rs"])).unwrap();
        assert_eq!(paths(&listing), vec!["main.rs"]);
    }

    #[test]
    fn local_read_is_confined_to_the_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        write(&root.join("a.md"), "inside");
        write(&tmp.path().join("secret.md"), "outside");
        let config = local(&root, &[]);

        assert_eq!(read_local_entry(&config, "a.md").unwrap(), "inside");
        assert!(read_local_entry(&config, "../secret.md").is_err());
        assert!(read_local_entry(&config, "/etc/hosts").is_err());
        assert!(read_local_entry(&config, "main.rs").is_err(), "not included");
        assert!(read_local_entry(&config, ".env").is_err());
        assert!(read_local_entry(&config, "").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn local_symlinks_cannot_escape_the_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        write(&root.join("a.md"), "inside");
        write(&tmp.path().join("secret.md"), "outside");
        std::os::unix::fs::symlink(tmp.path().join("secret.md"), root.join("link.md")).unwrap();
        std::os::unix::fs::symlink(tmp.path(), root.join("up")).unwrap();
        let config = local(&root, &[]);

        let listing = list_local_entries(&config).unwrap();
        assert_eq!(paths(&listing), vec!["a.md"], "symlinks are not listed");
        let err = read_local_entry(&config, "link.md").unwrap_err();
        assert!(err.contains("outside"), "{err}");
        let err = read_local_entry(&config, "up/secret.md").unwrap_err();
        assert!(err.contains("outside"), "{err}");
    }

    #[test]
    fn local_single_file_source_has_one_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("spec.md");
        write(&file, "spec");
        let config = local(&file, &[]);
        assert_eq!(paths(&list_local_entries(&config).unwrap()), vec!["spec.md"]);
        assert_eq!(read_local_entry(&config, "spec.md").unwrap(), "spec");
        assert!(read_local_entry(&config, "other.md").is_err());
    }

    #[test]
    fn local_read_rejects_large_and_binary_files() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("big.md"), &"x".repeat(SOURCE_ENTRY_MAX_BYTES + 1));
        fs::write(tmp.path().join("bin.md"), [0xff, 0xfe, 0x00]).unwrap();
        let config = local(tmp.path(), &[]);
        assert!(read_local_entry(&config, "big.md").unwrap_err().contains("limit"));
        assert!(read_local_entry(&config, "bin.md").unwrap_err().contains("UTF-8"));
    }

    #[test]
    fn local_listing_truncates_at_the_limit() {
        let tmp = tempfile::tempdir().unwrap();
        for i in 0..(LOCAL_ENTRIES_MAX + 3) {
            fs::write(tmp.path().join(format!("{i}.md")), "x").unwrap();
        }
        let listing = list_local_entries(&local(tmp.path(), &[])).unwrap();
        assert_eq!(listing.entries.len(), LOCAL_ENTRIES_MAX);
        assert!(listing.truncated);
    }

    #[test]
    fn url_cache_freshness_and_collision_guard() {
        let tmp = tempfile::tempdir().unwrap();
        let cached = CachedUrl {
            url: "https://example.com/a".into(),
            fetched_at: 1000,
            body: "hello".into(),
        };
        write_url_cache(tmp.path(), &cached).unwrap();
        assert_eq!(read_url_cache(tmp.path(), &cached.url), Some(cached.clone()));
        assert!(read_url_cache(tmp.path(), "https://example.com/b").is_none());

        assert!(is_fresh(&cached, 60, 1059));
        assert!(!is_fresh(&cached, 60, 1060));
        assert!(!is_fresh(&cached, 0, 1000), "ttl 0 always refetches");
    }

    #[test]
    fn public_ip_classification() {
        for public in ["93.184.216.34", "1.1.1.1", "2606:4700::1111"] {
            assert!(is_public_ip(public.parse().unwrap()), "{public}");
        }
        for private in [
            "127.0.0.1", "10.1.2.3", "172.16.0.1", "192.168.1.1", "169.254.169.254",
            "0.0.0.0", "100.64.0.1", "198.18.0.1", "255.255.255.255", "240.0.0.1",
            "::1", "::", "fd00::1", "fe80::1", "::ffff:127.0.0.1", "2001:db8::1",
        ] {
            assert!(!is_public_ip(private.parse().unwrap()), "{private}");
        }
    }

    #[test]
    fn redirect_policy_blocks_public_to_private_only() {
        assert!(redirect_allowed(None, false), "the user may point at a local URL");
        assert!(redirect_allowed(Some(false), false));
        assert!(redirect_allowed(Some(false), true));
        assert!(redirect_allowed(Some(true), true));
        assert!(!redirect_allowed(Some(true), false));
    }

    #[test]
    fn next_hop_rejects_other_schemes_and_credentials() {
        let base = url::Url::parse("https://example.com/docs/a.md").unwrap();
        assert_eq!(next_hop(&base, "b.md").unwrap().as_str(), "https://example.com/docs/b.md");
        assert!(next_hop(&base, "file:///etc/passwd").is_err());
        assert!(next_hop(&base, "https://user:pw@example.com/").is_err());
    }

    /// Serve one canned response per connection, forever.
    async fn serve(response: String) -> SocketAddr {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else { return };
                let response = response.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = socket.read(&mut buf).await;
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        addr
    }

    fn redirect_to(target: SocketAddr) -> String {
        format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{}/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            target
        )
    }

    const OK_BODY: &str =
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 6\r\nConnection: close\r\n\r\nsecret";

    #[tokio::test]
    async fn redirect_from_public_to_private_is_refused() {
        let private = serve(OK_BODY.to_string()).await;
        let public = serve(redirect_to(private)).await;
        let err = fetch_url_with(&format!("http://{}/start", public), |a| a.port() == public.port())
            .await
            .unwrap_err();
        assert!(err.contains("private or local address"), "{err}");
    }

    #[tokio::test]
    async fn redirects_between_allowed_addresses_are_followed() {
        let target = serve(OK_BODY.to_string()).await;
        let start = serve(redirect_to(target)).await;
        // Both "private": the user chose a local URL, so its redirects are fine.
        let body = fetch_url_with(&format!("http://{}/start", start), |_| false).await.unwrap();
        assert_eq!(body, "secret");
        // Both "public".
        let body = fetch_url_with(&format!("http://{}/start", start), |_| true).await.unwrap();
        assert_eq!(body, "secret");
    }

    #[tokio::test]
    async fn redirect_loops_stop() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let response = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{}/again\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            addr
        );
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else { return };
                let response = response.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = socket.read(&mut buf).await;
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        let err = fetch_url_with(&format!("http://{}/start", addr), |_| false).await.unwrap_err();
        assert!(err.contains("more than 5 times"), "{err}");
    }

    #[test]
    fn text_content_types() {
        for ok in [
            "text/markdown; charset=utf-8",
            "text/html",
            "application/json",
            "application/vnd.api+json",
        ] {
            assert!(is_text_content_type(ok), "{ok}");
        }
        for bad in ["image/png", "application/octet-stream", "application/pdf"] {
            assert!(!is_text_content_type(bad), "{bad}");
        }
    }

    #[test]
    fn cloud_wire_shapes_parse() {
        let detail: cloud::CloudContextDetail = serde_json::from_str(
            r#"{"id":"ctx_1","slug":"team","display_name":"Team","description":null,
                "owner_type":"team","owner_id":"org_1","is_editable":true,"write_policy":"shared",
                "sources":[
                  {"source_id":"src_1","added_by_user_id":"u","added_at":"t","status":"resolved",
                   "source":{"id":"src_1","slug":"wiki","display_name":"Wiki","description":null,
                             "kind":"documentation","config":{}}},
                  {"source_id":"src_2","added_by_user_id":"u","added_at":"t","status":"inaccessible"}
                ]}"#,
        )
        .unwrap();
        assert_eq!(detail.sources.len(), 2);
        assert!(detail.sources[1].source.is_none());

        let source: cloud::CloudSourceDetail = serde_json::from_str(
            r#"{"id":"src_1","kind":"documentation","documents":[
                {"path":"index.md","doc_type":"page","title":"Home","reserved":true,
                 "updated_at":"t","updated_by":null}]}"#,
        )
        .unwrap();
        assert_eq!(source.documents[0].title.as_deref(), Some("Home"));
    }

    #[tokio::test]
    async fn local_context_dispatch_lists_and_reads() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("a.md"), "alpha");
        let context = Context {
            slug: "c".into(),
            display_name: String::new(),
            description: String::new(),
            body: ContextBody::Local {
                sources: vec![ContextSource {
                    id: "docs".into(),
                    display_name: String::new(),
                    description: String::new(),
                    spec: SourceSpec::Local(local(tmp.path(), &[])),
                }],
            },
            created_at: String::new(),
            updated_at: String::new(),
        };
        let sources = list_context_sources(&context).await.unwrap();
        assert_eq!(sources[0].kind, "local");
        let listing = list_source_entries(&context, "docs").await.unwrap();
        assert_eq!(paths(&listing), vec!["a.md"]);
        assert_eq!(read_source_entry(&context, "docs", "a.md").await.unwrap(), "alpha");
        assert!(read_source_entry(&context, "nope", "a.md").await.is_err());
    }
}
