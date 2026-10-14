//! One interface for every secret Automatic stores: API keys, OAuth tokens,
//! account tokens and the env-encryption key.
//!
//! The backend depends on the build:
//!
//! - **Release builds** use the OS keychain: the macOS Keychain, the Windows
//!   Credential Manager, and on Linux the Secret Service over D-Bus (GNOME
//!   Keyring, KWallet, KeePassXC).
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
//!
//! # Linux
//!
//! The Secret Service needs a D-Bus session bus and a provider with a default
//! keyring. Without them every call from a release build returns an error.
//! There is no fallback store.
//!
//! Every call first confirms that the default keyring is unlocked. Some
//! providers answer a search of a locked keyring with "no items", and a
//! stored secret would then look absent.
//!
//! A locked keyring makes the provider show an unlock prompt, and the call
//! waits for it. [`set_unlock_prompt`] decides how long. A process an agent
//! started (`mcp-serve`, `mcp-proxy`) shows none, because nobody may be
//! there to answer. Every other process waits a bounded time. After a call
//! finds the keyring still locked, calls in the next few seconds fail at once
//! with the same error instead of prompting again.
//!
//! Older builds kept secrets in the kernel keyring, which the kernel empties
//! on reboot. A read that finds nothing in the Secret Service looks there and
//! moves what it finds to the Secret Service. A store or a delete removes the
//! kernel keyring copy, so an old value cannot come back.

/// Store `secret` under `account`, replacing any value already there.
pub fn store_secret(account: &str, secret: &str) -> Result<(), String> {
    backend::store(account, secret)
}

/// Load the secret stored under `account`.
///
/// Returns an error when nothing is stored, as well as when the backend
/// fails. A caller that asks whether a secret is stored uses
/// [`load_secret_if_present`]. `.is_ok()` here would report a store that
/// cannot be read as "nothing stored".
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

/// Whether this process may show the keyring's unlock prompt.
///
/// Only the Linux Secret Service asks this. The macOS Keychain and the
/// Windows Credential Manager show their own dialogs, which no setting here
/// changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnlockPrompt {
    /// Show the prompt and wait up to this many seconds for an answer.
    WaitUpTo(u64),
    /// Never show the prompt. A locked keyring is an error at once.
    Never,
}

/// How long a process that sets no policy waits for the unlock prompt.
/// Long enough to type a password, short enough that a prompt nobody sees
/// does not hold the call for good.
#[cfg(any(test, target_os = "linux"))]
const DEFAULT_UNLOCK_WAIT_SECS: u64 = 60;

static UNLOCK_PROMPT: std::sync::OnceLock<UnlockPrompt> = std::sync::OnceLock::new();

/// Set this process's unlock prompt policy. Call once at startup, before
/// the first secret is read. A later call has no effect.
///
/// A process an agent starts should pass [`UnlockPrompt::Never`]: an
/// `mcp-proxy` waiting on a prompt nobody answers holds up the agent that
/// started it. Without a call the process waits 60 seconds.
pub fn set_unlock_prompt(policy: UnlockPrompt) {
    let _ = UNLOCK_PROMPT.set(policy);
}

/// This process's unlock prompt policy.
#[cfg(any(test, target_os = "linux"))]
fn unlock_prompt() -> UnlockPrompt {
    UNLOCK_PROMPT
        .get()
        .copied()
        .unwrap_or(UnlockPrompt::WaitUpTo(DEFAULT_UNLOCK_WAIT_SECS))
}

/// The prompt timeout the Secret Service client takes for `policy`. Zero
/// makes it cancel the prompt without showing it.
#[cfg(any(test, target_os = "linux"))]
fn prompt_timeout_secs(policy: UnlockPrompt) -> u64 {
    match policy {
        UnlockPrompt::WaitUpTo(seconds) => seconds,
        UnlockPrompt::Never => 0,
    }
}

/// What a locked keyring is called in an error, by policy. A process that
/// never prompts says so, or the user cannot tell why no prompt appeared.
#[cfg(any(test, target_os = "linux"))]
fn locked_keyring_detail(policy: UnlockPrompt) -> &'static str {
    match policy {
        UnlockPrompt::WaitUpTo(_) => "the default keyring is still locked",
        UnlockPrompt::Never => {
            "the default keyring is locked, and this process does not show an unlock prompt \
             because an agent started it. Unlock the keyring in your desktop session, or open \
             Automatic and let it ask"
        }
    }
}

