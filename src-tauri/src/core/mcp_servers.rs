use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::env_crypto;
use super::paths::{get_library_dir, is_valid_name};
use super::recently_added::{record_recently_added, remove_recently_added};

// ── MCP Servers ──────────────────────────────────────────────────────────────

pub fn get_mcp_servers_dir() -> Result<PathBuf, String> {
    Ok(get_library_dir()?.join("mcp_servers"))
}

/// Names of every server stored in the registry, sorted.
///
/// A registry directory that does not exist yet lists as empty. Every other
/// failure is an error, never a shorter list. Cloud sync deletes a server
/// from the cloud library when it is missing from this list, and project
/// sync drops it from agent config files.
pub fn list_mcp_server_configs() -> Result<Vec<String>, String> {
    let dir = get_mcp_servers_dir()?;

    // Absence is decided by the read itself. `Path::exists` also reports
    // false when the directory is there but cannot be reached.
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(format!(
                "Failed to read the MCP server registry '{}': {}",
                dir.display(),
                e
            ))
        }
    };

    let mut servers = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| {
            format!(
                "Failed to read an entry of the MCP server registry '{}': {}",
                dir.display(),
                e
            )
        })?;
        let path = entry.path();
        // Only an entry named like a server config is inspected, so an
        // unrelated file cannot fail the listing.
        let Some(name) = mcp_server_name_from_path(&path) else {
            continue;
        };
        // `fs::metadata` follows symlinks, as `Path::is_file` does.
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => servers.push(name.to_string()),
            // Not a regular file, for example a directory named `x.json`.
            Ok(_) => {}
            // A dangling symlink, or a file deleted during the listing. The
            // reader reports the same entry as not stored.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Failed to inspect MCP server '{}': {}", name, e)),
        }
    }

    servers.sort();
    Ok(servers)
}

/// The server name a registry path stores a config for, or `None` when the
/// path is not named like a server config (`<valid name>.json`).
fn mcp_server_name_from_path(path: &Path) -> Option<&str> {
    if !path.extension().is_some_and(|ext| ext == "json") {
        return None;
    }
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| is_valid_name(stem))
}

/// Read a single MCP server config from disk and return it as a JSON string
/// with env values **decrypted** (plaintext) so the frontend and sync engine
/// see regular strings.
///
/// Returns an error when no config is stored under `name`, as well as when
/// the read fails. Callers that must tell the two apart use
/// [`read_mcp_server_config_if_present`].
pub fn read_mcp_server_config(name: &str) -> Result<String, String> {
    read_mcp_server_config_if_present(name)?
        .ok_or_else(|| format!("MCP server '{}' not found", name))
}

/// Read a single MCP server config, or `None` when no config is stored under
/// `name`. The JSON string is the one [`read_mcp_server_config`] returns.
///
/// A failed read is an error here, never `None`. Decrypting env values needs
/// the env-encryption key, so a locked keychain or a denied keychain prompt
/// fails the read of a server that still exists. Use this wherever a missing
/// server leads to a removal or an overwrite.
pub fn read_mcp_server_config_if_present(name: &str) -> Result<Option<String>, String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }
    let dir = get_mcp_servers_dir()?;
    let path = dir.join(format!("{}.json", name));

    // Absence is decided by the read itself. `Path::exists` also reports
    // false when the file is there but cannot be reached.
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let mut config: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid JSON in config: {}", e))?;

    // Decrypt env values if present.
    if let Some(env) = config.get_mut("env") {
        env_crypto::decrypt_env_values(env)?;
    }

    if config.get("_author").is_none() {
        if let Some(repo) = super::remote_sources::get_display_provenance("mcp_server", name) {
            if let Some(obj) = config.as_object_mut() {
                obj.insert(
                    "_author".to_string(),
                    serde_json::json!({
                        "name": repo,
                        "repository_url": format!("https://github.com/{}", repo),
                    }),
                );
            }
        }
    }

    serde_json::to_string(&config)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// Write a registry file for `name` whose env value carries the encryption
/// prefix but is not valid ciphertext, and return its path.
///
/// Tests use a fixed in-memory env-encryption key, so they cannot make the
/// key itself unavailable. Reading this file fails in `decrypt_env_values`,
/// which is where a read fails when the key cannot be loaded.
#[cfg(test)]
pub(crate) fn write_undecryptable_mcp_server_config(name: &str) -> PathBuf {
    let dir = get_mcp_servers_dir().expect("mcp servers dir");
    fs::create_dir_all(&dir).expect("create mcp servers dir");
    let path = dir.join(format!("{}.json", name));
    fs::write(
        &path,
        r#"{"command":"npx","env":{"API_KEY":"enc:v1:not-a-real-ciphertext"}}"#,
    )
    .expect("write undecryptable config");
    path
}

/// Persist a single MCP server config.  Env values are **encrypted** before
/// writing so that API keys are never stored in plaintext on disk.
///
/// The `data` parameter is the raw JSON string from the frontend (env values
/// are plaintext at this point — the frontend never sees the encrypted form).
///
/// A save that changes the server's `url` deletes the OAuth secrets it stops
/// using, as [`tidy_oauth_secrets_on_url_change`] describes.
pub fn save_mcp_server_config(name: &str, data: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }

    let mut config: serde_json::Value =
        serde_json::from_str(data).map_err(|e| format!("Invalid JSON: {}", e))?;

    // Encrypt env values before writing to disk.
    if let Some(env) = config.get_mut("env") {
        env_crypto::encrypt_env_values(env)?;
    }

    let dir = get_mcp_servers_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let path = dir.join(format!("{}.json", name));
    let is_new = !path.exists();
    tidy_oauth_secrets_on_url_change(name, &config)?;
    let serialized = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(&path, serialized).map_err(|e| e.to_string())?;

    if is_new {
        record_recently_added("mcp_servers", name);
    }

    Ok(())
}

/// Store a config for a server that must be new, and refuse to replace any
/// existing file.
///
/// The file is opened with `create_new`, so a server written between the
/// caller's name check and this write is never overwritten. On a
/// case-insensitive file system, a case variant of an existing name fails
/// here too. Otherwise this stores the config as [`save_mcp_server_config`]
/// does: env values are encrypted and stale OAuth secrets are tidied.
pub(crate) fn create_mcp_server_config(name: &str, data: &str) -> Result<(), String> {
    use std::io::Write;

    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }
    let mut config: serde_json::Value =
        serde_json::from_str(data).map_err(|e| format!("Invalid JSON: {}", e))?;
    if let Some(env) = config.get_mut("env") {
        env_crypto::encrypt_env_values(env)?;
    }
    let serialized = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    let dir = get_mcp_servers_dir()?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", name));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => {
                format!("MCP server '{}' already exists in the library", name)
            }
            _ => format!("Failed to create MCP server '{}': {}", name, e),
        })?;
    // The file was created empty here, so nothing of another server's is
    // tidied away. Any failure removes the file this call created.
    let written = tidy_oauth_secrets_on_url_change(name, &config)
        .and_then(|()| file.write_all(serialized.as_bytes()).map_err(|e| e.to_string()));
    if let Err(e) = written {
        let _ = fs::remove_file(&path);
        return Err(format!("Failed to write MCP server '{}': {}", name, e));
    }
    record_recently_added("mcp_servers", name);
    Ok(())
}

/// Delete the OAuth secrets the server `name` stops using when `config` is
/// written as its registry file. Call this before the write.
///
/// This is tidying. A secret is filed under the server's name and the URL it
/// was issued for, so a token issued for the old URL is never found for the
/// new one, whether or not anything is deleted here. What is deleted:
///
/// - the secrets for the old URL, which nothing will ask for again;
/// - any secrets already filed for the new URL, so a server does not start
///   out signed in as an earlier server of the same name and URL;
/// - secrets an older version stored under the name alone, which were issued
///   for a URL the server no longer has.
///
/// A write that keeps the URL makes no call to the secret store. A failure in
/// the store is logged and never returned. The error here is a registry file
/// that could not be read.
///
/// [`save_mcp_server_config`] calls this, which covers the editor, a cloud
/// sync pull, a plugin update, an import from an agent file and a remote
/// source install.
pub(crate) fn tidy_oauth_secrets_on_url_change(
    name: &str,
    config: &serde_json::Value,
) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }
    record_legacy_oauth_urls()?;
    let path = get_mcp_servers_dir()?.join(format!("{}.json", name));
    let new_url = comparable_server_url(config);
    let old_url = match stored_server_url(&path)? {
        StoredUrl::Known(old_url) if old_url == new_url => return Ok(()),
        StoredUrl::NotStored if new_url.is_none() => return Ok(()),
        StoredUrl::Known(old_url) => old_url,
        StoredUrl::NotStored | StoredUrl::Unknown => None,
    };

    if old_url.is_none() && new_url.is_none() {
        crate::proxy::forget_legacy_oauth_secrets(name);
    }
    for url in [old_url, new_url].into_iter().flatten() {
        crate::proxy::forget_oauth_secrets(name, &url);
    }
    Ok(())
}

/// What the registry file at a path says about its server's URL.
enum StoredUrl {
    /// No file is stored there.
    NotStored,
    /// The file is not valid JSON, so it may or may not have had a URL.
    Unknown,
    /// The file's `url` in comparable form, or `None` when it has none.
    Known(Option<String>),
}

/// Read the URL of the registry file at `path`. Env values are not
/// decrypted, so this never reads the secret store.
fn stored_server_url(path: &Path) -> Result<StoredUrl, String> {
    let stored = match fs::read_to_string(path) {
        Ok(stored) => stored,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(StoredUrl::NotStored),
        Err(e) => return Err(e.to_string()),
    };
    Ok(match serde_json::from_str::<serde_json::Value>(&stored) {
        Ok(stored) => StoredUrl::Known(comparable_server_url(&stored)),
        Err(_) => StoredUrl::Unknown,
    })
}

/// The URL saved for the server `name`, in comparable form. `None` when the
/// server is not stored or has no URL.
///
/// A server's OAuth secrets are filed under this URL. A command that is given
/// only a server name reads it here. Env values are not decrypted, so this
/// never reads the secret store.
pub fn saved_mcp_server_url(name: &str) -> Result<Option<String>, String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }
    let path = get_mcp_servers_dir()?.join(format!("{}.json", name));
    match stored_server_url(&path)? {
        StoredUrl::NotStored => Ok(None),
        StoredUrl::Known(url) => Ok(url),
        StoredUrl::Unknown => Err(format!("The config of MCP server '{}' is not valid JSON", name)),
    }
}

/// Whether `url` is the URL saved for the server `name`. False when the
/// server is not stored or has no URL.
///
/// Sign-in asks this before it starts. A token for a URL that is not saved
/// would be filed where nothing looks for it.
pub fn mcp_server_url_is(name: &str, url: &str) -> Result<bool, String> {
    let Some(saved_url) = saved_mcp_server_url(name)? else {
        return Ok(false);
    };
    Ok(comparable_mcp_url(url) == Some(saved_url))
}

