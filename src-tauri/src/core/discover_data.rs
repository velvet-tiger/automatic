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
// can write them to disk at startup (see that function for when each is written).  They are
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
/// The MCP server and template catalogues are copies of content compiled into
/// this binary, and nothing else writes them. They are rewritten whenever they
/// differ from the bundled content, whatever `force` says. A version gate is
/// not enough: the startup gate follows the bundled library version, which
/// does not change when only the app's catalogue does, so new catalogue
/// entries would never reach existing installs.
///
/// The collections file also holds entries that remote sources add, so it
/// follows `force`:
/// - `force = true`  — overwrite it (a newer bundled library, or Reinstall
///   Defaults).
/// - `force = false` — only write it when it is absent (first run).
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

    seed_file_if_changed(&mcp_servers_path()?, SEED_MCP_SERVERS)?;
    seed_file(&collections_path()?, SEED_COLLECTIONS, force)?;
    // featured-community.json is not seeded — it is fetched from the remote
    // endpoint on demand and cached to disk.  If the file does not exist and
    // the network is unavailable, the UI shows an empty / "check back" state.

    let templates_json = build_bundled_templates_json()?;
    seed_file_if_changed(&templates_path()?, &templates_json)?;

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

/// Write `content` to `path` unless the file already holds exactly that
/// content. Only for files that are pure copies of bundled content: any other
/// content in the file is replaced.
///
/// The existing file is read only when it is a regular file of the expected
/// length, so a FIFO or a link to a huge file cannot stall startup. The new
/// copy is renamed into place, which replaces a link instead of writing
/// through it, and readers in other processes never see a half-written file.
fn seed_file_if_changed(path: &PathBuf, content: &str) -> Result<(), String> {
    if holds_exactly(path, content)? {
        return Ok(());
    }
    replace_atomically(path, content)
}

/// Whether `path` is a regular file whose bytes equal `content`. Does not
/// follow a link: a link is never a match, so it gets replaced.
fn holds_exactly(path: &PathBuf, content: &str) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => {
            return Err(format!(
                "Failed to inspect discover file {}: {}",
                path.display(),
                e
            ))
        }
    };
    if !metadata.is_file() || metadata.len() != content.len() as u64 {
        return Ok(false);
    }
    let existing = fs::read(path)
        .map_err(|e| format!("Failed to read discover file {}: {}", path.display(), e))?;
    Ok(existing == content.as_bytes())
}

/// Write `content` to a temporary file beside `path`, then rename it over
/// `path`.
fn replace_atomically(path: &PathBuf, content: &str) -> Result<(), String> {
    use std::io::Write;

    let parent = path
        .parent()
        .ok_or_else(|| format!("Discover file {} has no parent folder", path.display()))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| {
        format!(
            "Failed to create a temporary file in {}: {}",
            parent.display(),
            e
        )
    })?;
    temp.write_all(content.as_bytes())
        .and_then(|()| temp.as_file().sync_all())
        .map_err(|e| {
            format!(
                "Failed to write a temporary copy of {}: {}",
                path.display(),
                e
            )
        })?;
    temp.persist(path).map_err(|e| {
        format!(
            "Failed to replace discover file {}: {}",
            path.display(),
            e.error
        )
    })?;
    Ok(())
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

/// Read the Discover collections as a list of entries. A missing file is an
/// empty list. A file that does not parse is an error, so a caller that
/// writes the list back never replaces entries it could not read.
pub(crate) fn read_collection_entries() -> Result<Vec<serde_json::Value>, String> {
    let path = collections_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    serde_json::from_str(&raw).map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
}