/// How long a keyring found still locked stays known as locked. Calls in
/// that time fail at once instead of prompting again. One sync can read many
/// secrets, and a prompt for each would follow one "Cancel" with another.
#[cfg(any(test, target_os = "linux"))]
const LOCKED_RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(10);

/// When the keyring was last found unusable, and the error then.
#[cfg(any(test, target_os = "linux"))]
type LockedSince = std::sync::Mutex<Option<(std::time::Instant, String)>>;

/// The error from `memo` when it is younger than `max_age`.
#[cfg(any(test, target_os = "linux"))]
fn recent_failure(memo: &LockedSince, max_age: std::time::Duration) -> Option<String> {
    let memo = memo.lock().ok()?;
    let (failed_at, error) = memo.as_ref()?;
    (failed_at.elapsed() < max_age).then(|| error.clone())
}

/// Keep the outcome of a check of the keyring: its error when it failed,
/// nothing when it worked.
#[cfg(any(test, target_os = "linux"))]
fn remember_check(memo: &LockedSince, outcome: &Result<(), String>) {
    if let Ok(mut memo) = memo.lock() {
        *memo = outcome
            .as_ref()
            .err()
            .map(|error| (std::time::Instant::now(), error.clone()));
    }
}

/// Check the backend configuration. Call once at startup.
///
/// A mistyped `AUTOMATIC_KEYCHAIN` makes every call to the store fail. This
/// check reports it at startup, with the variable's name, before a sync or a
/// sign-in runs into it. Release and test builds have nothing to configure.
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
/// errors.
fn absent_entry_as_none(read: Result<String, keyring::Error>) -> Result<Option<String>, String> {
    match read {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(keychain_error(e)),
    }
}

/// The message a caller sees for a keychain failure.
///
/// On Linux a failure to reach or open the store names the Secret Service.
/// The D-Bus text alone ("Unable to autolaunch a dbus-daemon") does not tell
/// a user what is missing.
fn keychain_error(error: keyring::Error) -> String {
    #[cfg(target_os = "linux")]
    if matches!(
        error,
        keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_)
    ) {
        return secret_service_unavailable(&error.to_string());
    }
    error.to_string()
}

#[cfg(any(test, target_os = "linux"))]
fn secret_service_unavailable(detail: &str) -> String {
    format!(
        "Automatic keeps secrets in the Secret Service on Linux, and could not use it: {}. \
         It needs a D-Bus session bus and a Secret Service provider (GNOME Keyring, KWallet \
         or KeePassXC) with an unlocked default keyring.",
        detail
    )
}

/// Return the stored secret. When nothing is stored, return the copy an older
/// build left in the kernel keyring, after passing it to `adopt`.
///
/// `adopt` moves the secret to the Secret Service. If the move fails this is
/// an error: a secret handed out from the kernel keyring alone would be gone
/// after the next reboot, with nothing to tell the user why.
///
/// `stored_again` reads the Secret Service a second time. Several Automatic
/// processes start together after an update, and another one may move the
/// secret between this process's two reads: it would be in neither place
/// this process looked. The mover writes before it deletes, so a second read
/// finds what it moved. Without it the env-encryption key would look absent
/// and the next encrypt would replace it.
///
/// `legacy` is only called when nothing is stored, `adopt` only when `legacy`
/// found a secret, and `stored_again` only when it did not.
#[cfg(any(test, target_os = "linux"))]
fn stored_or_legacy(
    stored: Result<Option<String>, String>,
    legacy: impl FnOnce() -> Result<Option<String>, String>,
    adopt: impl FnOnce(&str) -> Result<(), String>,
    stored_again: impl FnOnce() -> Result<Option<String>, String>,
) -> Result<Option<String>, String> {
    if let Some(secret) = stored? {
        return Ok(Some(secret));
    }
    let Some(secret) = legacy()? else {
        return stored_again();
    };
    adopt(&secret).map_err(|e| {
        format!(
            "found this secret in the kernel keyring but could not move it to the Secret Service: {}",
            e
        )
    })?;
    Ok(Some(secret))
}