// ── The URLs that secrets from older versions were issued for ───────────────
//
// Older versions filed a server's OAuth secrets under its name alone, so
// nothing in the secret store says which URL they were issued for. The best
// evidence is the URL the server had before this version changed anything.
// It is written to a file once, the first time it is needed, and never
// changed. An old secret is then only ever used for the URL recorded for its
// server. A server created later, or one whose URL has changed since, does
// not get it, whether or not the old secret could be deleted.

/// The file that records the URL each server had when this version first ran.
fn legacy_oauth_urls_path() -> Result<PathBuf, String> {
    Ok(super::paths::get_automatic_dir()?.join("mcp_oauth_legacy_urls.json"))
}

/// Record the URL of every stored server, unless that has been done already.
///
/// Saving, deleting and renaming a server each call this first, so the record
/// describes the registry as an older version left it. The first writer
/// wins. A second process that gets here at the same time has read the same
/// registry.
///
/// An error stops the caller from changing the registry. A record written
/// after the change would name the new URL as the one an old secret was
/// issued for.
pub(crate) fn record_legacy_oauth_urls() -> Result<(), String> {
    let path = legacy_oauth_urls_path()?;
    if path.exists() {
        return Ok(());
    }
    let record = serde_json::to_string_pretty(&registry_urls()?).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // Written under another name and then linked into place, so no reader
    // sees half a file and an existing record is never replaced.
    let draft = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&draft, &record)
        .map_err(|e| format!("Failed to write {}: {}", draft.display(), e))?;
    let linked = fs::hard_link(&draft, &path);
    let _ = fs::remove_file(&draft);
    match linked {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        // Some filesystems have no hard links.
        Err(_) => write_new_file(&path, &record),
    }
}

/// Write `contents` to `path` unless a file is already there, which is
/// success. A reader may see the file before it is complete. It then fails
/// to parse the record and uses no old secret.
///
/// A file that could not be written in full is removed. Left in place it
/// would count as the record for good, and it would never parse.
fn write_new_file(path: &Path, contents: &str) -> Result<(), String> {
    use std::io::Write;

    match fs::OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => file.write_all(contents.as_bytes()).map_err(|e| {
            let _ = fs::remove_file(path);
            format!("Failed to write {}: {}", path.display(), e)
        }),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(format!("Failed to create {}: {}", path.display(), e)),
    }
}

/// The comparable URL of every stored server that has one, by server name.
///
/// An entry that cannot be read is left out, with a warning. A directory
/// named like a config, or a file that is not text, must not stop every
/// later save. A server left out gets no old secret, which is the safe side.
fn registry_urls() -> Result<BTreeMap<String, String>, String> {
    let dir = get_mcp_servers_dir()?;
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(format!("Failed to read {}: {}", dir.display(), e)),
    };
    let mut urls = BTreeMap::new();
    for entry in entries {
        let path = entry
            .map_err(|e| format!("Failed to read {}: {}", dir.display(), e))?
            .path();
        let Some(name) = mcp_server_name_from_path(&path) else {
            continue;
        };
        match stored_server_url(&path) {
            Ok(StoredUrl::Known(Some(url))) => {
                urls.insert(name.to_string(), url);
            }
            Ok(_) => {}
            Err(e) => eprintln!(
                "warning: could not read {} to record its URL, so no OAuth token from an older version will be used for it: {}",
                path.display(),
                e
            ),
        }
    }
    Ok(urls)
}

/// Whether `url` is the URL the server `name` had when this version first
/// ran, which is the URL its secrets from an older version were issued for.
///
/// False for a server that did not exist then or had no URL. A record that
/// cannot be read is an error, and the old secrets then go unused.
pub fn legacy_oauth_url_is(name: &str, url: &str) -> Result<bool, String> {
    record_legacy_oauth_urls()?;
    let path = legacy_oauth_urls_path()?;
    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let urls: BTreeMap<String, String> = serde_json::from_str(&raw)
        .map_err(|e| format!("Invalid JSON in {}: {}", path.display(), e))?;
    Ok(urls.get(name).is_some_and(|recorded| Some(recorded) == comparable_mcp_url(url).as_ref()))
}

/// The `url` of a server config in comparable form, or `None` when the config
/// has none.
fn comparable_server_url(config: &serde_json::Value) -> Option<String> {
    comparable_mcp_url(config.get("url")?.as_str()?)
}

/// A server URL in the form two URLs are compared in, or `None` when it is
/// empty.
///
/// Parsing puts the scheme and host in lower case, drops a default port and
/// gives an empty path a `/`, so tidying a URL does not sign the user out. A
/// value that does not parse as a URL is compared as written.
///
/// OAuth secrets are filed under a hash of this form. Changing what it
/// returns for a URL would sign every user out of every server.
pub fn comparable_mcp_url(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    Some(match url::Url::parse(url) {
        Ok(parsed) => parsed.to_string(),
        Err(_) => url.to_string(),
    })
}

/// Add an MCP server config read from an agent's own config file to the
/// registry, unless a server with that name is already there.
///
/// An agent file inside a project is not a trusted source: the repository
/// may be someone else's.  A discovered entry may introduce a new server, but
/// it must never replace one the user already has, because every project and
/// user-level assignment that lists the name would pick up its URL and
/// headers on the next sync.  Names are compared without regard to case, as
/// the rest of the registry does.
///
/// The destination file is also checked directly.  The filesystem may treat
/// two names as the same file where a string comparison does not (Unicode
/// normalisation and non-ASCII case on macOS), and writing would then replace
/// the existing entry.
///
/// A config the sync guard would block is stored with the discovered marker,
/// as [`mark_discovered_if_blocked`] describes.
///
/// An env value that looks like Automatic's own ciphertext is stored empty,
/// as [`blank_ciphertext_like_env_values`] describes.
pub fn add_discovered_mcp_server_config(name: &str, config_json: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }
    if super::contains_ignore_ascii_case(&list_mcp_server_configs()?, name)
        || get_mcp_servers_dir()?.join(format!("{}.json", name)).exists()
    {
        return Ok(());
    }
    // Blanked first, so the config that is validated is the one stored.
    // Validation reads env key names only, and blanking keeps every key.
    let config_json = blank_ciphertext_like_env_values(config_json);
    save_mcp_server_config(name, &super::mark_discovered_if_blocked(&config_json))
}

/// Replace every `env` value that carries the encryption prefix with an
/// empty string, in a config that did not come from this install's registry:
/// one discovered in an agent file, or one a remote source provides.  The key
/// is kept.
///
/// Saving a config leaves a prefixed value as it is, because it is taken for
/// a value that is already encrypted.  A prefixed value from an agent file is
/// not ciphertext this install can read: sync writes decrypted values to
/// agent files, so they never hold Automatic's ciphertext.  Nor is one from a
/// remote source's repository, which was written on another machine.  Stored
/// as it is, the registry entry could never be decrypted, and cloud sync and
/// the project's own sync would stop on it until the user deleted the server.
///
/// An empty value means "inherit from the environment".  The value is not
/// encrypted as plain text instead.  It would then read back as a plain
/// string with the prefix, and the next save would store it unencrypted and
/// make the entry unreadable again.
///
/// Returns the input unchanged when it is not JSON, has no `env` object, or
/// has no prefixed value, leaving the save path to report any error.
pub(crate) fn blank_ciphertext_like_env_values(config_json: &str) -> String {
    let Ok(mut config) = serde_json::from_str::<serde_json::Value>(config_json) else {
        return config_json.to_string();
    };
    let Some(env) = config.get_mut("env").and_then(|env| env.as_object_mut()) else {
        return config_json.to_string();
    };

    let mut blanked = false;
    for value in env.values_mut() {
        if let serde_json::Value::String(text) = value {
            if env_crypto::is_encrypted(text) {
                text.clear();
                blanked = true;
            }
        }
    }
    if !blanked {
        return config_json.to_string();
    }
    config.to_string()
}

/// Delete a single MCP server config, and the OAuth secrets stored for it.
///
/// Deleting the secrets is tidying, as [`crate::proxy::forget_oauth_secrets`]
/// explains. A failure there is logged and the config is still deleted. A
/// server with no URL has no secrets, and is deleted without a call to the
/// store.
///
/// Callers are responsible for removing the name from projects, profiles and
/// global selections.
pub fn delete_mcp_server_config(name: &str) -> Result<(), String> {
    delete_mcp_server_config_reporting(name).map(|_| ())
}

/// Like [`delete_mcp_server_config`], and return a description of each
/// OAuth secret the store would not delete, for a caller that reports them.
pub fn delete_mcp_server_config_reporting(name: &str) -> Result<Vec<String>, String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }
    record_legacy_oauth_urls()?;
    let dir = get_mcp_servers_dir()?;
    let path = dir.join(format!("{}.json", name));
    let secret_problems = match stored_server_url(&path)? {
        StoredUrl::Known(Some(url)) => crate::proxy::forget_oauth_secrets_reporting(name, &url),
        StoredUrl::Known(None) => Vec::new(),
        // The server may have had a URL once, under an older version.
        StoredUrl::NotStored | StoredUrl::Unknown => {
            crate::proxy::forget_legacy_oauth_secrets_reporting(name)
        }
    };

    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }

    remove_recently_added("mcp_servers", name);

    Ok(secret_problems)
}

/// Rename a local MCP server config on disk. Callers are responsible for
/// updating any project or template that referenced the old name, and must
/// do so whenever this returns `Ok`, warnings or not.
///
/// Refuses built-in server names and refuses to overwrite an existing config
/// under `new_name`. If `old_name == new_name` this is a no-op.
///
/// Any provenance entry recorded against `old_name` is moved to `new_name` so
/// the GitHub author badge continues to show. A provenance file that cannot
/// be read refuses the rename before anything moves. A provenance write that
/// fails after the move does not fail the rename, because the file has moved
/// and the caller still has references to rewrite. It is returned as a
/// warning naming both servers.
///
/// The server's OAuth secrets are filed under its name and URL, so they move
/// to `new_name` once the file has. Left behind, the renamed server would
/// look signed out. A failed move is logged and does not fail the rename, as
/// [`crate::proxy::move_oauth_secrets`] explains. A server with no URL is
/// renamed without a call to the store.
pub fn rename_mcp_server_config(old_name: &str, new_name: &str) -> Result<Vec<String>, String> {
    if !is_valid_name(old_name) || !is_valid_name(new_name) {
        return Err("Invalid server name".into());
    }
    if old_name == new_name {
        return Ok(Vec::new());
    }
    if is_builtin_mcp_server(old_name) {
        return Err("Built-in servers cannot be renamed".into());
    }

    let dir = get_mcp_servers_dir()?;
    let old_path = dir.join(format!("{}.json", old_name));
    let new_path = dir.join(format!("{}.json", new_name));

    if !old_path.exists() {
        return Err(format!("MCP server '{}' not found", old_name));
    }
    if new_path.exists() {
        return Err(format!("MCP server '{}' already exists", new_name));
    }

    // Read ownership strictly before anything moves. A damaged provenance
    // file would otherwise lose the server's source on rename, so the rename
    // is refused and nothing on disk changes.
    let source = super::remote_sources::get_provenance("mcp_server", old_name).map_err(|e| {
        format!(
            "MCP server '{}' was not renamed, because its source ownership could not be read: {}",
            old_name, e
        )
    })?;

    record_legacy_oauth_urls()?;
    let url = stored_server_url(&old_path)?;

    fs::rename(&old_path, &new_path).map_err(|e| e.to_string())?;

    if let StoredUrl::Known(Some(url)) = url {
        crate::proxy::move_oauth_secrets(old_name, new_name, &url);
    }

    remove_recently_added("mcp_servers", old_name);

    let mut warnings = Vec::new();
    if source.is_some() {
        if let Err(e) = super::remote_sources::move_provenance("mcp_server", old_name, new_name) {
            warnings.push(format!(
                "MCP server '{}' was renamed to '{}', but its source ownership could not be moved: {}",
                old_name, new_name, e
            ));
        }
    }

    Ok(warnings)
}

