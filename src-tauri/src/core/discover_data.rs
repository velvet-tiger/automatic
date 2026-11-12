use std::collections::{BTreeSet, HashMap, HashSet};
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
/// Automatic's record of the bundled collection slugs it last wrote. See
/// [`sync_bundled_collections`].
const BUNDLED_COLLECTIONS_RECORD_FILE: &str = "bundled-collections.json";
/// Advisory lock taken around every read-modify-write of the collections. See
/// [`with_collections_lock`].
const COLLECTIONS_LOCK_FILE: &str = "collections.json.lock";
/// Name prefix for where Reinstall Defaults moves a collections file it cannot
/// read. A timestamp follows, so a later reinstall never overwrites an earlier
/// backup.
const UNREADABLE_COLLECTIONS_BACKUP_PREFIX: &str = "collections.json.unreadable-";

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
/// The collections file also holds entries that remote sources add, so it is
/// merged rather than replaced. See [`sync_bundled_collections`]. `force` is
/// for Reinstall Defaults only: it restores bundled collections the user
/// deleted, and replaces a collections file that cannot be read. Startup
/// passes `false`, because a startup run can meet a file another process is
/// writing.
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
    // featured-community.json is not seeded — it is fetched from the remote
    // endpoint on demand and cached to disk.  If the file does not exist and
    // the network is unavailable, the UI shows an empty / "check back" state.

    let templates_json = build_bundled_templates_json()?;
    seed_file_if_changed(&templates_path()?, &templates_json)?;

    // Last, so a collections file that cannot be read does not stop the
    // catalogues above from refreshing.
    with_collections_lock(|| sync_bundled_collections(force))
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

// ── Bundled collections ───────────────────────────────────────────────────────

/// Bring the bundled collections in `collections.json` up to date with this
/// binary, and keep every other entry. Call it only while holding
/// [`with_collections_lock`].
///
/// A bundled slug always belongs to Automatic: remote sources may not install
/// over one (`remote_sources::collection_conflict`), and removing a source
/// puts the bundled copy back. So the merge can replace bundled entries by
/// slug without touching what remote sources or the user added.
///
/// The record file in [`bundled_collections_record_path`] lists the bundled
/// slugs of the last merge. Nothing else tells these cases apart:
/// - A slug in the record that is no longer bundled was retired, and its
///   entry is removed.
/// - A slug in the record that is still bundled but missing from the file was
///   deleted by the user, and stays deleted unless `force` is set.
///
/// An absent file gets the bundled copy as shipped. A file that cannot be read
/// or parsed is an error and is left alone, so entries that remote sources
/// wrote are never lost. With `force` it is moved aside to a backup and
/// replaced with the bundled copy, which is what Reinstall Defaults asks for.
fn sync_bundled_collections(force: bool) -> Result<(), String> {
    let bundled = bundled_collections()?;
    let bundled_slugs = collection_slugs(&bundled);
    let path = collections_path()?;

    match read_existing_collections(&path) {
        Ok(None) => replace_atomically(&path, SEED_COLLECTIONS)?,
        Ok(Some(existing)) => {
            let previous = read_bundled_collections_record()?;
            let retired: HashSet<&str> = previous
                .iter()
                .map(String::as_str)
                .filter(|slug| !bundled_slugs.contains(*slug))
                .collect();
            let deleted: HashSet<&str> = if force {
                HashSet::new()
            } else {
                previous
                    .iter()
                    .map(String::as_str)
                    .filter(|slug| bundled_slugs.contains(*slug))
                    .collect()
            };
            for slug in displaced_slugs(&existing, &bundled, &previous) {
                eprintln!(
                    "[automatic] the bundled collection '{}' replaces an entry with the same slug",
                    slug
                );
            }
            let merged = merge_bundled_collections(&existing, &bundled, &retired, &deleted);
            if merged != existing {
                let json = serde_json::to_string_pretty(&merged)
                    .map_err(|e| format!("Failed to serialize collections: {}", e))?;
                replace_atomically(&path, &json)?;
            }
        }
        Err(e) if force => {
            let backup = unreadable_collections_backup_path(&path)?;
            fs::rename(&path, &backup).map_err(|rename_error| {
                format!(
                    "{}. Failed to move it aside to {}: {}",
                    e,
                    backup.display(),
                    rename_error
                )
            })?;
            eprintln!(
                "[automatic] moved unreadable collections to {} and restored the bundled copy: {}",
                backup.display(),
                e
            );
            replace_atomically(&path, SEED_COLLECTIONS)?;
        }
        Err(e) => return Err(e),
    }

    // Written after the collections, so a failed write above leaves the old
    // record and the next run retries the same removals.
    seed_file_if_changed(
        &bundled_collections_record_path()?,
        &bundled_collections_record(&bundled_slugs)?,
    )
}

