//! One interface for every secret Automatic stores: API keys, OAuth tokens,
//! account tokens and the env-encryption key.
//!
//! The backend depends on the build:
//!
//! - **Release builds** use the OS keychain.
//! - **Debug builds** use a JSON file in the dev data directory. macOS ties a
//!   keychain item's "Always Allow" to the binary that asked for it. A dev
//!   binary changes on every rebuild, so each stored secret raised a password
//!   dialog after every build, and unattended runs stalled on those dialogs.
//!   Set `AUTOMATIC_KEYCHAIN=system` to use the OS keychain from a debug build.
//! - **Test builds** use a map in memory, so a test never reads or changes a
//!   real secret.
//!
//! The file backend is compiled out of release builds. No environment variable
//! can make a release build write a secret to disk.

/// Store `secret` under `account`, replacing any value already there.
pub fn store_secret(account: &str, secret: &str) -> Result<(), String> {
    backend::store(account, secret)
}

/// Load the secret stored under `account`.
///
/// Returns an error when nothing is stored, as well as when the backend
/// fails. Callers that only need presence use `.is_ok()`. Callers that must
/// tell the two apart use [`load_secret_if_present`].
pub fn load_secret(account: &str) -> Result<String, String> {
    backend::load(account)
}

/// Load the secret stored under `account`, or `None` when nothing is stored.
///
/// A failed read is an error here, never `None`. Use this before creating a
/// secret that must not replace an existing one: a backend that could not be
/// read may still hold the value.
pub fn load_secret_if_present(account: &str) -> Result<Option<String>, String> {
    backend::load_if_present(account)
}

/// Delete the secret stored under `account`.
///
/// Returns an error when nothing is stored, matching the OS keychain.
pub fn delete_secret(account: &str) -> Result<(), String> {
    backend::delete(account)
}

/// Check the backend configuration. Call once at startup.
///
/// The presence checks treat every error as "nothing stored". Without this
/// check, a mistyped `AUTOMATIC_KEYCHAIN` would look like an empty store, and
/// sync would write agent config as if no OAuth token existed. Release and
/// test builds have nothing to configure.
pub fn check_secret_store_config() -> Result<(), String> {
    #[cfg(all(not(test), debug_assertions))]
    {
        dev_backend()?;
    }
    Ok(())
}

#[cfg(any(test, debug_assertions))]
fn missing_secret(account: &str) -> String {
    format!("no secret is stored for '{}'", account)
}