/// Replace the Discover collections with `entries`.
pub(crate) fn write_collection_entries(entries: &[serde_json::Value]) -> Result<(), String> {
    let path = collections_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
    }
    let json = serde_json::to_string_pretty(entries)
        .map_err(|e| format!("Failed to serialize collections: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

/// The bundled collection with `slug`, as the first run writes it, or
/// `None` when no bundled collection has that slug.
pub(crate) fn seed_collection(slug: &str) -> Result<Option<serde_json::Value>, String> {
    let seeds: Vec<serde_json::Value> = serde_json::from_str(SEED_COLLECTIONS)
        .map_err(|e| format!("Failed to parse the bundled collections: {}", e))?;
    Ok(seeds
        .into_iter()
        .find(|entry| entry.get("slug").and_then(|v| v.as_str()) == Some(slug)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::with_test_home;

    fn with_temp_home<T>(test: impl FnOnce() -> T) -> T {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), test)
    }

    #[test]
    fn a_stale_mcp_catalogue_is_replaced_without_force() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let path = mcp_servers_path().expect("path");
            fs::write(&path, "[]").expect("write stale catalogue");

            init_discover_files(false).expect("second seed");

            assert_eq!(fs::read_to_string(&path).expect("read"), SEED_MCP_SERVERS);
        });
    }

    #[test]
    fn stale_templates_are_replaced_without_force() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let path = templates_path().expect("path");
            fs::write(&path, "[]").expect("write stale templates");

            init_discover_files(false).expect("second seed");

            let expected = build_bundled_templates_json().expect("templates json");
            assert_eq!(fs::read_to_string(&path).expect("read"), expected);
        });
    }

    #[test]
    fn an_up_to_date_mcp_catalogue_is_not_rewritten() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let path = mcp_servers_path().expect("path");
            let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1);
            fs::File::options()
                .write(true)
                .open(&path)
                .expect("open")
                .set_modified(old)
                .expect("set mtime");

            init_discover_files(false).expect("second seed");

            let modified = fs::metadata(&path)
                .expect("metadata")
                .modified()
                .expect("mtime");
            assert_eq!(modified, old);
        });
    }

    #[test]
    fn up_to_date_templates_are_not_rewritten() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let path = templates_path().expect("path");
            let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1);
            fs::File::options()
                .write(true)
                .open(&path)
                .expect("open")
                .set_modified(old)
                .expect("set mtime");

            init_discover_files(false).expect("second seed");

            let modified = fs::metadata(&path)
                .expect("metadata")
                .modified()
                .expect("mtime");
            assert_eq!(modified, old);
        });
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_place_of_the_catalogue_is_replaced_not_written_through() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let path = mcp_servers_path().expect("path");
            let elsewhere = path.with_file_name("elsewhere.txt");
            fs::write(&elsewhere, "keep me").expect("write link target");
            fs::remove_file(&path).expect("remove catalogue");
            std::os::unix::fs::symlink(&elsewhere, &path).expect("plant link");

            init_discover_files(false).expect("second seed");

            assert_eq!(fs::read_to_string(&elsewhere).expect("read"), "keep me");
            let metadata = fs::symlink_metadata(&path).expect("metadata");
            assert!(metadata.is_file());
            assert_eq!(fs::read_to_string(&path).expect("read"), SEED_MCP_SERVERS);
        });
    }

    #[test]
    fn a_catalogue_that_cannot_be_replaced_is_an_error() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let path = mcp_servers_path().expect("path");
            fs::remove_file(&path).expect("remove catalogue");
            fs::create_dir(&path).expect("put a folder in its place");
            fs::write(path.join("inside"), "x").expect("fill the folder");

            let error = init_discover_files(false).expect_err("a folder cannot be replaced");

            assert!(error.contains("mcp-servers.json"), "{}", error);
        });
    }

    #[test]
    fn reinstall_defaults_restores_a_stale_catalogue() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let path = mcp_servers_path().expect("path");
            fs::write(&path, "[]").expect("write stale catalogue");

            crate::core::reinstall_defaults().expect("reinstall defaults");

            assert_eq!(fs::read_to_string(&path).expect("read"), SEED_MCP_SERVERS);
        });
    }

    #[test]
    fn collections_are_kept_without_force() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let path = collections_path().expect("path");
            fs::write(&path, "[]").expect("write collections");

            init_discover_files(false).expect("second seed");
            assert_eq!(fs::read_to_string(&path).expect("read"), "[]");

            init_discover_files(true).expect("forced seed");
            assert_eq!(fs::read_to_string(&path).expect("read"), SEED_COLLECTIONS);
        });
    }

    #[test]
    fn the_bundled_catalogue_lists_context_dev() {
        let servers: Vec<serde_json::Value> =
            serde_json::from_str(SEED_MCP_SERVERS).expect("parse catalogue");
        assert!(servers.iter().any(|s| s["slug"] == "context-dev"));
    }
}