/// A backup path beside `path` that does not exist yet, named
/// `collections.json.unreadable-<seconds since the Unix epoch>`, with a
/// counter added if a backup from the same second exists.
fn unreadable_collections_backup_path(path: &std::path::Path) -> Result<PathBuf, String> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("The system clock is before 1970: {}", e))?
        .as_secs();
    let mut candidate = path.with_file_name(format!(
        "{}{}",
        UNREADABLE_COLLECTIONS_BACKUP_PREFIX, seconds
    ));
    let mut counter = 1;
    while fs::symlink_metadata(&candidate).is_ok() {
        candidate = path.with_file_name(format!(
            "{}{}-{}",
            UNREADABLE_COLLECTIONS_BACKUP_PREFIX, seconds, counter
        ));
        counter += 1;
    }
    Ok(candidate)
}

/// Run `body` while holding an exclusive lock on the collections file.
///
/// Startup merges, remote-source installs and remote-source removals all read
/// `collections.json`, change it and write it back. They can run at the same
/// time, in one process or in several (the GUI and each `mcp-serve`). Without
/// the lock, the last writer drops the other's change. The lock is advisory
/// and lives on a separate file, because the collections file itself is
/// replaced by rename on every write. A nested call on the same thread would
/// wait on itself forever, so it is refused with an error instead.
pub(crate) fn with_collections_lock<T>(
    body: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _held = CollectionsLockHeld::enter()?;
    let lock_path = get_discover_dir()?.join(COLLECTIONS_LOCK_FILE);
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
    }
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| format!("Failed to open {}: {}", lock_path.display(), e))?;
    lock_file
        .lock()
        .map_err(|e| format!("Failed to lock {}: {}", lock_path.display(), e))?;
    let result = body();
    // Closing the file releases the lock.
    drop(lock_file);
    result
}

thread_local! {
    static COLLECTIONS_LOCK_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Marks this thread as holding the collections lock until dropped.
struct CollectionsLockHeld;

impl CollectionsLockHeld {
    fn enter() -> Result<Self, String> {
        if COLLECTIONS_LOCK_HELD.with(|held| held.replace(true)) {
            return Err("The collections lock is already held on this thread".to_string());
        }
        Ok(Self)
    }
}

impl Drop for CollectionsLockHeld {
    fn drop(&mut self) {
        COLLECTIONS_LOCK_HELD.with(|held| held.set(false));
    }
}

/// Slugs of entries the merge is about to replace that the last merge did not
/// write: a collection a remote source or the user added under a slug that a
/// newer release now bundles. Empty when there is no record yet, because then
/// every bundled entry looks new.
fn displaced_slugs<'a>(
    existing: &'a [serde_json::Value],
    bundled: &[serde_json::Value],
    previous: &BTreeSet<String>,
) -> Vec<&'a str> {
    if previous.is_empty() {
        return Vec::new();
    }
    existing
        .iter()
        .filter_map(|entry| collection_slug(entry).map(|slug| (slug, entry)))
        .filter(|(slug, entry)| {
            !previous.contains(*slug)
                && bundled
                    .iter()
                    .any(|b| collection_slug(b) == Some(*slug) && b != *entry)
        })
        .map(|(slug, _)| slug)
        .collect()
}