/// Finish a delete that covers both Linux stores. `stored` is the result of
/// the Secret Service delete, and `delete_legacy` removes the kernel keyring
/// copy and reports whether there was one.
///
/// A failed Secret Service delete returns before `delete_legacy` runs. The
/// kernel keyring copy may be the only one left for a later migration, and it
/// must not be lost to a delete that did not work.
///
/// Deleting only the kernel keyring copy is a successful delete: the secret
/// was stored and is now gone.
#[cfg(any(test, target_os = "linux"))]
fn deleted_from_either(
    stored: Result<(), keyring::Error>,
    delete_legacy: impl FnOnce() -> Result<bool, String>,
) -> Result<(), String> {
    let stored_removed = match stored {
        Ok(()) => true,
        Err(keyring::Error::NoEntry) => false,
        Err(e) => return Err(keychain_error(e)),
    };
    let legacy_removed = delete_legacy().map_err(|e| {
        format!(
            "{}. Until it is removed or the machine restarts, a read can bring the secret back",
            e
        )
    })?;
    if stored_removed || legacy_removed {
        Ok(())
    } else {
        Err(keychain_error(keyring::Error::NoEntry))
    }
}

// ── Backend selection ────────────────────────────────────────────────────────

#[cfg(test)]
use memory as backend;