/// Reported availability of a configured MCP server, shown as a status
/// indicator on the project's MCP tab.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct McpServerAvailability {
    pub available: bool,
    pub message: Option<String>,
}

/// Check whether a configured MCP server currently looks available.
///
/// For `stdio` servers this resolves the configured command against an
/// absolute/relative path or the `PATH` environment variable — the same
/// resolution a shell does — without spawning the process. For `http` and
/// `sse` servers it sends a lightweight HTTP request to the configured URL;
/// any response, even an error status, counts as available, since the
/// question being answered is reachability, not authentication (OAuth
/// token validity already has its own indicator).
pub async fn check_mcp_server_status(name: &str) -> Result<McpServerAvailability, String> {
    let raw = read_mcp_server_config(name)?;
    let config: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid config for '{}': {}", name, e))?;

    let transport = config
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("stdio");

    match transport {
        "stdio" => Ok(check_stdio_command_available(
            config.get("command").and_then(|v| v.as_str()),
        )),
        "http" | "sse" => {
            check_http_endpoint_available(
                config.get("url").and_then(|v| v.as_str()),
                config.get("headers"),
            )
            .await
        }
        other => Ok(McpServerAvailability {
            available: false,
            message: Some(format!("Unknown transport '{}'", other)),
        }),
    }
}

/// Check whether a raw stdio command string (e.g. `npx`, `uvx`,
/// `/usr/local/bin/mcp-server`) resolves to an executable — the same
/// resolution `check_mcp_server_status` performs for a saved server,
/// but exposed for the editor UI so the user is warned before saving
/// that the dependency their MCP server relies on isn't installed.
pub fn check_mcp_command_available(command: &str) -> McpServerAvailability {
    check_stdio_command_available(Some(command))
}

fn check_stdio_command_available(command: Option<&str>) -> McpServerAvailability {
    let Some(command) = command.map(str::trim).filter(|c| !c.is_empty()) else {
        return McpServerAvailability {
            available: false,
            message: Some("No command configured.".to_string()),
        };
    };

    match resolve_command_path(command) {
        Some(path) => McpServerAvailability {
            available: true,
            message: Some(format!("Resolved to {}", path.display())),
        },
        None => McpServerAvailability {
            available: false,
            message: Some(format!(
                "Command '{}' was not found on disk or on PATH.",
                command
            )),
        },
    }
}

/// Resolves `command` the way a shell would: a path containing a separator
/// is checked directly, otherwise every directory on `PATH` is searched.
/// This only checks the file exists and is executable — it never spawns
/// the process, so it stays cheap enough to run on every tab load.
fn resolve_command_path(command: &str) -> Option<PathBuf> {
    let candidate = std::path::Path::new(command);
    if command.contains(std::path::MAIN_SEPARATOR) || candidate.is_absolute() {
        return is_executable_file(candidate).then(|| candidate.to_path_buf());
    }

    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let full = dir.join(command);
        if is_executable_file(&full) {
            return Some(full);
        }
        #[cfg(windows)]
        for ext in ["exe", "cmd", "bat"] {
            let with_ext = dir.join(format!("{command}.{ext}"));
            if is_executable_file(&with_ext) {
                return Some(with_ext);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable_file(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &std::path::Path) -> bool {
    path.is_file()
}

async fn check_http_endpoint_available(
    url: Option<&str>,
    headers: Option<&serde_json::Value>,
) -> Result<McpServerAvailability, String> {
    let Some(url) = url.map(str::trim).filter(|u| !u.is_empty()) else {
        return Ok(McpServerAvailability {
            available: false,
            message: Some("No URL configured.".to_string()),
        });
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e))?;

    let mut request = client.head(url);
    if let Some(header_map) = headers.and_then(|h| h.as_object()) {
        for (key, value) in header_map {
            if let Some(value_str) = value.as_str() {
                request = request.header(key.as_str(), value_str);
            }
        }
    }

    match request.send().await {
        Ok(response) => Ok(McpServerAvailability {
            available: true,
            message: Some(format!(
                "Responded with HTTP {}",
                response.status().as_u16()
            )),
        }),
        Err(e) => Ok(McpServerAvailability {
            available: false,
            message: Some(format!("Not reachable: {}", e)),
        }),
    }
}

/// Read raw Claude Desktop config.
///
/// Uses [`dirs::config_dir`] to resolve the platform-specific configuration
/// directory so the import works on macOS, Linux **and** Windows:
///
/// | Platform | Path                                                                |
/// |----------|---------------------------------------------------------------------|
/// | macOS    | `~/Library/Application Support/Claude/claude_desktop_config.json`   |
/// | Linux    | `~/.config/Claude/claude_desktop_config.json`                       |
/// | Windows  | `%APPDATA%\Claude\claude_desktop_config.json`                       |
pub fn list_mcp_servers() -> Result<String, String> {
    let config_dir = dirs::config_dir().ok_or("Could not determine config directory")?;
    let config_path = config_dir.join("Claude/claude_desktop_config.json");

    if config_path.exists() {
        fs::read_to_string(config_path).map_err(|e| e.to_string())
    } else {
        Ok("{}".to_string())
    }
}

/// The well-known name used for the Automatic MCP server everywhere:
/// registry files, project assignments, and agent config files.
pub const AUTOMATIC_SERVER_NAME: &str = "automatic";

/// The well-known name of the bundled "automatic" skill that teaches agents
/// how to use the Automatic MCP service.  Always assigned to every project.
pub const AUTOMATIC_SKILL_NAME: &str = "automatic";

/// Resolve the path of the Automatic binary that gets written into MCP
/// configs (the registry entry and every per-project agent config file).
///
/// `std::env::current_exe()` returns the *invocation* path on macOS: a
/// process spawned through the CLI symlink (`/usr/local/bin/automatic`) sees
/// the symlink itself, while the GUI process sees the app-bundle binary —
/// two different strings for the same file. Writing whichever string the
/// current process happened to be invoked by made the GUI and
/// symlink-spawned `mcp-serve` processes perpetually overwrite each other's
/// configs. Instead:
///
/// 1. Canonicalize `current_exe()` so the invocation alias is stripped.
/// 2. Prefer the stable CLI symlink when it resolves to this same binary —
///    it survives app moves/updates, and every process that resolves through
///    this function then emits the identical string.
pub fn automatic_binary_path() -> String {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return "automatic".to_string(),
    };
    let canonical = std::fs::canonicalize(&exe).unwrap_or(exe);

    #[cfg(unix)]
    {
        for candidate in unix_cli_symlink_candidates() {
            if std::fs::canonicalize(&candidate)
                .map(|c| c == canonical)
                .unwrap_or(false)
            {
                return candidate.display().to_string();
            }
        }

        // A quarantined launch runs the app from an ephemeral App
        // Translocation mount (/private/var/folders/…/AppTranslocation/…).
        // That path dies with the mount — persisting it into MCP configs
        // leaves every agent spawning a binary that no longer exists
        // (ENOENT).  The alias check above also fails here, because the CLI
        // symlink resolves to the real install location rather than the
        // mount.  Prefer any live CLI symlink over ever emitting the
        // translocated path.
        if canonical
            .components()
            .any(|c| c.as_os_str() == "AppTranslocation")
        {
            for candidate in unix_cli_symlink_candidates() {
                if std::fs::canonicalize(&candidate).is_ok() {
                    return candidate.display().to_string();
                }
            }
        }
    }

    canonical.display().to_string()
}

/// The locations `cli_install` may have placed the `automatic` CLI symlink.
/// Kept in sync with `cli_install::unix::preferred_install_path`.
#[cfg(unix)]
fn unix_cli_symlink_candidates() -> Vec<std::path::PathBuf> {
    let mut candidates = vec![std::path::PathBuf::from("/usr/local/bin/automatic")];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".local").join("bin").join("automatic"));
    }
    candidates
}