/// Split a keychain read into "nothing stored" and "the read failed".
///
/// Only `NoEntry` means nothing is stored. A denied prompt, a locked keychain
/// or an unreachable backend leaves the stored value in place, so those stay
/// errors. On Linux the `keyring` crate itself reports a denied, revoked or
/// expired key as `NoEntry`, so those cases cannot be told apart here.
fn absent_entry_as_none(read: Result<String, keyring::Error>) -> Result<Option<String>, String> {
    match read {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

// ── Backend selection ────────────────────────────────────────────────────────

#[cfg(test)]
use memory as backend;

#[cfg(all(not(test), not(debug_assertions)))]
use system as backend;

#[cfg(all(not(test), debug_assertions))]
mod backend {
    use super::{dev_backend, dev_secrets_path, file, system, DevBackend};

    pub(super) fn store(account: &str, secret: &str) -> Result<(), String> {
        match dev_backend()? {
            DevBackend::File => file::store(&dev_secrets_path()?, account, secret),
            DevBackend::System => system::store(account, secret),
        }
    }

    pub(super) fn load(account: &str) -> Result<String, String> {
        match dev_backend()? {
            DevBackend::File => file::load(&dev_secrets_path()?, account),
            DevBackend::System => system::load(account),
        }
    }

    pub(super) fn load_if_present(account: &str) -> Result<Option<String>, String> {
        match dev_backend()? {
            DevBackend::File => file::load_if_present(&dev_secrets_path()?, account),
            DevBackend::System => system::load_if_present(account),
        }
    }

    pub(super) fn delete(account: &str) -> Result<(), String> {
        match dev_backend()? {
            DevBackend::File => file::delete(&dev_secrets_path()?, account),
            DevBackend::System => system::delete(account),
        }
    }
}

/// Environment variable that picks the backend in a debug build.
#[cfg(debug_assertions)]
const BACKEND_ENV: &str = "AUTOMATIC_KEYCHAIN";

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DevBackend {
    File,
    System,
}

/// Map the value of `AUTOMATIC_KEYCHAIN` to a backend. An unknown value is an
/// error, so a typo cannot silently pick the wrong store.
#[cfg(debug_assertions)]
fn parse_dev_backend(value: Option<&str>) -> Result<DevBackend, String> {
    match value {
        None | Some("file") => Ok(DevBackend::File),
        Some("system") => Ok(DevBackend::System),
        Some(other) => Err(format!(
            "{} must be 'file' or 'system', got '{}'",
            BACKEND_ENV, other
        )),
    }
}

#[cfg(all(not(test), debug_assertions))]
fn dev_backend() -> Result<DevBackend, String> {
    match std::env::var(BACKEND_ENV) {
        Ok(value) => parse_dev_backend(Some(&value)),
        Err(std::env::VarError::NotPresent) => parse_dev_backend(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(format!("{} is not valid UTF-8", BACKEND_ENV))
        }
    }
}

#[cfg(all(not(test), debug_assertions))]
fn dev_secrets_path() -> Result<std::path::PathBuf, String> {
    Ok(super::paths::get_automatic_dir()?.join("dev-secrets.json"))
}

// ── OS keychain ──────────────────────────────────────────────────────────────

#[cfg(not(test))]
mod system {
    use keyring::Entry;

    // Debug builds use a separate service, so dev and release entries never
    // collide.
    #[cfg(debug_assertions)]
    const KEYCHAIN_SERVICE: &str = "automatic_desktop_dev";
    #[cfg(not(debug_assertions))]
    const KEYCHAIN_SERVICE: &str = "automatic_desktop";

    fn entry(account: &str) -> Result<Entry, String> {
        Entry::new(KEYCHAIN_SERVICE, account).map_err(|e| e.to_string())
    }

    pub(super) fn store(account: &str, secret: &str) -> Result<(), String> {
        entry(account)?
            .set_password(secret)
            .map_err(|e| e.to_string())
    }

    pub(super) fn load(account: &str) -> Result<String, String> {
        entry(account)?.get_password().map_err(|e| e.to_string())
    }

    pub(super) fn load_if_present(account: &str) -> Result<Option<String>, String> {
        super::absent_entry_as_none(entry(account)?.get_password())
    }

    pub(super) fn delete(account: &str) -> Result<(), String> {
        entry(account)?
            .delete_credential()
            .map_err(|e| e.to_string())
    }
}

// ── Dev secrets file ─────────────────────────────────────────────────────────

#[cfg(debug_assertions)]
mod file {
    use std::collections::BTreeMap;
    use std::fs::{self, File, OpenOptions};
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use super::missing_secret;

    type Secrets = BTreeMap<String, String>;

    pub(super) fn store(path: &Path, account: &str, secret: &str) -> Result<(), String> {
        let _lock = lock_for_update(path)?;
        let mut secrets = read_secrets(path)?;
        secrets.insert(account.to_string(), secret.to_string());
        write_secrets(path, &secrets)
    }

    /// Reads take no lock. A write replaces the file with one rename, so a
    /// reader sees either the old file or the new one.
    pub(super) fn load(path: &Path, account: &str) -> Result<String, String> {
        read_secrets(path)?
            .remove(account)
            .ok_or_else(|| missing_secret(account))
    }

    /// `None` when the account, or the whole file, does not exist. A file
    /// that cannot be read or parsed is an error.
    pub(super) fn load_if_present(path: &Path, account: &str) -> Result<Option<String>, String> {
        Ok(read_secrets(path)?.remove(account))
    }

    pub(super) fn delete(path: &Path, account: &str) -> Result<(), String> {
        let _lock = lock_for_update(path)?;
        let mut secrets = read_secrets(path)?;
        if secrets.remove(account).is_none() {
            return Err(missing_secret(account));
        }
        write_secrets(path, &secrets)
    }

    /// Take an exclusive lock for one read-modify-write. Every write rewrites
    /// the whole file, and a debug build runs as several processes at once
    /// (the app, `mcp-serve`, one `mcp-proxy` per remote server). Without the
    /// lock, two of them writing at the same moment would drop one secret.
    ///
    /// The lock is on a sidecar file because the secrets file itself is
    /// replaced on every write. It is released when the returned file closes.
    fn lock_for_update(path: &Path) -> Result<File, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
        }
        let lock_path = with_suffix(path, ".lock");
        let lock = owner_only(OpenOptions::new().write(true).create(true).truncate(false))
            .open(&lock_path)
            .and_then(|file| file.lock().map(|()| file));
        lock.map_err(|e| format!("failed to lock {}: {}", lock_path.display(), e))
    }

    /// A file that does not exist yet holds no secrets. A file that cannot be
    /// parsed is an error, so a later write never replaces it with less. The
    /// error gives the position only: the parser's own message can quote a
    /// value from the file.
    fn read_secrets(path: &Path) -> Result<Secrets, String> {
        match fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str(&raw).map_err(|e| {
                format!(
                    "dev secrets file {} is not a JSON object of strings (line {}, column {})",
                    path.display(),
                    e.line(),
                    e.column()
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Secrets::new()),
            Err(e) => Err(format!(
                "failed to read dev secrets file {}: {}",
                path.display(),
                e
            )),
        }
    }

    /// Write through a temp file and rename it into place, so a crash leaves
    /// either the old file or the new one. The temp file is created owner-only
    /// before any secret is written to it.
    fn write_secrets(path: &Path, secrets: &Secrets) -> Result<(), String> {
        let body = serde_json::to_vec_pretty(secrets)
            .map_err(|e| format!("failed to serialise dev secrets: {}", e))?;

        // The process id keeps a stale temp file from another process's
        // crashed write out of the way.
        let tmp_path = with_suffix(path, &format!(".{}.tmp", std::process::id()));
        // A leftover from a crashed write would make `create_new` fail.
        let _ = fs::remove_file(&tmp_path);

        let written = owner_only(OpenOptions::new().write(true).create_new(true))
            .open(&tmp_path)
            .and_then(|mut tmp| {
                tmp.write_all(&body)?;
                tmp.sync_all()
            })
            .and_then(|()| fs::rename(&tmp_path, path));

        written.map_err(|e| {
            let _ = fs::remove_file(&tmp_path);
            format!(
                "failed to write dev secrets file {}: {}",
                path.display(),
                e
            )
        })
    }

    fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(suffix);
        path.with_file_name(name)
    }

    #[cfg(unix)]
    fn owner_only(options: &mut OpenOptions) -> &mut OpenOptions {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600)
    }

    // Windows has no mode bits. The file inherits the access list of the
    // user's profile directory, which already limits it to that user.
    #[cfg(not(unix))]
    fn owner_only(options: &mut OpenOptions) -> &mut OpenOptions {
        options
    }
}