/// The bundled collections, parsed.
fn bundled_collections() -> Result<Vec<serde_json::Value>, String> {
    serde_json::from_str(SEED_COLLECTIONS)
        .map_err(|e| format!("Failed to parse the bundled collections: {}", e))
}

fn collection_slug(entry: &serde_json::Value) -> Option<&str> {
    entry.get("slug").and_then(|v| v.as_str())
}

fn collection_slugs(entries: &[serde_json::Value]) -> BTreeSet<String> {
    entries
        .iter()
        .filter_map(collection_slug)
        .map(str::to_string)
        .collect()
}

/// `existing` with each bundled entry replaced in place by its current copy,
/// bundled entries it lacks appended unless their slug is in `deleted`, and
/// entries whose slug is in `retired` dropped. Every other entry keeps its
/// content and position. A bundled slug that appears more than once keeps
/// only its first position.
fn merge_bundled_collections(
    existing: &[serde_json::Value],
    bundled: &[serde_json::Value],
    retired: &HashSet<&str>,
    deleted: &HashSet<&str>,
) -> Vec<serde_json::Value> {
    let bundled_by_slug: HashMap<&str, &serde_json::Value> = bundled
        .iter()
        .filter_map(|entry| collection_slug(entry).map(|slug| (slug, entry)))
        .collect();
    let mut placed: HashSet<&str> = HashSet::new();
    let mut merged = Vec::with_capacity(existing.len() + bundled.len());

    for entry in existing {
        match collection_slug(entry) {
            Some(slug) if bundled_by_slug.contains_key(slug) => {
                if let Some((slug, bundled_entry)) = bundled_by_slug.get_key_value(slug) {
                    if placed.insert(slug) {
                        merged.push((*bundled_entry).clone());
                    }
                }
            }
            Some(slug) if retired.contains(slug) => {}
            _ => merged.push(entry.clone()),
        }
    }
    for entry in bundled {
        if let Some(slug) = collection_slug(entry) {
            if !deleted.contains(slug) && placed.insert(slug) {
                merged.push(entry.clone());
            }
        }
    }
    merged
}

/// The largest collections file Automatic reads or writes. A real file is a
/// few hundred kilobytes at most. The cap keeps a stray huge file from
/// stalling startup, and [`write_collection_entries`] enforces it so no
/// remote source can grow the file past what the merge will read.
const MAX_COLLECTIONS_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// The entries in the collections file at `path`, or `None` when there is no
/// file. A link, a folder, an oversized file or one that does not parse is an
/// error. A link is not followed, so the merge never writes through one.
fn read_existing_collections(path: &PathBuf) -> Result<Option<Vec<serde_json::Value>>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Failed to inspect {}: {}", path.display(), e)),
    };
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    if metadata.len() > MAX_COLLECTIONS_FILE_BYTES {
        return Err(format!(
            "{} is larger than {} bytes",
            path.display(),
            MAX_COLLECTIONS_FILE_BYTES
        ));
    }
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
}

/// Where the slugs of the last bundled collections written are recorded.
fn bundled_collections_record_path() -> Result<PathBuf, String> {
    Ok(get_discover_dir()?.join(BUNDLED_COLLECTIONS_RECORD_FILE))
}

/// The record file's content for `slugs`: a sorted JSON array, so an
/// unchanged set gives identical bytes and the file is not rewritten.
fn bundled_collections_record(slugs: &BTreeSet<String>) -> Result<String, String> {
    serde_json::to_string_pretty(slugs)
        .map_err(|e| format!("Failed to serialize the bundled collection slugs: {}", e))
}