/// True when two binary path strings refer to the same file. Falls back to
/// literal comparison when canonicalisation fails (e.g. a stale path whose
/// target no longer exists — that is a genuine change).
fn same_automatic_binary(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

/// Ensure the `automatic` MCP server entry is present in the Automatic
/// registry and assigned to all projects.
///
/// **Registry entry** — makes the server visible in the Automatic UI (MCP
/// Servers list and per-project MCP selector).  The entry is always
/// overwritten so the binary path stays current after updates.
///
/// **Project assignment** — adds `"automatic"` to every registered project's
/// `mcp_servers` list if not already present, then persists the project.
///
/// **Returns** the identifiers (see `project_ident`) of all projects that
/// have a configured directory and at least one agent.  The caller is responsible for re-syncing these
/// projects in the background so that:
/// - The `automatic` MCP server entry in each agent config file reflects the
///   current binary path (which changes between dev builds and release).
/// - Any newly added `automatic` skill files are written to disk.
///
/// The MCP server is exposed to agents via per-project config files written
/// during agent sync (e.g. `.mcp.json` for Claude Code).  We intentionally
/// do NOT write to the global `~/.mcp.json` or the plugin `.mcp.json` —
/// having multiple registrations of the same server causes Claude Code to
/// deduplicate and drop tools.
///
/// The binary path is resolved from the current executable so it always
/// reflects the installed release binary rather than a hard-coded path.
pub fn ensure_automatic_in_global_mcp() -> Result<Vec<String>, String> {
    let binary = automatic_binary_path();

    // ── 1. Read old registry entry to detect binary path change ──────────
    //
    // If the binary path has changed (e.g. dev→release or after an update),
    // every project that writes the automatic server entry needs a re-sync.
    // Compared via `same_automatic_binary`, not string equality: the GUI
    // process and a symlink-spawned `mcp-serve` see different `current_exe()`
    // strings for the *same* binary, and treating that as a change made every
    // spawn re-sync (and rewrite files in) every project.
    let binary_changed = read_mcp_server_config(AUTOMATIC_SERVER_NAME)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| {
            v.get("command")
                .and_then(|c| c.as_str())
                .map(|c| !same_automatic_binary(c, &binary))
        })
        .unwrap_or(true); // no existing entry → treat as changed

    // ── 2. Registry entry ────────────────────────────────────────────────
    let registry_config = serde_json::json!({
        "type": "stdio",
        "command": binary,
        "args": ["mcp-serve"],
        "_builtin": true
    });
    let registry_str = serde_json::to_string_pretty(&registry_config).map_err(|e| e.to_string())?;
    // save_mcp_server_config handles directory creation and env encryption.
    save_mcp_server_config(AUTOMATIC_SERVER_NAME, &registry_str)?;

    // ── 3. Assign MCP server + skill to all projects, collect sync candidates
    let mut projects_to_sync: Vec<String> = Vec::new();

    if let Ok(project_idents) = super::list_project_idents() {
        for name in project_idents {
            if let Ok(raw) = super::read_project(&name) {
                if let Ok(mut project) = serde_json::from_str::<super::Project>(&raw) {
                    // Nothing may be written into a read-only project.
                    if project.is_read_only_here() {
                        continue;
                    }
                    let mut changed = false;

                    if !project
                        .mcp_servers
                        .iter()
                        .any(|s| s == AUTOMATIC_SERVER_NAME)
                    {
                        project.mcp_servers.push(AUTOMATIC_SERVER_NAME.to_string());
                        changed = true;
                    }

                    if !project.skills.iter().any(|s| s == AUTOMATIC_SKILL_NAME) {
                        project.skills.push(AUTOMATIC_SKILL_NAME.to_string());
                        changed = true;
                    }

                    if changed {
                        if let Ok(updated) = serde_json::to_string_pretty(&project) {
                            let _ = super::save_project(&name, &updated);
                        }
                    }

                    // Queue for re-sync if:
                    // - We just added automatic server/skill (changed = true), OR
                    // - The binary path changed and this project has agents that
                    //   would write agent config files containing the path.
                    let has_syncable_config = !project.directory.is_empty()
                        && !project.agents.is_empty()
                        && std::path::Path::new(&project.directory).exists();

                    if has_syncable_config && (changed || binary_changed) {
                        projects_to_sync.push(name);
                    }
                }
            }
        }
    }

    Ok(projects_to_sync)
}

/// Returns `true` if the given MCP server name is a built-in server that
/// should not be deleted or have its core config edited by the user.
pub fn is_builtin_mcp_server(name: &str) -> bool {
    name == AUTOMATIC_SERVER_NAME
}