#[cfg(test)]
pub(crate) use memory::{set_fault as set_test_fault, Fault as TestFault};

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

    use super::keychain_error;

    // Debug builds use a separate service, so dev and release entries never
    // collide.
    #[cfg(debug_assertions)]
    const KEYCHAIN_SERVICE: &str = "automatic_desktop_dev";
    #[cfg(not(debug_assertions))]
    const KEYCHAIN_SERVICE: &str = "automatic_desktop";

    fn entry(account: &str) -> Result<Entry, String> {
        Entry::new(KEYCHAIN_SERVICE, account).map_err(keychain_error)
    }

    #[cfg(not(target_os = "linux"))]
    pub(super) fn store(account: &str, secret: &str) -> Result<(), String> {
        entry(account)?.set_password(secret).map_err(keychain_error)
    }

    #[cfg(not(target_os = "linux"))]
    pub(super) fn load(account: &str) -> Result<String, String> {
        entry(account)?.get_password().map_err(keychain_error)
    }

    #[cfg(not(target_os = "linux"))]
    pub(super) fn load_if_present(account: &str) -> Result<Option<String>, String> {
        super::absent_entry_as_none(entry(account)?.get_password())
    }

    #[cfg(not(target_os = "linux"))]
    pub(super) fn delete(account: &str) -> Result<(), String> {
        entry(account)?.delete_credential().map_err(keychain_error)
    }

    #[cfg(target_os = "linux")]
    pub(super) fn load(account: &str) -> Result<String, String> {
        load_if_present(account)?.ok_or_else(|| keyring::Error::NoEntry.to_string())
    }

    #[cfg(target_os = "linux")]
    pub(super) fn load_if_present(account: &str) -> Result<Option<String>, String> {
        let entry = unlocked_entry(account)?;
        super::stored_or_legacy(
            super::absent_entry_as_none(entry.get_password()),
            || kernel_keyring::load(account),
            |secret| store(account, secret),
            || super::absent_entry_as_none(entry.get_password()),
        )
    }

    /// The Secret Service now holds the current value, so the copy an older
    /// build left in the kernel keyring goes. If it stayed, a read would bring
    /// the old value back whenever the Secret Service item was lost.
    #[cfg(target_os = "linux")]
    pub(super) fn store(account: &str, secret: &str) -> Result<(), String> {
        unlocked_entry(account)?
            .set_password(secret)
            .map_err(keychain_error)?;
        kernel_keyring::delete(account)
            .map(|_removed| ())
            .map_err(|e| format!("the secret was stored, but {}", e))
    }

    #[cfg(target_os = "linux")]
    pub(super) fn delete(account: &str) -> Result<(), String> {
        let stored = unlocked_entry(account)?.delete_credential();
        super::deleted_from_either(stored, || kernel_keyring::delete(account))
    }

    /// An entry in a Secret Service whose default keyring is unlocked.
    #[cfg(target_os = "linux")]
    fn unlocked_entry(account: &str) -> Result<Entry, String> {
        confirm_default_keyring_unlocked()?;
        entry(account)
    }

    /// Ask the provider to unlock the default keyring, and fail if it stays
    /// locked.
    ///
    /// GNOME Keyring lists locked items in a search, so a read of a locked
    /// keyring fails unless the user unlocks it. KeePassXC's source shows a
    /// locked database answering with no items at all. Without this check the
    /// stored env-encryption key would look absent there, and the next encrypt
    /// would replace it.
    ///
    /// The prompt waits as long as [`super::set_unlock_prompt`] allows. A
    /// check that fails is remembered for a few seconds, and the calls in
    /// that time fail with the same error without prompting again.
    #[cfg(target_os = "linux")]
    fn confirm_default_keyring_unlocked() -> Result<(), String> {
        static LOCKED_SINCE: super::LockedSince = std::sync::Mutex::new(None);

        if let Some(error) = super::recent_failure(&LOCKED_SINCE, super::LOCKED_RETRY_AFTER) {
            return Err(error);
        }
        let outcome = check_default_keyring_unlocked(super::unlock_prompt());
        super::remember_check(&LOCKED_SINCE, &outcome);
        outcome
    }

    #[cfg(target_os = "linux")]
    fn check_default_keyring_unlocked(policy: super::UnlockPrompt) -> Result<(), String> {
        use dbus_secret_service::{EncryptionType, SecretService};
        use keyring::secret_service::{decode_error, get_collection};
        use keyring::Error;

        // No secret crosses this session, so it needs no encryption. The
        // timeout bounds the unlock prompt that `get_collection` may raise.
        // Without it the call waits for an answer for as long as a year.
        let service = SecretService::connect_with_max_prompt_timeout(
            EncryptionType::Plain,
            super::prompt_timeout_secs(policy),
        )
        .map_err(|e| keychain_error(decode_error(e)))?;
        // `get_collection` requests the unlock. The check below reads the
        // result, because a provider may accept the request and stay locked.
        // A prompt that was dismissed, timed out or never shown comes back
        // as `Prompt`, and a keyring that stayed locked as `Locked`. Other
        // failures, such as no default keyring, keep their own message.
        let collection = match get_collection(&service, "default") {
            Ok(collection) => collection,
            Err(Error::NoStorageAccess(inner))
                if matches!(
                    inner.downcast_ref::<dbus_secret_service::Error>(),
                    Some(dbus_secret_service::Error::Prompt | dbus_secret_service::Error::Locked)
                ) =>
            {
                return Err(super::secret_service_unavailable(super::locked_keyring_detail(
                    policy,
                )));
            }
            Err(e) => return Err(keychain_error(e)),
        };
        match collection.is_locked() {
            Ok(false) => Ok(()),
            Ok(true) => Err(super::secret_service_unavailable(
                super::locked_keyring_detail(policy),
            )),
            Err(e) => Err(keychain_error(decode_error(e))),
        }
    }

    /// The store builds used before the Secret Service. Nothing is written
    /// here any more. It is read so that a user who updates keeps the secrets
    /// stored since their last reboot, and each secret is removed from it
    /// once the Secret Service holds it.
    #[cfg(target_os = "linux")]
    mod kernel_keyring {
        use keyring::keyutils::KeyutilsCredential;
        use keyring::{Entry, Error};

        /// `None` when the kernel keyring cannot be opened at all, as in a
        /// container whose seccomp profile blocks `keyctl`. No build could
        /// have stored a secret there, so there is nothing to carry over.
        fn entry(account: &str) -> Option<Entry> {
            KeyutilsCredential::new_with_target(None, super::KEYCHAIN_SERVICE, account)
                .ok()
                .map(|credential| Entry::new_with_credential(Box::new(credential)))
        }

        /// The `keyring` crate reports a denied, revoked or expired key as
        /// `NoEntry`. Such a key cannot be read, so it counts as absent.
        pub(super) fn load(account: &str) -> Result<Option<String>, String> {
            let Some(entry) = entry(account) else {
                return Ok(None);
            };
            match entry.get_password() {
                Ok(secret) => Ok(Some(secret)),
                Err(Error::NoEntry) => Ok(None),
                Err(e) => Err(format!("failed to read the kernel keyring: {}", e)),
            }
        }

        /// Returns whether a key was removed.
        pub(super) fn delete(account: &str) -> Result<bool, String> {
            let Some(entry) = entry(account) else {
                return Ok(false);
            };
            match entry.delete_credential() {
                Ok(()) => Ok(true),
                Err(Error::NoEntry) => Ok(false),
                Err(e) => Err(format!("failed to delete the kernel keyring copy: {}", e)),
            }
        }
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

    /// A failure a test gives the store for one account. The map itself
    /// cannot fail, and callers have to be tested against a store that does.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Fault {
        /// Every call fails, as when no keychain service can be reached.
        Unreadable,
        /// A delete fails and the secret stays, as when the keychain refuses.
        DeleteRefused,
    }

    fn faults() -> &'static Mutex<HashMap<String, Fault>> {
        static FAULTS: OnceLock<Mutex<HashMap<String, Fault>>> = OnceLock::new();
        FAULTS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    /// Give `account` a fault, or clear it with `None`. The fault applies to
    /// every test in the process, so the account must be one no other test
    /// uses.
    pub(crate) fn set_fault(account: &str, fault: Option<Fault>) {
        let mut faults = faults().lock().expect("fault map lock");
        match fault {
            Some(fault) => faults.insert(account.to_string(), fault),
            None => faults.remove(account),
        };
    }

    fn fault(account: &str) -> Result<Option<Fault>, String> {
        let faults = faults().lock().map_err(|e| e.to_string())?;
        Ok(faults.get(account).copied())
    }

    fn fail_if_unreadable(account: &str) -> Result<(), String> {
        if fault(account)? == Some(Fault::Unreadable) {
            return Err(format!("test fault: the secret store cannot be read for '{}'", account));
        }
        Ok(())
    }

    pub(super) fn store(account: &str, secret: &str) -> Result<(), String> {
        fail_if_unreadable(account)?;
        let mut secrets = secrets().lock().map_err(|e| e.to_string())?;
        secrets.insert(account.to_string(), secret.to_string());
        Ok(())
    }

    pub(super) fn load(account: &str) -> Result<String, String> {
        fail_if_unreadable(account)?;
        let secrets = secrets().lock().map_err(|e| e.to_string())?;
        secrets
            .get(account)
            .cloned()
            .ok_or_else(|| missing_secret(account))
    }

    pub(super) fn load_if_present(account: &str) -> Result<Option<String>, String> {
        fail_if_unreadable(account)?;
        let secrets = secrets().lock().map_err(|e| e.to_string())?;
        Ok(secrets.get(account).cloned())
    }

    pub(super) fn delete(account: &str) -> Result<(), String> {
        fail_if_unreadable(account)?;
        if fault(account)? == Some(Fault::DeleteRefused) {
            return Err(format!("test fault: the secret store refused to delete '{}'", account));
        }
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

    // ── The unlock prompt ────────────────────────────────────────────────────

    #[test]
    fn a_process_that_never_prompts_asks_for_a_zero_timeout() {
        assert_eq!(prompt_timeout_secs(UnlockPrompt::Never), 0);
        assert_eq!(prompt_timeout_secs(UnlockPrompt::WaitUpTo(45)), 45);
    }

    #[test]
    fn a_process_that_sets_no_policy_waits_a_bounded_time() {
        // The test binary sets no policy.
        assert_eq!(unlock_prompt(), UnlockPrompt::WaitUpTo(DEFAULT_UNLOCK_WAIT_SECS));
    }

    #[test]
    fn a_locked_keyring_error_says_why_no_prompt_appeared() {
        let never = secret_service_unavailable(locked_keyring_detail(UnlockPrompt::Never));
        let waited = secret_service_unavailable(locked_keyring_detail(UnlockPrompt::WaitUpTo(60)));

        assert!(never.contains("does not show an unlock prompt"), "{never}");
        assert!(never.contains("open Automatic"), "{never}");
        assert!(waited.contains("still locked"), "{waited}");
        assert!(!waited.contains("does not show"), "{waited}");
    }

    #[test]
    fn a_keyring_found_locked_is_not_asked_again_for_a_few_seconds() {
        let memo: LockedSince = std::sync::Mutex::new(None);
        assert_eq!(recent_failure(&memo, LOCKED_RETRY_AFTER), None);

        remember_check(&memo, &Err("the default keyring is still locked".to_string()));
        assert_eq!(
            recent_failure(&memo, LOCKED_RETRY_AFTER),
            Some("the default keyring is still locked".to_string())
        );
        // Once the time has passed the keyring is checked again.
        assert_eq!(recent_failure(&memo, std::time::Duration::ZERO), None);

        // A check that worked forgets the failure.
        remember_check(&memo, &Ok(()));
        assert_eq!(recent_failure(&memo, LOCKED_RETRY_AFTER), None);
    }

    #[test]
    fn the_secret_service_message_names_what_is_missing() {
        let message = secret_service_unavailable("DBus error: no session bus");
        assert!(message.contains("DBus error: no session bus"));
        assert!(message.contains("Secret Service"));
        assert!(message.contains("D-Bus session bus"));
    }

    // ── stored_or_legacy ─────────────────────────────────────────────────────

    /// Run `stored_or_legacy` and return what it passed to `adopt`, and
    /// whether it called `legacy`. The second read of the store finds
    /// nothing.
    fn read_with_legacy(
        stored: Result<Option<String>, String>,
        legacy: Result<Option<String>, String>,
        adopt_result: Result<(), String>,
    ) -> (Result<Option<String>, String>, bool, Vec<String>) {
        let mut legacy_was_read = false;
        let mut adopted = Vec::new();
        let read = stored_or_legacy(
            stored,
            || {
                legacy_was_read = true;
                legacy
            },
            |secret| {
                adopted.push(secret.to_string());
                adopt_result
            },
            || Ok(None),
        );
        (read, legacy_was_read, adopted)
    }

    /// Run `stored_or_legacy` with nothing in either store on the first
    /// reads, and return whether it read the store a second time.
    fn read_after_both_were_empty(
        stored_again: Result<Option<String>, String>,
    ) -> (Result<Option<String>, String>, bool) {
        let mut store_was_read_again = false;
        let read = stored_or_legacy(
            Ok(None),
            || Ok(None),
            |_| panic!("nothing was found to adopt"),
            || {
                store_was_read_again = true;
                stored_again
            },
        );
        (read, store_was_read_again)
    }

    #[test]
    fn a_secret_another_process_moved_between_the_reads_is_found() {
        // The other process wrote to the Secret Service after this one read
        // it, and deleted the kernel keyring copy before this one read that.
        let (read, store_was_read_again) =
            read_after_both_were_empty(Ok(Some("moved-by-another-process".to_string())));
        assert_eq!(read, Ok(Some("moved-by-another-process".to_string())));
        assert!(store_was_read_again);
    }

    #[test]
    fn a_failed_second_read_is_an_error() {
        let (read, _) = read_after_both_were_empty(Err("keyring is locked".to_string()));
        assert_eq!(read, Err("keyring is locked".to_string()));
    }

    #[test]
    fn a_stored_secret_is_returned_without_reading_the_legacy_store() {
        let (read, legacy_was_read, adopted) = read_with_legacy(
            Ok(Some("current".to_string())),
            Ok(Some("old".to_string())),
            Ok(()),
        );
        assert_eq!(read, Ok(Some("current".to_string())));
        assert!(!legacy_was_read);
        assert!(adopted.is_empty());
    }

    #[test]
    fn a_legacy_secret_is_adopted_and_returned() {
        let (read, _, adopted) = read_with_legacy(Ok(None), Ok(Some("old".to_string())), Ok(()));
        assert_eq!(read, Ok(Some("old".to_string())));
        assert_eq!(adopted, vec!["old"]);
    }

    #[test]
    fn nothing_in_either_store_is_absent() {
        let (read, legacy_was_read, adopted) = read_with_legacy(Ok(None), Ok(None), Ok(()));
        assert_eq!(read, Ok(None));
        assert!(legacy_was_read);
        assert!(adopted.is_empty());

        let (read, store_was_read_again) = read_after_both_were_empty(Ok(None));
        assert_eq!(read, Ok(None));
        assert!(store_was_read_again);
    }

    #[test]
    fn a_failed_read_never_falls_back_to_the_legacy_store() {
        // A locked keyring may hold a newer value than the kernel keyring.
        let (read, legacy_was_read, adopted) = read_with_legacy(
            Err("keyring is locked".to_string()),
            Ok(Some("old".to_string())),
            Ok(()),
        );
        assert_eq!(read, Err("keyring is locked".to_string()));
        assert!(!legacy_was_read);
        assert!(adopted.is_empty());
    }

    #[test]
    fn a_failed_legacy_read_is_an_error() {
        let (read, _, adopted) =
            read_with_legacy(Ok(None), Err("kernel keyring failed".to_string()), Ok(()));
        assert_eq!(read, Err("kernel keyring failed".to_string()));
        assert!(adopted.is_empty());
    }

    #[test]
    fn a_legacy_secret_that_cannot_be_adopted_is_an_error() {
        let secret = "sk-legacy-7f3a91c2";
        let (read, _, adopted) = read_with_legacy(
            Ok(None),
            Ok(Some(secret.to_string())),
            Err("keyring is locked".to_string()),
        );
        let error = read.unwrap_err();
        assert!(error.contains("keyring is locked"), "error was: {error}");
        assert!(
            !error.contains(secret),
            "the error must not quote the secret"
        );
        assert_eq!(adopted, vec![secret]);
    }

    // ── deleted_from_either ──────────────────────────────────────────────────

    /// Run `deleted_from_either` and return whether it tried the legacy
    /// delete.
    fn delete_from_both(
        stored: Result<(), keyring::Error>,
        legacy: Result<bool, String>,
    ) -> (Result<(), String>, bool) {
        let mut legacy_was_tried = false;
        let deleted = deleted_from_either(stored, || {
            legacy_was_tried = true;
            legacy
        });
        (deleted, legacy_was_tried)
    }

    #[test]
    fn deleting_only_the_legacy_copy_is_a_successful_delete() {
        let (deleted, _) = delete_from_both(Err(keyring::Error::NoEntry), Ok(true));
        assert_eq!(deleted, Ok(()));
    }

    #[test]
    fn deleting_nothing_is_an_error() {
        let (deleted, legacy_was_tried) = delete_from_both(Err(keyring::Error::NoEntry), Ok(false));
        assert!(deleted.is_err());
        assert!(legacy_was_tried);
    }

    #[test]
    fn a_stored_secret_that_was_deleted_is_a_successful_delete() {
        assert_eq!(delete_from_both(Ok(()), Ok(false)), (Ok(()), true));
        assert_eq!(delete_from_both(Ok(()), Ok(true)), (Ok(()), true));
    }

    #[test]
    fn a_failed_delete_leaves_the_legacy_copy_alone() {
        // With no usable Secret Service, the kernel keyring copy may be the
        // only one left to migrate later.
        let locked = keyring::Error::NoStorageAccess("keyring is locked".into());
        let (deleted, legacy_was_tried) = delete_from_both(Err(locked), Ok(true));
        assert!(deleted.is_err());
        assert!(!legacy_was_tried);
    }

    #[test]
    fn a_legacy_copy_that_cannot_be_deleted_is_an_error() {
        // The Secret Service delete worked, but a read would bring the
        // secret back from the kernel keyring.
        let (deleted, _) = delete_from_both(Ok(()), Err("kernel keyring failed".to_string()));
        let error = deleted.unwrap_err();
        assert!(
            error.contains("kernel keyring failed"),
            "error was: {error}"
        );
        assert!(
            error.contains("bring the secret back"),
            "error was: {error}"
        );
    }

    // ── Linux store selection and error text ─────────────────────────────────

    #[cfg(target_os = "linux")]
    #[test]
    fn the_default_linux_store_keeps_secrets_across_a_reboot() {
        // The kernel keyring store reports `UntilReboot`. A change to the
        // `keyring` features that brought it back as the default fails here.
        let persistence = keyring::default::default_credential_builder().persistence();
        assert!(matches!(
            persistence,
            keyring::credential::CredentialPersistence::UntilDelete
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_linux_store_failure_names_the_secret_service() {
        let locked = keyring::Error::NoStorageAccess("unlock prompt was dismissed".into());
        assert!(keychain_error(locked).contains("Secret Service"));

        let no_bus = keyring::Error::PlatformFailure("no session bus".into());
        assert!(keychain_error(no_bus).contains("D-Bus session bus"));

        let not_text = keychain_error(keyring::Error::BadEncoding(vec![0xff]));
        assert!(
            !not_text.contains("Secret Service"),
            "error was: {not_text}"
        );
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