/// The slugs recorded at the last merge. A missing record is an empty set:
/// installs from before the record existed remove nothing on their first
/// merge. A record that cannot be read is also an empty set, with a warning.
/// It is Automatic's own bookkeeping, so the merge rewrites it, and failing
/// startup over it would stop bundled collections from ever updating.
fn read_bundled_collections_record() -> Result<BTreeSet<String>, String> {
    let path = bundled_collections_record_path()?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(e) => return Err(format!("Failed to inspect {}: {}", path.display(), e)),
    };
    let parsed = if metadata.is_file() && metadata.len() <= MAX_COLLECTIONS_FILE_BYTES {
        fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|raw| serde_json::from_str(&raw).map_err(|e| e.to_string()))
    } else {
        Err("not a regular file of a sensible size".to_string())
    };
    Ok(parsed.unwrap_or_else(|e| {
        eprintln!(
            "[automatic] ignoring the bundled collections record {}: {}",
            path.display(),
            e
        );
        BTreeSet::new()
    }))
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

/// Replace the Discover collections with `entries`. Call it only while
/// holding [`with_collections_lock`], after reading the entries under the same
/// lock. The write is atomic, so a reader never sees a half-written file.
/// A list larger than [`MAX_COLLECTIONS_FILE_BYTES`] is refused.
pub(crate) fn write_collection_entries(entries: &[serde_json::Value]) -> Result<(), String> {
    let path = collections_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
    }
    let json = serde_json::to_string_pretty(entries)
        .map_err(|e| format!("Failed to serialize collections: {}", e))?;
    if json.len() as u64 > MAX_COLLECTIONS_FILE_BYTES {
        return Err(format!(
            "The collections would be larger than {} bytes",
            MAX_COLLECTIONS_FILE_BYTES
        ));
    }
    replace_atomically(&path, &json)
}

