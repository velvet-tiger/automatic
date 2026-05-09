use std::fs;
use std::path::PathBuf;

use super::paths::get_automatic_dir;

// ── Discover file names ───────────────────────────────────────────────────────

/// Subdirectory inside `~/.automatic` (or `~/.automatic-dev`) that holds the
/// three Discover catalogue files.
const DISCOVER_DIR: &str = "discover";

/// Legacy directory name retained for one-shot migration of users upgrading
/// from a build that stored catalogues under `~/.automatic/marketplace/`.
const LEGACY_DISCOVER_DIR: &str = "marketplace";

const MCP_SERVERS_FILE: &str = "mcp-servers.json";
const COLLECTIONS_FILE: &str = "collections.json";
const TEMPLATES_FILE: &str = "templates.json";
const FEATURED_COMMUNITY_FILE: &str = "featured-community.json";

// ── Compiled-in seed content ──────────────────────────────────────────────────
//
// These strings are embedded in the binary *only* so that `init_discover_files`
// can write them to disk on first run or after a version upgrade.  They are
// never returned directly to callers — the on-disk files are the sole source of
// truth at runtime.  Once the remote endpoint at tryautomatic.app is live,
// `init_discover_files` can be updated to fetch from there instead, and
// these constants can be removed.

const SEED_MCP_SERVERS: &str = include_str!("../../assets/discover/featured-mcp-servers.json");
const SEED_COLLECTIONS: &str = include_str!("../../assets/discover/collections.json");
// Individual template JSON files are held in project_templates::BUNDLED_TEMPLATES;
// we re-export that slice here so the seeding logic can aggregate it without a
// circular dependency.
pub(super) use super::templates::BUNDLED_TEMPLATES;

// ── Directory helpers ─────────────────────────────────────────────────────────

fn get_discover_dir() -> Result<PathBuf, String> {
    Ok(get_automatic_dir()?.join(DISCOVER_DIR))
}

fn mcp_servers_path() -> Result<PathBuf, String> {
    Ok(get_discover_dir()?.join(MCP_SERVERS_FILE))
}

fn collections_path() -> Result<PathBuf, String> {
    Ok(get_discover_dir()?.join(COLLECTIONS_FILE))
}

fn templates_path() -> Result<PathBuf, String> {
    Ok(get_discover_dir()?.join(TEMPLATES_FILE))
}

pub fn featured_community_path() -> Result<PathBuf, String> {
    Ok(get_discover_dir()?.join(FEATURED_COMMUNITY_FILE))
}

// ── Startup seeding ───────────────────────────────────────────────────────────

/// Ensure `~/.automatic/discover/` exists and write the three catalogue
/// files.
///
/// - `force = true`  — overwrite every file (used when the app version changes,
///   matching the same gate used for bundled skill reinstallation).
/// - `force = false` — only write files that are absent (first run).
///
/// Either way the content written comes from the seed constants above; once the
/// remote endpoint is live this is where the fetch will happen instead.
pub fn init_discover_files(force: bool) -> Result<(), String> {
    migrate_legacy_dir()?;

    let dir = get_discover_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| {
            format!(
                "Failed to create discover directory {}: {}",
                dir.display(),
                e
            )
        })?;
    }

    seed_file(&mcp_servers_path()?, SEED_MCP_SERVERS, force)?;
    seed_file(&collections_path()?, SEED_COLLECTIONS, force)?;
    // featured-community.json is not seeded — it is fetched from the remote
    // endpoint on demand and cached to disk.  If the file does not exist and
    // the network is unavailable, the UI shows an empty / "check back" state.

    let templates_json = build_bundled_templates_json()?;
    seed_file(&templates_path()?, &templates_json, force)?;

    Ok(())
}

/// One-shot migration: if the legacy `~/.automatic/marketplace/` directory
/// exists from a prior build but the new `~/.automatic/discover/` directory
/// does not, rename the old directory to the new name so existing on-disk
/// catalogue files (including any user-cached `featured-community.json`) are
/// preserved.  Silently no-ops once migration has completed.
fn migrate_legacy_dir() -> Result<(), String> {
    let new_dir = get_discover_dir()?;
    if new_dir.exists() {
        return Ok(());
    }
    let legacy_dir = get_automatic_dir()?.join(LEGACY_DISCOVER_DIR);
    if !legacy_dir.exists() {
        return Ok(());
    }
    fs::rename(&legacy_dir, &new_dir).map_err(|e| {
        format!(
            "Failed to migrate legacy discover directory from {} to {}: {}",
            legacy_dir.display(),
            new_dir.display(),
            e
        )
    })
}

/// Write `content` to `path`.  Skips if the file already exists and `force` is
/// `false`.
fn seed_file(path: &PathBuf, content: &str, force: bool) -> Result<(), String> {
    if !force && path.exists() {
        return Ok(());
    }
    fs::write(path, content)
        .map_err(|e| format!("Failed to write discover file {}: {}", path.display(), e))
}

/// Aggregate the individual compiled-in template entries into a single JSON
/// array string for writing to `templates.json`.
fn build_bundled_templates_json() -> Result<String, String> {
    let raw_array: Vec<serde_json::Value> = BUNDLED_TEMPLATES
        .iter()
        .map(|(_, raw)| {
            serde_json::from_str::<serde_json::Value>(raw)
                .map_err(|e| format!("Failed to parse bundled template: {}", e))
        })
        .collect::<Result<Vec<_>, _>>()?;

    serde_json::to_string_pretty(&raw_array).map_err(|e| e.to_string())
}

// ── Public readers ────────────────────────────────────────────────────────────
//
// These read exclusively from disk.  The startup seeding above ensures the
// files exist before the UI is shown.  If a file is somehow absent (e.g. the
// user deleted it mid-session) the readers return an empty JSON array so the
// Discover view renders empty rather than crashing.

/// Read `~/.automatic/discover/mcp-servers.json`.
pub fn read_mcp_servers_json() -> Result<String, String> {
    read_json_file(&mcp_servers_path()?)
}

/// Read `~/.automatic/discover/collections.json`.
pub fn read_collections_json() -> Result<String, String> {
    read_json_file(&collections_path()?)
}

/// Read `~/.automatic/discover/templates.json`.
pub fn read_templates_json() -> Result<String, String> {
    read_json_file(&templates_path()?)
}

/// Read `~/.automatic/discover/featured-community.json`.
pub fn read_featured_community_json() -> Result<String, String> {
    read_json_file(&featured_community_path()?)
}

fn read_json_file(path: &PathBuf) -> Result<String, String> {
    if path.exists() {
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))
    } else {
        // File absent — return empty array so the UI renders empty rather than
        // erroring out.  The startup thread will have written the file by the
        // time the user interacts with the Discover view, but this guards the
        // brief window on first launch.
        Ok("[]".to_string())
    }
}