/// Returns `true` if the given skill name is a built-in skill that should
/// not be deleted or removed from projects by the user.
pub fn is_builtin_skill(name: &str) -> bool {
    name == AUTOMATIC_SKILL_NAME
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::with_test_home;
    use std::path::Path;
    use tempfile::TempDir;

    // ── Path-injectable helpers ───────────────────────────────────────────────

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn save_at(dir: &Path, name: &str, data: &str) -> Result<(), String> {
        if !is_valid_name(name) {
            return Err("Invalid server name".into());
        }
        let mut config: serde_json::Value =
            serde_json::from_str(data).map_err(|e| format!("Invalid JSON: {}", e))?;
        if let Some(env) = config.get_mut("env") {
            env_crypto::encrypt_env_values(env)?;
        }
        if !dir.exists() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let path = dir.join(format!("{}.json", name));
        let serialized = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
        fs::write(path, serialized).map_err(|e| e.to_string())
    }

    fn read_at(dir: &Path, name: &str) -> Result<String, String> {
        if !is_valid_name(name) {
            return Err("Invalid server name".into());
        }
        let path = dir.join(format!("{}.json", name));
        if !path.exists() {
            return Err(format!("MCP server '{}' not found", name));
        }
        let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let mut config: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| format!("Invalid JSON: {}", e))?;
        if let Some(env) = config.get_mut("env") {
            env_crypto::decrypt_env_values(env)?;
        }
        serde_json::to_string(&config).map_err(|e| e.to_string())
    }

    fn list_at(dir: &Path) -> Result<Vec<String>, String> {
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut servers = Vec::new();
        let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if is_valid_name(stem) {
                            servers.push(stem.to_string());
                        }
                    }
                }
            }
        }
        Ok(servers)
    }

    fn delete_at(dir: &Path, name: &str) -> Result<(), String> {
        if !is_valid_name(name) {
            return Err("Invalid server name".into());
        }
        let path = dir.join(format!("{}.json", name));
        if path.exists() {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    // ── list ─────────────────────────────────────────────────────────────────

    #[test]
    fn list_returns_empty_when_dir_missing() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers"); // not created
        let names = list_at(&dir).expect("list");
        assert!(names.is_empty());
    }

    #[test]
    fn list_returns_saved_server_names() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        let config = r#"{"command": "npx", "args": ["-y", "some-server"]}"#;

        save_at(&dir, "server-a", config).expect("save a");
        save_at(&dir, "server-b", config).expect("save b");

        let mut names = list_at(&dir).expect("list");
        names.sort();
        assert_eq!(names, vec!["server-a", "server-b"]);
    }

    // ── save + read roundtrip ────────────────────────────────────────────────

    #[test]
    fn save_and_read_config_without_env() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        let config = r#"{"command": "npx", "args": ["-y", "my-server"]}"#;

        save_at(&dir, "my-server", config).expect("save");
        let raw = read_at(&dir, "my-server").expect("read");
        let val: serde_json::Value = serde_json::from_str(&raw).expect("parse");

        assert_eq!(val["command"].as_str().unwrap(), "npx");
    }

    #[test]
    fn env_values_are_encrypted_at_rest() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        let config = r#"{"command": "npx", "env": {"API_KEY": "my-secret"}}"#;

        save_at(&dir, "secure-server", config).expect("save");

        // Read raw bytes — should NOT contain the plaintext secret.
        let raw = fs::read_to_string(dir.join("secure-server.json")).expect("read raw");
        assert!(
            !raw.contains("my-secret"),
            "plaintext secret must not be stored on disk"
        );
        assert!(
            raw.contains("enc:v1:"),
            "encrypted sentinel should be present on disk"
        );
    }

    #[test]
    fn read_decrypts_env_values_to_plaintext() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        let config = r#"{"command": "npx", "env": {"API_KEY": "my-secret"}}"#;

        save_at(&dir, "secure-server", config).expect("save");
        let raw = read_at(&dir, "secure-server").expect("read");
        let val: serde_json::Value = serde_json::from_str(&raw).expect("parse");

        assert_eq!(
            val["env"]["API_KEY"].as_str().unwrap(),
            "my-secret",
            "env values should be decrypted on read"
        );
    }

    #[test]
    fn double_save_does_not_double_encrypt() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        let config = r#"{"command": "npx", "env": {"KEY": "value"}}"#;

        save_at(&dir, "srv", config).expect("first save");
        // Read decrypted, then save again (simulating a frontend re-save).
        let decrypted = read_at(&dir, "srv").expect("read");
        save_at(&dir, "srv", &decrypted).expect("second save");

        // Should still decrypt correctly.
        let result = read_at(&dir, "srv").expect("re-read");
        let val: serde_json::Value = serde_json::from_str(&result).expect("parse");
        assert_eq!(val["env"]["KEY"].as_str().unwrap(), "value");
    }

    // ── delete ───────────────────────────────────────────────────────────────

    #[test]
    fn delete_removes_config_file() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        save_at(&dir, "to-delete", r#"{"command": "node"}"#).expect("save");
        assert!(dir.join("to-delete.json").exists());

        delete_at(&dir, "to-delete").expect("delete");
        assert!(!dir.join("to-delete.json").exists());
    }

    #[test]
    fn delete_is_idempotent_for_missing_server() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        delete_at(&dir, "ghost").expect("delete non-existent should not error");
    }

    // ── list: empty versus unreadable ────────────────────────────────────────

    /// The registry directory under the current test home, created.
    fn registry_dir() -> PathBuf {
        let dir = get_mcp_servers_dir().expect("dir");
        fs::create_dir_all(&dir).expect("create registry dir");
        dir
    }

    #[test]
    fn a_registry_that_does_not_exist_yet_lists_as_empty() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            assert!(!get_mcp_servers_dir().expect("dir").exists());
            assert_eq!(list_mcp_server_configs(), Ok(Vec::new()));
        });
    }

    #[test]
    fn the_listing_is_sorted_and_holds_only_server_configs() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let config = r#"{"command":"npx"}"#;
            save_mcp_server_config("list-zeta", config).expect("save");
            save_mcp_server_config("list-alpha", config).expect("save");
            let dir = registry_dir();
            fs::write(dir.join("notes.txt"), "not a server").expect("write");
            fs::write(dir.join("no-extension"), "{}").expect("write");
            // The stem `.` is not a valid server name.
            fs::write(dir.join("..json"), "{}").expect("write");

            assert_eq!(
                list_mcp_server_configs(),
                Ok(vec!["list-alpha".to_string(), "list-zeta".to_string()])
            );
        });
    }

    #[test]
    fn a_registry_path_that_is_not_a_directory_is_an_error() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let dir = get_mcp_servers_dir().expect("dir");
            fs::create_dir_all(dir.parent().expect("parent")).expect("create parent");
            fs::write(&dir, "a file where the registry directory belongs").expect("write");

            let err = list_mcp_server_configs().expect_err("the listing must fail");
            assert!(
                err.contains(&dir.display().to_string()),
                "error names the registry path: {}",
                err
            );
        });
    }

    #[test]
    fn a_directory_named_like_a_server_is_not_listed() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            save_mcp_server_config("list-real", r#"{"command":"npx"}"#).expect("save");
            fs::create_dir_all(registry_dir().join("list-folder.json")).expect("mkdir");

            assert_eq!(list_mcp_server_configs(), Ok(vec!["list-real".to_string()]));
        });
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_is_not_listed_and_a_live_one_is() {
        let home = tmp();
        let outside = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let dir = registry_dir();
            let target = outside.path().join("target.json");
            fs::write(&target, r#"{"command":"npx"}"#).expect("write target");
            std::os::unix::fs::symlink(&target, dir.join("list-linked.json")).expect("symlink");
            std::os::unix::fs::symlink(
                outside.path().join("never-created.json"),
                dir.join("list-dangling.json"),
            )
            .expect("dangling symlink");

            assert_eq!(
                list_mcp_server_configs(),
                Ok(vec!["list-linked".to_string()])
            );
            // The reader agrees with the listing about the dangling entry.
            assert_eq!(read_mcp_server_config_if_present("list-dangling"), Ok(None));
        });
    }

    #[cfg(unix)]
    #[test]
    fn a_server_entry_that_cannot_be_inspected_is_an_error() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let dir = registry_dir();
            // A symlink to itself: the entry exists but following it fails.
            let looped = dir.join("list-looped.json");
            std::os::unix::fs::symlink(&looped, &looped).expect("symlink loop");

            let err = list_mcp_server_configs().expect_err("the listing must fail");
            assert!(
                err.contains("list-looped"),
                "error names the server: {}",
                err
            );
        });
    }

    /// Puts a directory's permissions back when dropped, so the temp dir can
    /// be removed even when an assertion fails first.
    #[cfg(unix)]
    struct RestorePermissions {
        path: PathBuf,
        permissions: fs::Permissions,
    }

    #[cfg(unix)]
    impl Drop for RestorePermissions {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.path, self.permissions.clone());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_registry_that_cannot_be_read_is_an_error_not_an_empty_list() {
        use std::os::unix::fs::PermissionsExt;

        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            save_mcp_server_config("list-hidden", r#"{"command":"npx"}"#).expect("save");
            let dir = registry_dir();
            let _restore = RestorePermissions {
                path: dir.clone(),
                permissions: fs::metadata(&dir).expect("metadata").permissions(),
            };
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o000)).expect("chmod");

            // Root reads a directory whatever its mode, so the failure cannot
            // be produced and there is nothing to assert.
            if fs::read_dir(&dir).is_ok() {
                return;
            }

            let err = list_mcp_server_configs().expect_err("the listing must fail");
            assert!(
                err.contains(&dir.display().to_string()),
                "error names the registry path: {}",
                err
            );
        });
    }

    /// `Path::exists` reports false for a directory under a parent that
    /// cannot be searched, so a listing that asks `exists` first returns an
    /// empty list here.
    #[cfg(unix)]
    #[test]
    fn a_registry_under_an_unsearchable_parent_is_an_error_not_an_empty_list() {
        use std::os::unix::fs::PermissionsExt;

        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            save_mcp_server_config("list-unreachable", r#"{"command":"npx"}"#).expect("save");
            let dir = registry_dir();
            let parent = dir.parent().expect("registry parent").to_path_buf();
            let _restore = RestorePermissions {
                path: parent.clone(),
                permissions: fs::metadata(&parent).expect("metadata").permissions(),
            };
            fs::set_permissions(&parent, fs::Permissions::from_mode(0o000)).expect("chmod");

            // Root reaches a directory whatever its parent's mode, so the
            // failure cannot be produced and there is nothing to assert.
            if fs::read_dir(&dir).is_ok() {
                return;
            }

            let err = list_mcp_server_configs().expect_err("the listing must fail");
            assert!(
                err.contains(&dir.display().to_string()),
                "error names the registry path: {}",
                err
            );
        });
    }

    // ── read: missing versus unreadable ──────────────────────────────────────

    #[test]
    fn a_missing_server_reads_as_none() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            assert_eq!(read_mcp_server_config_if_present("reader-absent"), Ok(None));
            assert_eq!(
                read_mcp_server_config("reader-absent"),
                Err("MCP server 'reader-absent' not found".to_string())
            );
        });
    }

    #[test]
    fn a_stored_server_reads_with_its_env_decrypted() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let config = r#"{"command":"npx","env":{"API_KEY":"plain-secret"}}"#;
            save_mcp_server_config("reader-stored", config).expect("save");

            let raw = read_mcp_server_config_if_present("reader-stored")
                .expect("read")
                .expect("server is stored");
            let parsed: serde_json::Value = serde_json::from_str(&raw).expect("parse");
            assert_eq!(parsed["env"]["API_KEY"], "plain-secret");
            assert_eq!(read_mcp_server_config("reader-stored"), Ok(raw));
        });
    }

    #[test]
    fn a_server_that_cannot_be_decrypted_is_an_error_not_a_missing_server() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            write_undecryptable_mcp_server_config("reader-locked");

            let typed = read_mcp_server_config_if_present("reader-locked");
            assert!(typed.is_err(), "got {:?}", typed);

            let plain = read_mcp_server_config("reader-locked").expect_err("read must fail");
            assert!(
                !plain.contains("not found"),
                "a failed read must not look like a missing server: {}",
                plain
            );
        });
    }

    #[test]
    fn a_server_holding_invalid_json_is_an_error_not_a_missing_server() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let dir = get_mcp_servers_dir().expect("dir");
            fs::create_dir_all(&dir).expect("create dir");
            fs::write(dir.join("reader-corrupt.json"), "{ not json").expect("write");

            let typed = read_mcp_server_config_if_present("reader-corrupt");
            assert!(typed.is_err(), "got {:?}", typed);
        });
    }

    #[test]
    fn an_invalid_name_is_an_error_not_a_missing_server() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            assert_eq!(
                read_mcp_server_config_if_present("../escape"),
                Err("Invalid server name".to_string())
            );
        });
    }

    /// Two invocation aliases of the same file (e.g. the CLI symlink vs the
    /// app-bundle binary) must compare equal; a dead path must not.
    #[cfg(unix)]
    #[test]
    fn same_automatic_binary_resolves_symlink_aliases() {
        let dir = tempfile::tempdir().expect("tempdir");
        let real = dir.path().join("real-binary");
        std::fs::write(&real, b"bin").expect("write real");
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).expect("symlink");

        assert!(
            same_automatic_binary(alias.to_str().unwrap(), real.to_str().unwrap()),
            "a symlink alias of the same binary must not count as a path change"
        );
        assert!(
            !same_automatic_binary(real.to_str().unwrap(), "/stale/path/automatic"),
            "a dead path is a genuine change"
        );
    }

    /// A registry entry holding a symlink *alias* of the currently running
    /// binary is not a path change — flagging it as one made every
    /// `mcp-serve` spawn re-sync (and rewrite files in) every project,
    /// because the GUI and symlink-spawned processes see different
    /// `current_exe()` strings for the same file.
    #[cfg(unix)]
    #[test]
    fn ensure_automatic_in_global_mcp_ignores_symlink_alias_of_current_binary() {
        let home = tempfile::tempdir().expect("tempdir");
        let project_dir = tempfile::tempdir().expect("project dir");
        let alias_dir = tempfile::tempdir().expect("alias dir");

        // Symlink alias pointing at the real current test binary.
        let exe = std::env::current_exe().expect("current exe");
        let alias = alias_dir.path().join("automatic");
        std::os::unix::fs::symlink(&exe, &alias).expect("symlink to current exe");

        with_test_home(home.path().to_path_buf(), || {
            let seeded = serde_json::json!({
                "type": "stdio",
                "command": alias.to_str().unwrap(),
                "args": ["mcp-serve"],
                "_builtin": true
            });
            save_mcp_server_config(AUTOMATIC_SERVER_NAME, &seeded.to_string())
                .expect("seed alias registry entry");

            // Project already fully assigned so `changed` stays false and the
            // only thing that could queue it is a (spurious) binary change.
            let project = super::super::Project {
                name: "demo".to_string(),
                directory: project_dir.path().to_string_lossy().to_string(),
                agents: vec!["claude".to_string()],
                mcp_servers: vec![AUTOMATIC_SERVER_NAME.to_string()],
                skills: vec![AUTOMATIC_SKILL_NAME.to_string()],
                ..Default::default()
            };
            let serialized = serde_json::to_string_pretty(&project).expect("serialize project");
            super::super::save_project("demo", &serialized).expect("save project");

            let projects_to_sync =
                ensure_automatic_in_global_mcp().expect("ensure automatic entry");
            assert!(
                projects_to_sync.is_empty(),
                "an alias of the current binary must not queue re-syncs, got {:?}",
                projects_to_sync
            );
        });
    }

    #[test]
    fn ensure_automatic_in_global_mcp_detects_stale_binary_path_and_flags_resync() {
        let home = tempfile::tempdir().expect("tempdir");
        let project_dir = tempfile::tempdir().expect("project dir");

        with_test_home(home.path().to_path_buf(), || {
            // Seed a registry entry whose command path can never match the
            // real test binary's `current_exe()`, simulating what's left
            // behind after the app moves or updates.
            save_mcp_server_config(
                AUTOMATIC_SERVER_NAME,
                r#"{"type":"stdio","command":"/stale/path/automatic","args":["mcp-serve"],"_builtin":true}"#,
            )
            .expect("seed stale registry entry");

            let project = super::super::Project {
                name: "demo".to_string(),
                directory: project_dir.path().to_string_lossy().to_string(),
                agents: vec!["claude".to_string()],
                mcp_servers: vec![AUTOMATIC_SERVER_NAME.to_string()],
                ..Default::default()
            };
            let serialized = serde_json::to_string_pretty(&project).expect("serialize project");
            super::super::save_project("demo", &serialized).expect("save project");

            let projects_to_sync =
                ensure_automatic_in_global_mcp().expect("ensure automatic entry");
            assert!(
                projects_to_sync.contains(&"demo".to_string()),
                "a project referencing a stale automatic binary path should be queued for re-sync, got {:?}",
                projects_to_sync
            );

            let updated_registry =
                read_mcp_server_config(AUTOMATIC_SERVER_NAME).expect("read updated registry entry");
            let value: serde_json::Value =
                serde_json::from_str(&updated_registry).expect("parse registry entry");
            assert_ne!(
                value["command"].as_str(),
                Some("/stale/path/automatic"),
                "registry entry should be repaired to the current binary path, not left stale"
            );
        });
    }

    // ── MCP server availability checks ──────────────────────────────────────

    #[test]
    fn resolve_command_path_finds_absolute_executable_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let script = tmp.path().join("fake-server");
        fs::write(&script, "#!/bin/sh\necho hi\n").expect("write script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
        }

        let resolved = resolve_command_path(script.to_str().expect("utf8 path"));
        assert_eq!(resolved.as_deref(), Some(script.as_path()));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_command_path_rejects_non_executable_absolute_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let script = tmp.path().join("not-executable");
        // fs::write creates the file without the executable bit set.
        fs::write(&script, "just data").expect("write file");

        assert!(resolve_command_path(script.to_str().expect("utf8 path")).is_none());
    }

    #[test]
    fn resolve_command_path_finds_bare_command_on_path() {
        // "sh" is present on PATH in every unix dev/CI environment this runs in.
        assert!(resolve_command_path("sh").is_some());
    }

    #[test]
    fn resolve_command_path_returns_none_for_unknown_bare_command() {
        assert!(resolve_command_path("definitely-not-a-real-command-xyz123").is_none());
    }

    #[test]
    fn check_stdio_command_available_reports_missing_command_as_unavailable() {
        let status = check_stdio_command_available(None);
        assert!(!status.available);
        assert!(status.message.is_some());
    }

    #[test]
    fn check_mcp_command_available_reports_known_command_as_available() {
        let status = check_mcp_command_available("sh");
        assert!(status.available);
    }

    #[test]
    fn check_mcp_command_available_reports_unknown_command_as_unavailable() {
        let status = check_mcp_command_available("definitely-not-a-real-command-xyz123");
        assert!(!status.available);
        assert!(status.message.is_some());
    }

    #[tokio::test]
    async fn check_http_endpoint_available_treats_any_response_as_available() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local listener");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                use tokio::io::AsyncWriteExt;
                let _ = stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                    .await;
            }
        });

        let url = format!("http://{}/", addr);
        let status = check_http_endpoint_available(Some(&url), None)
            .await
            .expect("check status");
        assert!(status.available);
    }

    #[tokio::test]
    async fn check_http_endpoint_available_reports_unreachable_url_as_unavailable() {
        // Port 1 is a privileged port essentially never bound to in dev/CI
        // environments, so a connection attempt reliably fails.
        let status = check_http_endpoint_available(Some("http://127.0.0.1:1/"), None)
            .await
            .expect("check status");
        assert!(!status.available);
    }

    #[tokio::test]
    async fn check_http_endpoint_available_reports_missing_url() {
        let status = check_http_endpoint_available(None, None)
            .await
            .expect("check status");
        assert!(!status.available);
    }

    #[test]
    fn read_config_hydrates_remote_author_from_provenance() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            save_mcp_server_config("remote-server", r#"{"type":"stdio","command":"npx"}"#)
                .expect("save config");
            super::super::remote_sources::record_provenance(
                "mcp_server",
                "remote-server",
                "octocat/remote-servers",
            )
            .expect("record provenance");

            let raw = read_mcp_server_config("remote-server").expect("read config");
            let value: serde_json::Value = serde_json::from_str(&raw).expect("parse config");

            assert_eq!(
                value["_author"]["repository_url"].as_str(),
                Some("https://github.com/octocat/remote-servers")
            );
        });
    }

    #[test]
    fn read_config_survives_a_corrupt_provenance_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            save_mcp_server_config("plain-server", r#"{"type":"stdio","command":"npx"}"#)
                .expect("save config");
            // A truncated write, as an interrupted save used to leave behind.
            let provenance = crate::core::paths::get_automatic_dir()
                .expect("automatic dir")
                .join("source-provenance.json");
            fs::write(&provenance, r#"{"mcp_server:plain-server": "octo"#)
                .expect("write corrupt provenance");

            let raw = read_mcp_server_config("plain-server").expect("read config");
            let value: serde_json::Value = serde_json::from_str(&raw).expect("parse config");

            assert_eq!(value["command"].as_str(), Some("npx"));
            assert!(value.get("_author").is_none());
            assert!(
                super::super::remote_sources::get_provenance("mcp_server", "plain-server").is_err(),
                "the strict lookup must still report the damaged file"
            );
        });
    }

    // ── invalid name handling ────────────────────────────────────────────────

    #[test]
    fn save_with_empty_name_returns_error() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        let result = save_at(&dir, "", r#"{"command": "node"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn read_returns_error_for_missing_server() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        let result = read_at(&dir, "nonexistent");
        assert!(result.is_err());
    }

    // ── non-string env values ignored ────────────────────────────────────────

    #[test]
    fn non_string_env_values_are_passed_through_unchanged() {
        let tmp = tmp();
        let dir = tmp.path().join("mcp_servers");
        // env contains a number — must not be altered by encrypt/decrypt.
        let config = r#"{"command": "node", "env": {"PORT": "8080"}}"#;
        save_at(&dir, "with-port", config).expect("save");
        let raw = read_at(&dir, "with-port").expect("read");
        let val: serde_json::Value = serde_json::from_str(&raw).expect("parse");
        assert_eq!(val["env"]["PORT"].as_str().unwrap(), "8080");
    }

    // ── rename ───────────────────────────────────────────────────────────────

    #[test]
    fn rename_moves_config_file_and_preserves_body() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            save_mcp_server_config("old-name", r#"{"type":"stdio","command":"npx"}"#)
                .expect("save");

            rename_mcp_server_config("old-name", "new-name").expect("rename");

            assert!(read_mcp_server_config("old-name").is_err());
            let raw = read_mcp_server_config("new-name").expect("read new");
            let value: serde_json::Value = serde_json::from_str(&raw).expect("parse");
            assert_eq!(value["command"].as_str(), Some("npx"));
        });
    }

    #[test]
    fn rename_is_a_noop_when_names_match() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            save_mcp_server_config("same", r#"{"type":"stdio","command":"npx"}"#).expect("save");
            rename_mcp_server_config("same", "same").expect("noop");
            assert!(read_mcp_server_config("same").is_ok());
        });
    }

    #[test]
    fn rename_refuses_when_target_exists() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            save_mcp_server_config("a", r#"{"type":"stdio","command":"a"}"#).expect("save a");
            save_mcp_server_config("b", r#"{"type":"stdio","command":"b"}"#).expect("save b");

            let err = rename_mcp_server_config("a", "b").expect_err("should refuse");
            assert!(err.contains("already exists"), "unexpected error: {err}");

            // Both originals still present, untouched.
            assert!(read_mcp_server_config("a").is_ok());
            assert!(read_mcp_server_config("b").is_ok());
        });
    }

    #[test]
    fn rename_returns_error_for_missing_source() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            let err = rename_mcp_server_config("ghost", "phantom").expect_err("should error");
            assert!(err.contains("not found"), "unexpected error: {err}");
        });
    }

    #[test]
    fn rename_migrates_provenance_entry() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            save_mcp_server_config("old-remote", r#"{"type":"stdio","command":"npx"}"#)
                .expect("save");
            super::super::remote_sources::record_provenance(
                "mcp_server",
                "old-remote",
                "octocat/mcp-collection",
            )
            .expect("record provenance");

            rename_mcp_server_config("old-remote", "new-remote").expect("rename");

            assert_eq!(
                super::super::remote_sources::get_provenance("mcp_server", "old-remote")
                    .expect("read old provenance"),
                None
            );
            assert_eq!(
                super::super::remote_sources::get_provenance("mcp_server", "new-remote")
                    .expect("read new provenance"),
                Some("octocat/mcp-collection".to_string())
            );
        });
    }

    #[test]
    fn rename_is_refused_when_provenance_cannot_be_read() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            save_mcp_server_config("owned-old", r#"{"type":"stdio","command":"npx"}"#)
                .expect("save");
            let provenance = crate::core::paths::get_automatic_dir()
                .expect("automatic dir")
                .join("source-provenance.json");
            fs::write(&provenance, r#"{"mcp_server:owned-old": "octo"#)
                .expect("write corrupt provenance");

            let err = rename_mcp_server_config("owned-old", "owned-new").expect_err("refused");

            assert!(err.contains("was not renamed"), "unexpected error: {err}");
            let dir = get_mcp_servers_dir().expect("servers dir");
            assert!(dir.join("owned-old.json").exists());
            assert!(!dir.join("owned-new.json").exists());
            assert_eq!(
                fs::read_to_string(&provenance).expect("read provenance"),
                r#"{"mcp_server:owned-old": "octo"#
            );
        });
    }

    #[test]
    fn rename_warns_when_a_provenance_move_fails() {
        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            save_mcp_server_config("moving-old", r#"{"type":"stdio","command":"npx"}"#)
                .expect("save");
            super::super::remote_sources::record_provenance(
                "mcp_server",
                "moving-old",
                "octocat/mcp-collection",
            )
            .expect("record provenance");

            super::super::remote_sources::fail_provenance_writes(true);
            let result = rename_mcp_server_config("moving-old", "moving-new");
            super::super::remote_sources::fail_provenance_writes(false);

            let warnings = result.expect("the rename itself succeeds");
            assert_eq!(warnings.len(), 1, "{warnings:?}");
            assert!(
                warnings[0].contains("'moving-old' was renamed to 'moving-new'")
                    && warnings[0].contains("source ownership could not be moved"),
                "unexpected warnings: {warnings:?}"
            );
            assert!(
                super::super::remote_sources::get_provenance("mcp_server", "moving-new")
                    .expect("read provenance")
                    .is_none(),
                "a failed move must not record the new name"
            );
            assert!(get_mcp_servers_dir()
                .expect("servers dir")
                .join("moving-new.json")
                .exists());
        });
    }

    // ── OAuth secrets follow the server ──────────────────────────────────────
    //
    // The test secret store and the token presence cache are shared by the
    // whole test process, so each test uses server names no other test uses.

    use crate::core::secret_store::{self, set_test_fault, TestFault};

    const URL: &str = "https://mcp.example.com/mcp";
    const OTHER_URL: &str = "https://other.example/mcp";
    const OAUTH_SERVER: &str = r#"{"type":"http","url":"https://mcp.example.com/mcp"}"#;
    const OTHER_URL_SERVER: &str = r#"{"type":"http","url":"https://other.example/mcp"}"#;
    const STDIO_SERVER: &str = r#"{"type":"stdio","command":"npx"}"#;

    fn sign_in(server: &str, url: &str) {
        crate::proxy::store_oauth_token(server, url, "access-token").expect("store token");
        crate::proxy::store_oauth_credentials(server, url, r#"{"refresh_token":"refresh"}"#)
            .expect("store credentials");
    }

    fn has_no_oauth_secrets(server: &str, url: &str) -> bool {
        crate::proxy::load_oauth_token(server, url).is_err()
            && crate::proxy::load_oauth_credentials(server, url).is_err()
            && crate::proxy::has_oauth_token(server, url) == Ok(false)
    }

    /// The entry an older version stored a server's token under.
    fn legacy_token_entry(server: &str) -> String {
        format!("mcp_oauth_token_{}", server)
    }

    /// Leave a server in the registry as an older version would have. The
    /// file is written directly. A save through this version would first
    /// record that the server did not exist before.
    fn leave_server_from_an_older_version(server: &str, config: &str) {
        let dir = get_mcp_servers_dir().expect("dir");
        fs::create_dir_all(&dir).expect("create the registry");
        fs::write(dir.join(format!("{}.json", server)), config).expect("write");
    }

    #[test]
    fn the_url_record_describes_the_registry_before_this_version_changed_it() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let (server, later) = ("url-record-fixture", "url-record-later-fixture");
            leave_server_from_an_older_version(server, OAUTH_SERVER);

            save_mcp_server_config(server, OTHER_URL_SERVER).expect("change the url");
            save_mcp_server_config(later, OAUTH_SERVER).expect("create another");

            assert!(legacy_oauth_url_is(server, URL).expect("recorded url"));
            assert!(legacy_oauth_url_is(server, " HTTPS://MCP.Example.com:443/mcp ").expect("tidied"));
            assert!(!legacy_oauth_url_is(server, OTHER_URL).expect("new url"));
            assert!(!legacy_oauth_url_is(later, URL).expect("created later"));
        });
    }

    #[test]
    fn the_url_record_is_written_before_a_delete_or_a_rename() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let (deleted, renamed) = ("url-record-deleted-fixture", "url-record-renamed-fixture");
            leave_server_from_an_older_version(deleted, OAUTH_SERVER);
            leave_server_from_an_older_version(renamed, OAUTH_SERVER);

            delete_mcp_server_config(deleted).expect("delete");
            rename_mcp_server_config(renamed, "url-record-renamed-to-fixture").expect("rename");

            assert!(legacy_oauth_url_is(deleted, URL).expect("deleted"));
            assert!(legacy_oauth_url_is(renamed, URL).expect("renamed"));
            assert!(!legacy_oauth_url_is("url-record-renamed-to-fixture", URL).expect("new name"));
        });
    }

    #[test]
    fn the_url_record_leaves_out_an_entry_it_cannot_read() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "url-record-beside-unreadable-fixture";
            leave_server_from_an_older_version(server, OAUTH_SERVER);
            let dir = get_mcp_servers_dir().expect("dir");
            fs::create_dir_all(dir.join("url-record-directory-fixture.json")).expect("a directory");
            fs::write(dir.join("url-record-binary-fixture.json"), [0xff, 0xfe, 0x00]).expect("not text");

            save_mcp_server_config(server, OTHER_URL_SERVER).expect("the save is not blocked");

            assert!(legacy_oauth_url_is(server, URL).expect("recorded"));
            assert!(!legacy_oauth_url_is("url-record-binary-fixture", URL).expect("left out"));
        });
    }

    #[test]
    fn a_new_file_is_written_once_and_never_replaced() {
        let dir = tmp();
        let path = dir.path().join("record.json");

        write_new_file(&path, "first").expect("write");
        write_new_file(&path, "second").expect("an existing file is success");

        assert_eq!(fs::read_to_string(&path).expect("read"), "first");
    }

    #[test]
    fn a_url_record_that_does_not_parse_is_an_error() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let path = legacy_oauth_urls_path().expect("path");
            fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
            fs::write(&path, "not json").expect("write");

            legacy_oauth_url_is("url-record-corrupt-fixture", URL).expect_err("not valid JSON");
            // A save still works. The record exists, so it is left alone.
            save_mcp_server_config("url-record-corrupt-fixture", OAUTH_SERVER).expect("save");
        });
    }

    #[test]
    fn a_token_does_not_follow_a_server_to_a_new_url() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "secrets-url-changed-fixture";
            save_mcp_server_config(server, OAUTH_SERVER).expect("save");
            sign_in(server, URL);
            assert_eq!(crate::proxy::has_oauth_token(server, URL), Ok(true));

            save_mcp_server_config(server, OTHER_URL_SERVER).expect("save new url");

            assert!(has_no_oauth_secrets(server, OTHER_URL));
            // The entries for the old URL were tidied away.
            assert!(has_no_oauth_secrets(server, URL));
        });
    }

    #[test]
    fn a_token_does_not_follow_a_server_to_a_new_url_when_it_cannot_be_deleted() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "fault-save-refused-fixture";
            save_mcp_server_config(server, OAUTH_SERVER).expect("save");
            sign_in(server, URL);
            set_test_fault(
                &crate::proxy::oauth_token_entry(server, URL),
                Some(TestFault::DeleteRefused),
            );

            save_mcp_server_config(server, OTHER_URL_SERVER).expect("the save still goes ahead");

            assert!(mcp_server_url_is(server, OTHER_URL).expect("url"));
            assert_eq!(crate::proxy::has_oauth_token(server, OTHER_URL), Ok(false));
            assert!(crate::proxy::load_oauth_token(server, OTHER_URL).is_err());
            // Still stored, and still only for the URL it was issued for.
            assert_eq!(crate::proxy::load_oauth_token(server, URL).expect("token"), "access-token");
        });
    }

    #[test]
    fn a_save_goes_ahead_when_the_secret_store_cannot_be_read() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "fault-save-unreadable-fixture";
            for url in [URL, OTHER_URL] {
                set_test_fault(&crate::proxy::oauth_token_entry(server, url), Some(TestFault::Unreadable));
            }
            set_test_fault(&legacy_token_entry(server), Some(TestFault::Unreadable));

            save_mcp_server_config(server, OAUTH_SERVER).expect("create");
            save_mcp_server_config(server, OTHER_URL_SERVER).expect("change the url");

            assert!(mcp_server_url_is(server, OTHER_URL).expect("url"));
        });
    }

    #[test]
    fn a_save_that_keeps_the_url_keeps_the_oauth_secrets() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "secrets-url-kept-fixture";
            save_mcp_server_config(server, OAUTH_SERVER).expect("save");
            sign_in(server, URL);

            // The same URL written another way, and another field changed.
            let rewritten = " HTTPS://MCP.Example.com:443/mcp ";
            save_mcp_server_config(
                server,
                &serde_json::json!({ "type": "http", "url": rewritten, "headers": { "X-Team": "a" } })
                    .to_string(),
            )
            .expect("save again");

            assert_eq!(
                crate::proxy::load_oauth_token(server, rewritten).expect("token"),
                "access-token"
            );
            assert!(crate::proxy::load_oauth_credentials(server, rewritten).is_ok());
            assert_eq!(crate::proxy::has_oauth_token(server, URL), Ok(true));
        });
    }

    #[test]
    fn a_save_that_drops_the_url_removes_the_oauth_secrets() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "secrets-url-dropped-fixture";
            save_mcp_server_config(server, OAUTH_SERVER).expect("save");
            sign_in(server, URL);

            save_mcp_server_config(server, STDIO_SERVER).expect("save as stdio");

            assert!(has_no_oauth_secrets(server, URL));
        });
    }

    #[test]
    fn a_new_server_does_not_start_signed_in_as_an_earlier_one() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "secrets-url-inherit-fixture";
            // Left by an earlier server of this name at this URL, and by one
            // from an older version.
            sign_in(server, URL);
            secret_store::store_secret(&legacy_token_entry(server), "older-token").expect("seed");
            assert_eq!(crate::proxy::has_oauth_token(server, URL), Ok(true));

            save_mcp_server_config(server, OAUTH_SERVER).expect("save");

            assert!(has_no_oauth_secrets(server, URL));
            assert_eq!(secret_store::load_secret_if_present(&legacy_token_entry(server)), Ok(None));
        });
    }

    #[test]
    fn a_token_an_older_version_stored_does_not_follow_a_server_to_a_new_url() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "secrets-url-legacy-fixture";
            leave_server_from_an_older_version(server, OAUTH_SERVER);
            secret_store::store_secret(&legacy_token_entry(server), "older-token").expect("seed");

            save_mcp_server_config(server, OTHER_URL_SERVER).expect("save new url");

            assert!(has_no_oauth_secrets(server, OTHER_URL));
        });
    }

    #[test]
    fn a_save_over_a_config_that_does_not_parse_does_not_keep_a_sign_in() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "secrets-url-unparsed-fixture";
            save_mcp_server_config(server, OAUTH_SERVER).expect("save");
            sign_in(server, URL);
            let path = get_mcp_servers_dir().expect("dir").join(format!("{}.json", server));
            fs::write(&path, "not json").expect("corrupt the config");

            save_mcp_server_config(server, OAUTH_SERVER).expect("save again");

            assert!(has_no_oauth_secrets(server, URL));
        });
    }

    #[test]
    fn a_save_under_another_case_of_the_name_does_not_carry_the_token_to_the_new_url() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            // Where the filesystem ignores case, the second save writes the
            // first server's file. Where it does not, it makes a second
            // server. Either way the first one has no token for the new URL.
            let server = "secrets-case-save-fixture";
            save_mcp_server_config(server, OAUTH_SERVER).expect("save");
            sign_in(server, URL);

            save_mcp_server_config(&server.to_uppercase(), OTHER_URL_SERVER)
                .expect("save under the other case");

            assert_eq!(crate::proxy::has_oauth_token(server, OTHER_URL), Ok(false));
            assert_eq!(crate::proxy::has_oauth_token(&server.to_uppercase(), OTHER_URL), Ok(false));
        });
    }

    #[test]
    fn delete_removes_the_oauth_secrets() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "secrets-delete-fixture";
            save_mcp_server_config(server, OAUTH_SERVER).expect("save");
            sign_in(server, URL);
            assert_eq!(crate::proxy::has_oauth_token(server, URL), Ok(true));

            delete_mcp_server_config(server).expect("delete");

            assert!(read_mcp_server_config(server).is_err());
            assert!(has_no_oauth_secrets(server, URL));
        });
    }

    #[test]
    fn delete_succeeds_for_a_server_with_no_oauth_secrets() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "secrets-delete-none-fixture";
            save_mcp_server_config(server, OAUTH_SERVER).expect("save");

            delete_mcp_server_config(server).expect("delete");

            assert!(read_mcp_server_config(server).is_err());
        });
    }

    #[test]
    fn delete_removes_a_token_an_older_version_left_without_a_config() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "secrets-delete-orphan-fixture";
            secret_store::store_secret(&legacy_token_entry(server), "older-token").expect("seed");

            delete_mcp_server_config(server).expect("delete");

            assert_eq!(secret_store::load_secret_if_present(&legacy_token_entry(server)), Ok(None));
        });
    }

    #[test]
    fn delete_goes_ahead_when_the_store_still_holds_the_token() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "fault-delete-refused-fixture";
            save_mcp_server_config(server, OAUTH_SERVER).expect("save");
            sign_in(server, URL);
            set_test_fault(
                &crate::proxy::oauth_token_entry(server, URL),
                Some(TestFault::DeleteRefused),
            );

            delete_mcp_server_config(server).expect("delete");

            assert!(read_mcp_server_config(server).is_err());
            // What is left can only be found for this name and this URL.
            assert!(crate::proxy::load_oauth_token(server, OTHER_URL).is_err());
        });
    }

    #[test]
    fn delete_goes_ahead_when_the_secret_store_cannot_be_read() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "fault-delete-unreadable-fixture";
            save_mcp_server_config(server, OAUTH_SERVER).expect("save");
            set_test_fault(&crate::proxy::oauth_token_entry(server, URL), Some(TestFault::Unreadable));
            set_test_fault(&legacy_token_entry(server), Some(TestFault::Unreadable));

            delete_mcp_server_config(server).expect("delete");

            assert!(read_mcp_server_config(server).is_err());
        });
    }

    #[test]
    fn rename_moves_the_oauth_secrets_to_the_new_name() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let (old, new) = ("secrets-rename-old-fixture", "secrets-rename-new-fixture");
            save_mcp_server_config(old, OAUTH_SERVER).expect("save");
            sign_in(old, URL);
            assert_eq!(crate::proxy::has_oauth_token(old, URL), Ok(true));
            assert_eq!(crate::proxy::has_oauth_token(new, URL), Ok(false));

            rename_mcp_server_config(old, new).expect("rename");

            assert_eq!(crate::proxy::load_oauth_token(new, URL).expect("token"), "access-token");
            assert_eq!(
                crate::proxy::load_oauth_credentials(new, URL).expect("credentials"),
                r#"{"refresh_token":"refresh"}"#
            );
            assert_eq!(crate::proxy::has_oauth_token(new, URL), Ok(true));
            assert!(has_no_oauth_secrets(old, URL));
        });
    }

    #[test]
    fn rename_moves_a_token_an_older_version_stored() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let (old, new) = ("secrets-rename-legacy-old-fixture", "secrets-rename-legacy-new-fixture");
            leave_server_from_an_older_version(old, OAUTH_SERVER);
            secret_store::store_secret(&legacy_token_entry(old), "older-token").expect("seed");

            rename_mcp_server_config(old, new).expect("rename");

            assert_eq!(crate::proxy::load_oauth_token(new, URL).expect("token"), "older-token");
            assert_eq!(secret_store::load_secret_if_present(&legacy_token_entry(old)), Ok(None));
        });
    }

    #[test]
    fn rename_succeeds_for_a_server_with_no_oauth_secrets() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let (old, new) = ("secrets-rename-none-old-fixture", "secrets-rename-none-new-fixture");
            save_mcp_server_config(old, OAUTH_SERVER).expect("save");

            rename_mcp_server_config(old, new).expect("rename");

            assert!(read_mcp_server_config(new).is_ok());
            assert!(has_no_oauth_secrets(old, URL));
            assert!(has_no_oauth_secrets(new, URL));
        });
    }

    #[test]
    fn a_refused_rename_leaves_the_oauth_secrets_where_they_were() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let (old, taken) = ("secrets-rename-kept-fixture", "secrets-rename-taken-fixture");
            save_mcp_server_config(old, OAUTH_SERVER).expect("save old");
            save_mcp_server_config(taken, OAUTH_SERVER).expect("save taken");
            sign_in(old, URL);
            crate::proxy::store_oauth_token(taken, URL, "other-token").expect("store other token");

            rename_mcp_server_config(old, taken).expect_err("should refuse");

            assert_eq!(crate::proxy::load_oauth_token(old, URL).expect("token"), "access-token");
            assert!(crate::proxy::load_oauth_credentials(old, URL).is_ok());
            assert_eq!(
                crate::proxy::load_oauth_token(taken, URL).expect("other token"),
                "other-token"
            );
        });
    }

    #[test]
    fn rename_goes_ahead_when_the_secret_store_cannot_be_read() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let (old, new) = ("fault-rename-unreadable-old-fixture", "fault-rename-unreadable-new-fixture");
            save_mcp_server_config(old, OAUTH_SERVER).expect("save");
            set_test_fault(&crate::proxy::oauth_token_entry(old, URL), Some(TestFault::Unreadable));

            rename_mcp_server_config(old, new).expect("rename");

            assert!(read_mcp_server_config(new).is_ok());
            assert!(has_no_oauth_secrets(new, URL));
        });
    }

    #[test]
    fn rename_goes_ahead_when_the_old_token_cannot_be_deleted() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let (old, new) = ("fault-rename-refused-old-fixture", "fault-rename-refused-new-fixture");
            save_mcp_server_config(old, OAUTH_SERVER).expect("save");
            sign_in(old, URL);
            set_test_fault(
                &crate::proxy::oauth_token_entry(old, URL),
                Some(TestFault::DeleteRefused),
            );

            rename_mcp_server_config(old, new).expect("rename");

            assert!(read_mcp_server_config(new).is_ok());
            assert_eq!(crate::proxy::load_oauth_token(new, URL).expect("token"), "access-token");
        });
    }

    #[test]
    fn a_server_with_no_url_is_deleted_without_a_call_to_the_secret_store() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "no-url-delete-fixture";
            save_mcp_server_config(server, STDIO_SERVER).expect("save");
            // A call to the store would delete this.
            secret_store::store_secret(&legacy_token_entry(server), "older-token").expect("seed");

            delete_mcp_server_config(server).expect("delete");

            assert!(read_mcp_server_config(server).is_err());
            assert_eq!(
                secret_store::load_secret_if_present(&legacy_token_entry(server)),
                Ok(Some("older-token".to_string()))
            );
        });
    }

    #[test]
    fn a_server_with_no_url_is_renamed_without_a_call_to_the_secret_store() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let (old, new) = ("no-url-rename-old-fixture", "no-url-rename-new-fixture");
            save_mcp_server_config(old, STDIO_SERVER).expect("save");
            secret_store::store_secret(&legacy_token_entry(old), "older-token").expect("seed");

            rename_mcp_server_config(old, new).expect("rename");

            assert!(read_mcp_server_config(new).is_ok());
            assert_eq!(
                secret_store::load_secret_if_present(&legacy_token_entry(old)),
                Ok(Some("older-token".to_string()))
            );
        });
    }

    #[test]
    fn a_token_left_under_a_server_with_no_url_is_not_used_when_it_is_given_one() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "no-url-then-url-fixture";
            save_mcp_server_config(server, STDIO_SERVER).expect("save");
            secret_store::store_secret(&legacy_token_entry(server), "older-token").expect("seed");

            save_mcp_server_config(server, OAUTH_SERVER).expect("save with a url");

            assert!(has_no_oauth_secrets(server, URL));
        });
    }

    // ── The saved URL ────────────────────────────────────────────────────────

    #[test]
    fn the_saved_url_is_matched_in_comparable_form() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "saved-url-fixture";
            save_mcp_server_config(server, OAUTH_SERVER).expect("save");

            assert!(mcp_server_url_is(server, URL).expect("same"));
            assert!(mcp_server_url_is(server, " HTTPS://MCP.example.com:443/mcp ").expect("tidied"));
            assert!(!mcp_server_url_is(server, OTHER_URL).expect("other host"));
            assert!(!mcp_server_url_is(server, "https://mcp.example.com/sse").expect("other path"));
            assert!(!mcp_server_url_is(server, "").expect("empty"));
            assert_eq!(saved_mcp_server_url(server).expect("saved"), comparable_mcp_url(URL));
        });
    }

    #[test]
    fn a_server_without_a_url_has_no_saved_url() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            save_mcp_server_config("saved-url-stdio-fixture", STDIO_SERVER).expect("save");

            assert!(!mcp_server_url_is("saved-url-stdio-fixture", URL).expect("stdio"));
            assert!(!mcp_server_url_is("saved-url-stdio-fixture", "").expect("empty"));
            assert_eq!(saved_mcp_server_url("saved-url-stdio-fixture").expect("stdio"), None);
            assert!(!mcp_server_url_is("saved-url-missing-fixture", URL).expect("missing"));
            assert_eq!(saved_mcp_server_url("saved-url-missing-fixture").expect("missing"), None);
        });
    }

    #[test]
    fn a_config_that_does_not_parse_has_no_readable_saved_url() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let server = "saved-url-unparsed-fixture";
            save_mcp_server_config(server, OAUTH_SERVER).expect("save");
            let path = get_mcp_servers_dir().expect("dir").join(format!("{}.json", server));
            fs::write(&path, "not json").expect("corrupt the config");

            saved_mcp_server_url(server).expect_err("not valid JSON");
        });
    }

    #[test]
    fn the_comparable_form_of_a_url_is_stable() {
        // OAuth secrets are filed under a hash of these exact strings.
        assert_eq!(
            comparable_mcp_url("https://mcp.example.com/mcp").as_deref(),
            Some("https://mcp.example.com/mcp")
        );
        assert_eq!(
            comparable_mcp_url(" HTTPS://MCP.Example.com:443 ").as_deref(),
            Some("https://mcp.example.com/")
        );
        assert_eq!(
            comparable_mcp_url("http://localhost:8080/sse?x=1").as_deref(),
            Some("http://localhost:8080/sse?x=1")
        );
        assert_eq!(comparable_mcp_url("not a url").as_deref(), Some("not a url"));
        assert_eq!(comparable_mcp_url("  "), None);
    }

    #[test]
    fn a_discovered_config_is_added_only_under_a_new_name() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            save_mcp_server_config("github", r#"{"type":"http","url":"https://trusted.example/mcp"}"#)
                .expect("seed");

            let incoming = r#"{"type":"http","url":"https://other.example/mcp"}"#;
            add_discovered_mcp_server_config("github", incoming).expect("existing name");
            add_discovered_mcp_server_config("GitHub", incoming).expect("case variant");
            add_discovered_mcp_server_config("fresh", incoming).expect("new name");
            assert!(add_discovered_mcp_server_config("../escape", incoming).is_err());

            assert_eq!(
                list_mcp_server_configs().expect("list"),
                vec!["fresh".to_string(), "github".to_string()]
            );
            let kept = read_mcp_server_config("github").expect("read");
            assert!(kept.contains("trusted.example"), "{kept}");
            let added = read_mcp_server_config("fresh").expect("read");
            assert!(added.contains("other.example"), "{added}");
        });
    }

    // ── discovered configs: ciphertext-like env values ───────────────────────

    #[test]
    fn blanking_empties_only_prefixed_env_strings_and_keeps_their_keys() {
        let incoming = serde_json::json!({
            "command": "npx",
            "args": ["enc:v1:not-an-env-value"],
            "env": {
                "PLANTED": "enc:v1:AAAA",
                "PLAIN": "value",
                "EMPTY": "",
                "MENTIONS_PREFIX": "see enc:v1: later",
                "NUMBER": 7,
                "NESTED": { "INNER": "enc:v1:AAAA" },
            },
            "headers": { "Authorization": "enc:v1:AAAA" },
        });

        let blanked: serde_json::Value =
            serde_json::from_str(&blank_ciphertext_like_env_values(&incoming.to_string()))
                .expect("parse");

        let mut expected = incoming.clone();
        expected["env"]["PLANTED"] = serde_json::json!("");
        assert_eq!(blanked, expected);
    }

    #[test]
    fn blanking_returns_other_input_unchanged() {
        for input in [
            "{ not json",
            r#"["enc:v1:AAAA"]"#,
            r#"{"command":"npx"}"#,
            r#"{"command":"npx","env":"enc:v1:AAAA"}"#,
            r#"{ "command": "npx",   "env": { "PLAIN": "value" } }"#,
        ] {
            assert_eq!(blank_ciphertext_like_env_values(input), input);
        }
    }

    #[test]
    fn a_discovered_config_with_a_ciphertext_like_env_value_is_stored_readable() {
        let home = tmp();
        with_test_home(home.path().to_path_buf(), || {
            let incoming = r#"{"command":"npx","env":{"API_KEY":"enc:v1:AAAA","REGION":"eu"}}"#;
            add_discovered_mcp_server_config("discovered-planted", incoming).expect("add");

            let raw = read_mcp_server_config("discovered-planted")
                .expect("the stored entry must be readable");
            let stored: serde_json::Value = serde_json::from_str(&raw).expect("parse");
            assert_eq!(stored["env"]["API_KEY"], "");
            assert_eq!(stored["env"]["REGION"], "eu");
            assert_eq!(stored["command"], "npx");
        });
    }
}