/// The bundled collection with `slug`, as the first run writes it, or
/// `None` when no bundled collection has that slug.
pub(crate) fn seed_collection(slug: &str) -> Result<Option<serde_json::Value>, String> {
    Ok(bundled_collections()?
        .into_iter()
        .find(|entry| collection_slug(entry) == Some(slug)))
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

    // ── Bundled collections ──────────────────────────────────────────────────

    use serde_json::json;

    fn slugs_of(entries: &[serde_json::Value]) -> Vec<&str> {
        entries.iter().filter_map(collection_slug).collect()
    }

    fn read_collections_file() -> Vec<serde_json::Value> {
        let raw = fs::read_to_string(collections_path().expect("path")).expect("read");
        serde_json::from_str(&raw).expect("parse")
    }

    fn write_collections_file(entries: &[serde_json::Value]) {
        let path = collections_path().expect("path");
        fs::write(&path, serde_json::to_string_pretty(entries).expect("json")).expect("write");
    }

    fn a_bundled_slug() -> String {
        collection_slug(&bundled_collections().expect("bundled")[0])
            .expect("slug")
            .to_string()
    }

    #[test]
    fn the_bundled_collections_have_unique_slugs() {
        let bundled = bundled_collections().expect("bundled");
        let slugs = slugs_of(&bundled);
        assert_eq!(
            slugs.len(),
            bundled.len(),
            "every bundled collection has a slug"
        );
        let unique: HashSet<&str> = slugs.iter().copied().collect();
        assert_eq!(unique.len(), slugs.len());
    }

    #[test]
    fn merge_replaces_bundled_entries_in_place_and_keeps_the_rest() {
        let existing = vec![
            json!({ "slug": "remote-first" }),
            json!({ "slug": "bundled-a", "name": "Old" }),
            json!({ "name": "no slug" }),
            json!({ "slug": "remote-last" }),
        ];
        let bundled = vec![
            json!({ "slug": "bundled-a", "name": "New" }),
            json!({ "slug": "bundled-b" }),
        ];

        let merged =
            merge_bundled_collections(&existing, &bundled, &HashSet::new(), &HashSet::new());

        assert_eq!(
            merged,
            vec![
                json!({ "slug": "remote-first" }),
                json!({ "slug": "bundled-a", "name": "New" }),
                json!({ "name": "no slug" }),
                json!({ "slug": "remote-last" }),
                json!({ "slug": "bundled-b" }),
            ]
        );
    }

    #[test]
    fn merge_drops_retired_slugs_and_duplicate_bundled_entries() {
        let existing = vec![
            json!({ "slug": "bundled-a" }),
            json!({ "slug": "retired" }),
            json!({ "slug": "users-own" }),
            json!({ "slug": "bundled-a", "name": "Duplicate" }),
        ];
        let bundled = vec![json!({ "slug": "bundled-a", "name": "New" })];
        let retired: HashSet<&str> = ["retired"].into_iter().collect();

        let merged = merge_bundled_collections(&existing, &bundled, &retired, &HashSet::new());

        assert_eq!(
            merged,
            vec![
                json!({ "slug": "bundled-a", "name": "New" }),
                json!({ "slug": "users-own" }),
            ]
        );
    }

    #[test]
    fn the_first_run_writes_the_bundled_collections_and_the_record() {
        with_temp_home(|| {
            init_discover_files(false).expect("seed");

            let path = collections_path().expect("path");
            assert_eq!(fs::read_to_string(&path).expect("read"), SEED_COLLECTIONS);
            let record = read_bundled_collections_record().expect("record");
            assert_eq!(
                record,
                collection_slugs(&bundled_collections().expect("bundled"))
            );
        });
    }

    #[test]
    fn a_new_bundled_collection_reaches_an_existing_install_and_remote_entries_stay() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let bundled_slug = a_bundled_slug();
            // An install from before the record existed, which lacks the
            // bundled collection and holds one that a remote source added.
            fs::remove_file(bundled_collections_record_path().expect("path"))
                .expect("remove record");
            write_collections_file(&[json!({ "slug": "from-a-remote-source", "name": "Kept" })]);

            init_discover_files(false).expect("second seed");

            let entries = read_collections_file();
            assert_eq!(
                entries[0],
                json!({ "slug": "from-a-remote-source", "name": "Kept" })
            );
            assert!(slugs_of(&entries).contains(&bundled_slug.as_str()));
        });
    }

    #[test]
    fn a_changed_bundled_collection_is_updated_without_force() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let bundled_slug = a_bundled_slug();
            write_collections_file(&[
                json!({ "slug": "from-a-remote-source" }),
                json!({ "slug": bundled_slug, "name": "Out of date" }),
            ]);

            init_discover_files(false).expect("second seed");

            let entries = read_collections_file();
            assert_eq!(entries[0], json!({ "slug": "from-a-remote-source" }));
            assert_eq!(
                Some(&entries[1]),
                seed_collection(&bundled_slug).expect("seed").as_ref()
            );
        });
    }

    #[test]
    fn a_retired_bundled_collection_is_removed_but_a_users_own_is_kept() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let mut recorded = read_bundled_collections_record().expect("record");
            recorded.insert("retired-bundled".to_string());
            fs::write(
                bundled_collections_record_path().expect("path"),
                bundled_collections_record(&recorded).expect("record json"),
            )
            .expect("write record");
            let mut entries = read_collections_file();
            entries.push(json!({ "slug": "retired-bundled" }));
            entries.push(json!({ "slug": "users-own" }));
            write_collections_file(&entries);

            init_discover_files(false).expect("second seed");

            let slugs: Vec<String> = slugs_of(&read_collections_file())
                .into_iter()
                .map(str::to_string)
                .collect();
            assert!(!slugs.contains(&"retired-bundled".to_string()));
            assert!(slugs.contains(&"users-own".to_string()));
            let record = read_bundled_collections_record().expect("record");
            assert!(!record.contains("retired-bundled"));
        });
    }

    #[test]
    fn up_to_date_collections_are_not_rewritten() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let mut entries = read_collections_file();
            entries.push(json!({ "slug": "from-a-remote-source" }));
            write_collections_file(&entries);
            let path = collections_path().expect("path");
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
    fn reinstall_defaults_keeps_remote_collections() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let bundled_slug = a_bundled_slug();
            write_collections_file(&[
                json!({ "slug": bundled_slug, "name": "Edited" }),
                json!({ "slug": "from-a-remote-source" }),
            ]);

            init_discover_files(true).expect("forced seed");

            let entries = read_collections_file();
            assert_eq!(
                Some(&entries[0]),
                seed_collection(&bundled_slug).expect("seed").as_ref()
            );
            assert_eq!(entries[1], json!({ "slug": "from-a-remote-source" }));
        });
    }

    #[test]
    fn unreadable_collections_are_an_error_without_force_and_replaced_with_force() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let path = collections_path().expect("path");
            fs::write(&path, "not json").expect("write broken file");

            let error = init_discover_files(false).expect_err("a broken file is an error");
            assert!(error.contains("collections.json"), "{}", error);
            assert_eq!(fs::read_to_string(&path).expect("read"), "not json");

            init_discover_files(true).expect("forced seed");
            assert_eq!(fs::read_to_string(&path).expect("read"), SEED_COLLECTIONS);
            fs::write(&path, "broken again").expect("break it again");
            init_discover_files(true).expect("second forced seed");

            let mut backups: Vec<String> = fs::read_dir(path.parent().expect("parent"))
                .expect("list")
                .map(|entry| entry.expect("entry").path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with(UNREADABLE_COLLECTIONS_BACKUP_PREFIX))
                })
                .map(|p| fs::read_to_string(p).expect("read backup"))
                .collect();
            backups.sort();
            assert_eq!(
                backups,
                vec!["broken again".to_string(), "not json".to_string()]
            );
        });
    }

    #[test]
    fn a_broken_record_is_ignored_and_rewritten() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let record_path = bundled_collections_record_path().expect("path");
            fs::write(&record_path, "not json").expect("break record");

            init_discover_files(false).expect("second seed");

            let record = read_bundled_collections_record().expect("record");
            assert_eq!(
                record,
                collection_slugs(&bundled_collections().expect("bundled"))
            );
        });
    }

    #[test]
    fn a_bundled_collection_the_user_deleted_stays_deleted_until_reinstall() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let bundled_slug = a_bundled_slug();
            let mut entries = read_collections_file();
            entries.retain(|entry| collection_slug(entry) != Some(bundled_slug.as_str()));
            write_collections_file(&entries);

            init_discover_files(false).expect("second seed");
            assert!(!slugs_of(&read_collections_file()).contains(&bundled_slug.as_str()));

            init_discover_files(true).expect("forced seed");
            assert!(slugs_of(&read_collections_file()).contains(&bundled_slug.as_str()));
        });
    }

    #[test]
    fn without_a_record_nothing_is_removed() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            fs::remove_file(bundled_collections_record_path().expect("path"))
                .expect("remove record");
            let mut entries = read_collections_file();
            entries.push(json!({ "slug": "maybe-retired" }));
            write_collections_file(&entries);

            init_discover_files(false).expect("second seed");

            assert!(slugs_of(&read_collections_file()).contains(&"maybe-retired"));
        });
    }

    #[test]
    fn collections_that_are_not_a_list_are_an_error() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let path = collections_path().expect("path");
            for content in ["{}", "null", ""] {
                fs::write(&path, content).expect("write");
                init_discover_files(false).expect_err(content);
                assert_eq!(fs::read_to_string(&path).expect("read"), content);
            }
        });
    }

    #[test]
    fn templates_refresh_even_when_collections_cannot_be_read() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            fs::write(collections_path().expect("path"), "not json").expect("break");
            let templates = templates_path().expect("path");
            fs::write(&templates, "[]").expect("stale templates");

            init_discover_files(false).expect_err("collections are unreadable");

            let expected = build_bundled_templates_json().expect("templates json");
            assert_eq!(fs::read_to_string(&templates).expect("read"), expected);
        });
    }

    #[test]
    fn a_record_that_cannot_be_written_is_an_error_after_the_merge() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let record = bundled_collections_record_path().expect("path");
            fs::remove_file(&record).expect("remove record");
            fs::create_dir(&record).expect("folder in its place");
            fs::write(record.join("inside"), "x").expect("fill folder");
            write_collections_file(&[json!({ "slug": "from-a-remote-source" })]);

            init_discover_files(false).expect_err("the record cannot be written");

            let slugs = slugs_of(&read_collections_file())
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>();
            assert!(slugs.contains(&"from-a-remote-source".to_string()));
            assert!(slugs.contains(&a_bundled_slug()));
        });
    }

    #[test]
    fn displaced_slugs_name_entries_the_last_merge_did_not_write() {
        let bundled = vec![
            json!({ "slug": "old" }),
            json!({ "slug": "new", "name": "B" }),
        ];
        let existing = vec![
            json!({ "slug": "old", "name": "Edited" }),
            json!({ "slug": "new", "name": "From a source" }),
        ];
        let previous: BTreeSet<String> = ["old".to_string()].into_iter().collect();

        assert_eq!(displaced_slugs(&existing, &bundled, &previous), vec!["new"]);
        assert!(displaced_slugs(&existing, &bundled, &BTreeSet::new()).is_empty());
    }

    #[test]
    fn oversized_collections_are_not_written() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let huge = "x".repeat(MAX_COLLECTIONS_FILE_BYTES as usize);
            let error = write_collection_entries(&[json!({ "slug": "big", "pad": huge })])
                .expect_err("too large");
            assert!(error.contains("larger than"), "{}", error);
            assert_eq!(
                fs::read_to_string(collections_path().expect("path")).expect("read"),
                SEED_COLLECTIONS
            );
        });
    }

    #[test]
    fn a_nested_collections_lock_is_an_error_not_a_hang() {
        with_temp_home(|| {
            init_discover_files(false).expect("seed");
            let inner =
                with_collections_lock(|| Ok(with_collections_lock(|| Ok(())))).expect("outer lock");
            assert!(inner.is_err());
            with_collections_lock(|| Ok(())).expect("released after the outer call");
        });
    }

    #[test]
    fn the_collections_lock_makes_a_second_writer_wait() {
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().to_path_buf();
        with_test_home(home.clone(), || {
            init_discover_files(false).expect("seed");
            let (locked_tx, locked_rx) = mpsc::channel();
            let holder = std::thread::spawn(move || {
                with_test_home(home, || {
                    with_collections_lock(|| {
                        locked_tx.send(()).expect("signal");
                        std::thread::sleep(Duration::from_millis(300));
                        Ok(())
                    })
                })
            });
            locked_rx.recv().expect("holder has the lock");

            let started = Instant::now();
            with_collections_lock(|| Ok(())).expect("second lock");

            assert!(started.elapsed() >= Duration::from_millis(200));
            holder.join().expect("join").expect("holder");
        });
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_place_of_the_collections_is_not_followed() {
        with_temp_home(|| {
            init_discover_files(false).expect("first seed");
            let path = collections_path().expect("path");
            let elsewhere = path.with_file_name("elsewhere.json");
            fs::write(&elsewhere, "[]").expect("write link target");
            fs::remove_file(&path).expect("remove collections");
            std::os::unix::fs::symlink(&elsewhere, &path).expect("plant link");

            init_discover_files(false).expect_err("a link is not read");
            init_discover_files(true).expect("forced seed");

            assert_eq!(fs::read_to_string(&elsewhere).expect("read"), "[]");
            assert!(fs::symlink_metadata(&path).expect("metadata").is_file());
        });
    }

    #[test]
    fn the_bundled_catalogue_lists_context_dev() {
        let servers: Vec<serde_json::Value> =
            serde_json::from_str(SEED_MCP_SERVERS).expect("parse catalogue");
        assert!(servers.iter().any(|s| s["slug"] == "context-dev"));
    }
}