// ── In-memory store for tests ────────────────────────────────────────────────

#[cfg(test)]
mod memory {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    use super::missing_secret;

    /// Shared by every test in the process, like the keychain it stands in
    /// for. A test that stores a secret should use an account name no other
    /// test reads.
    fn secrets() -> &'static Mutex<HashMap<String, String>> {
        static SECRETS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
        SECRETS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub(super) fn store(account: &str, secret: &str) -> Result<(), String> {
        let mut secrets = secrets().lock().map_err(|e| e.to_string())?;
        secrets.insert(account.to_string(), secret.to_string());
        Ok(())
    }

    pub(super) fn load(account: &str) -> Result<String, String> {
        let secrets = secrets().lock().map_err(|e| e.to_string())?;
        secrets
            .get(account)
            .cloned()
            .ok_or_else(|| missing_secret(account))
    }

    pub(super) fn load_if_present(account: &str) -> Result<Option<String>, String> {
        let secrets = secrets().lock().map_err(|e| e.to_string())?;
        Ok(secrets.get(account).cloned())
    }

    pub(super) fn delete(account: &str) -> Result<(), String> {
        let mut secrets = secrets().lock().map_err(|e| e.to_string())?;
        secrets
            .remove(account)
            .map(|_| ())
            .ok_or_else(|| missing_secret(account))
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builds_keep_secrets_in_memory() {
        let account = "secret-store-test/memory-round-trip";
        assert!(load_secret(account).is_err());

        store_secret(account, "first").unwrap();
        store_secret(account, "second").unwrap();
        assert_eq!(load_secret(account).unwrap(), "second");

        delete_secret(account).unwrap();
        assert!(load_secret(account).is_err());
        assert!(delete_secret(account).is_err());
    }

    #[test]
    fn a_missing_secret_loads_as_none() {
        let account = "secret-store-test/load-if-present";
        assert_eq!(load_secret_if_present(account), Ok(None));

        store_secret(account, "value").unwrap();
        assert_eq!(
            load_secret_if_present(account),
            Ok(Some("value".to_string()))
        );

        delete_secret(account).unwrap();
        assert_eq!(load_secret_if_present(account), Ok(None));
    }

    #[test]
    fn only_a_missing_keychain_entry_counts_as_absent() {
        assert_eq!(
            absent_entry_as_none(Ok("value".to_string())),
            Ok(Some("value".to_string()))
        );
        assert_eq!(absent_entry_as_none(Err(keyring::Error::NoEntry)), Ok(None));
    }

    #[test]
    fn a_failed_keychain_read_is_an_error() {
        let denied = keyring::Error::PlatformFailure("user denied access".into());
        assert!(absent_entry_as_none(Err(denied)).is_err());

        let locked = keyring::Error::NoStorageAccess("keychain is locked".into());
        assert!(absent_entry_as_none(Err(locked)).is_err());

        let not_text = keyring::Error::BadEncoding(vec![0xff]);
        assert!(absent_entry_as_none(Err(not_text)).is_err());
    }
}

#[cfg(all(test, debug_assertions))]
mod file_tests {
    use super::{file, parse_dev_backend, DevBackend};
    use tempfile::TempDir;

    #[test]
    fn file_backend_round_trips_and_keeps_other_accounts() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nested").join("dev-secrets.json");

        file::store(&path, "anthropic", "key-a").unwrap();
        file::store(&path, "openai", "key-b").unwrap();
        file::store(&path, "anthropic", "key-c").unwrap();

        assert_eq!(file::load(&path, "anthropic").unwrap(), "key-c");
        assert_eq!(file::load(&path, "openai").unwrap(), "key-b");

        file::delete(&path, "anthropic").unwrap();
        assert!(file::load(&path, "anthropic").is_err());
        assert_eq!(file::load(&path, "openai").unwrap(), "key-b");
    }

    #[test]
    fn file_backend_reports_a_missing_account() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("dev-secrets.json");

        assert!(file::load(&path, "absent").is_err());
        assert!(file::delete(&path, "absent").is_err());
        assert!(!path.exists(), "a failed delete must not create the file");
    }

    #[test]
    fn file_backend_tells_a_missing_account_from_an_unreadable_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("dev-secrets.json");

        assert_eq!(file::load_if_present(&path, "anthropic"), Ok(None));
        assert!(!path.exists(), "a read must not create the file");

        file::store(&path, "anthropic", "key-a").unwrap();
        assert_eq!(file::load_if_present(&path, "openai"), Ok(None));
        assert_eq!(
            file::load_if_present(&path, "anthropic"),
            Ok(Some("key-a".to_string()))
        );

        std::fs::write(&path, "not json").unwrap();
        assert!(file::load_if_present(&path, "anthropic").is_err());
    }

    #[test]
    fn file_backend_keeps_every_secret_under_concurrent_writers() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("dev-secrets.json");

        std::thread::scope(|scope| {
            for writer in 0..16 {
                let path = &path;
                scope.spawn(move || {
                    file::store(path, &format!("account-{}", writer), "value").unwrap();
                });
            }
        });

        for writer in 0..16 {
            assert!(file::load(&path, &format!("account-{}", writer)).is_ok());
        }
    }

    #[test]
    fn a_parse_error_does_not_quote_the_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("dev-secrets.json");
        std::fs::write(&path, "{\"anthropic\": 1234567890123}").unwrap();

        let error = file::load(&path, "anthropic").unwrap_err();

        assert!(!error.contains("1234567890123"), "error was: {error}");
    }

    #[test]
    fn file_backend_preserves_secrets_that_need_json_escaping() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("dev-secrets.json");
        let secret = "{\"client_id\":\"a\\b\",\"note\":\"line one\nline two\"}";

        file::store(&path, "mcp_oauth_creds_fixture", secret).unwrap();

        assert_eq!(file::load(&path, "mcp_oauth_creds_fixture").unwrap(), secret);
    }

    #[test]
    fn file_backend_refuses_to_overwrite_a_file_it_cannot_parse() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("dev-secrets.json");
        std::fs::write(&path, "not json").unwrap();

        assert!(file::store(&path, "anthropic", "key").is_err());
        assert!(file::load(&path, "anthropic").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not json");
    }

    #[cfg(unix)]
    #[test]
    fn file_backend_writes_owner_only_files_and_leaves_no_temp_file() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("dev-secrets.json");

        file::store(&path, "anthropic", "key-a").unwrap();
        file::store(&path, "openai", "key-b").unwrap();

        let mut names: Vec<String> = std::fs::read_dir(tmp.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["dev-secrets.json", "dev-secrets.json.lock"]);

        for name in names {
            let mode = std::fs::metadata(tmp.path().join(name))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn debug_builds_default_to_the_file_backend() {
        assert_eq!(parse_dev_backend(None), Ok(DevBackend::File));
        assert_eq!(parse_dev_backend(Some("file")), Ok(DevBackend::File));
        assert_eq!(parse_dev_backend(Some("system")), Ok(DevBackend::System));
    }

    #[test]
    fn an_unknown_backend_name_is_an_error() {
        assert!(parse_dev_backend(Some("")).is_err());
        assert!(parse_dev_backend(Some("System")).is_err());
        assert!(parse_dev_backend(Some("keychain")).is_err());
    }
}
