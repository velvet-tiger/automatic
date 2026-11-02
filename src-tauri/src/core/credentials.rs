use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::agents::credential::{
    delete_gateway_token as remove_stored_gateway_token, load_gateway_token, store_gateway_token,
};
use super::agents::known_agents;
use super::secret_store;
use super::settings::{read_settings, write_settings, Settings};
use super::store_reads::{FailedReads, StoreRead};

// ── Gateway tokens ───────────────────────────────────────────────────────────

/// Store the Cloudflare AI Gateway token for a configured gateway and record
/// that it is there. Any plaintext copy in `settings.json` goes.
pub fn save_gateway_token(agent_id: &str, token: &str) -> Result<(), String> {
    if token.is_empty() {
        return Err("The gateway token must not be empty.".to_string());
    }
    if !read_settings()?.agent_gateways.contains_key(agent_id) {
        return Err(format!("No gateway is configured for '{}'.", agent_id));
    }
    store_gateway_token(agent_id, token)?;

    // Read again: a keychain prompt can hold the store call for a while.
    let mut settings = read_settings()?;
    if let Some(gateway) = settings.agent_gateways.get_mut(agent_id) {
        gateway.cf_token = None;
        gateway.cf_token_stored = true;
    }
    write_settings(&settings)
}

/// Remove the Cloudflare AI Gateway token for `agent_id`, from the secret
/// store and from `settings.json`.
pub fn delete_gateway_token(agent_id: &str) -> Result<(), String> {
    let mut settings = read_settings()?;
    let Some(gateway) = settings.agent_gateways.get_mut(agent_id) else {
        return Ok(());
    };
    if gateway.cf_token_stored {
        remove_stored_gateway_token(agent_id)?;
    }
    gateway.cf_token = None;
    gateway.cf_token_stored = false;
    write_settings(&settings)
}

/// Move gateway tokens from `settings.json` to the secret store. Returns how
/// many it moved.
///
/// A token leaves the file only after the store has given it back, so a store
/// that fails loses nothing and the next start tries again. A plaintext token
/// replaces a stored one: it exists only where no move has finished, or where
/// an older build wrote it, so it is the newer value. Safe to run in several
/// processes at once, because each writes the same value.
pub fn migrate_gateway_tokens() -> Result<usize, String> {
    let settings = read_settings()?;
    let mut moved = Vec::new();
    let mut failures = Vec::new();
    for (agent_id, gateway) in &settings.agent_gateways {
        let Some(token) = &gateway.cf_token else {
            continue;
        };
        match move_gateway_token(agent_id, token) {
            Ok(()) => moved.push((agent_id.clone(), token.clone())),
            Err(e) => failures.push(format!("'{}': {}", agent_id, e)),
        }
    }

    if !moved.is_empty() {
        // Read again: a keychain prompt can hold the moves for a while, and
        // settings saved in that time must survive.
        let mut settings = read_settings()?;
        mark_gateway_tokens_moved(&mut settings, &moved);
        write_settings(&settings)?;
    }
    if failures.is_empty() {
        Ok(moved.iter().filter(|(_, token)| !token.is_empty()).count())
    } else {
        Err(format!(
            "could not move a gateway token to the secret store, so it stays in settings.json: {}",
            failures.join("; ")
        ))
    }
}

/// An empty token has nothing to move, and counts as no token.
fn move_gateway_token(agent_id: &str, token: &str) -> Result<(), String> {
    if token.is_empty() {
        return Ok(());
    }
    store_gateway_token(agent_id, token)?;
    confirm_token_stored(load_gateway_token(agent_id), token)
}

fn confirm_token_stored(read_back: Result<Option<String>, String>, token: &str) -> Result<(), String> {
    match read_back? {
        Some(stored) if stored == token => Ok(()),
        _ => Err("the secret store did not give the token back".to_string()),
    }
}

/// Clear the plaintext tokens in `moved`. A token the user changed since the
/// move stays, and the next start moves it.
fn mark_gateway_tokens_moved(settings: &mut Settings, moved: &[(String, String)]) {
    for (agent_id, token) in moved {
        if let Some(gateway) = settings.agent_gateways.get_mut(agent_id) {
            if gateway.cf_token.as_ref() == Some(token) {
                gateway.cf_token = None;
                gateway.cf_token_stored = !token.is_empty();
            }
        }
    }
}

// ── API Keys ─────────────────────────────────────────────────────────────────

/// Remembers, per provider, whether a key is present in the keychain.
///
/// Existence can only be established by reading the secret, and callers ask
/// repeatedly: every provider is probed whenever the agent-features toggle is
/// evaluated. On macOS each read is a separate access check that can raise its
/// own password dialog, so the answer is kept for the life of the process and
/// invalidated whenever this process writes or deletes a key.
fn presence_cache() -> &'static Mutex<HashMap<String, bool>> {
    static CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The key presence reads that failed, for [`StoreRead::Background`] checks.
fn failed_reads() -> &'static FailedReads {
    static FAILED: OnceLock<FailedReads> = OnceLock::new();
    FAILED.get_or_init(FailedReads::default)
}

fn set_cached_presence(provider: &str, present: bool) {
    if let Ok(mut cache) = presence_cache().lock() {
        cache.insert(provider.to_string(), present);
    }
}

/// Whether at least one known provider has a key in the keychain.
///
/// A key that is found answers the question whatever the other reads did.
/// When none is found and a read failed, the answer is not known, and that
/// read's error is returned.
pub(super) fn any_known_api_key_stored(read: StoreRead) -> Result<bool, String> {
    let known = known_agents();
    any_api_key_stored(known.iter().map(|id| id.as_str()), read)
}

/// [`any_known_api_key_stored`] for the given providers.
fn any_api_key_stored<'a>(
    providers: impl Iterator<Item = &'a str>,
    read: StoreRead,
) -> Result<bool, String> {
    let mut failed_read = None;
    for provider in providers {
        match api_key_presence(provider, read) {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(e) => failed_read = Some(e),
        }
    }
    match failed_read {
        Some(e) => Err(e),
        None => Ok(false),
    }
}

/// Best-effort write of `agent_features_enabled`. Errors reading or writing
/// settings are swallowed — failing to update the toggle should never block
/// the keychain operation that triggered it.
fn set_agent_features_enabled(value: bool) {
    if let Ok(mut settings) = read_settings() {
        if settings.agent_features_enabled != Some(value) {
            settings.agent_features_enabled = Some(value);
            let _ = write_settings(&settings);
        }
    }
}

pub fn save_api_key(provider: &str, key: &str) -> Result<(), String> {
    let had_keys_before = any_known_api_key_stored(StoreRead::Now);
    secret_store::store_secret(provider, key)?;
    set_cached_presence(provider, true);

    // Auto-enable agent features the first time a key is added. When the
    // store could not say whether a key was there before, the setting is the
    // user's own choice as far as anyone knows, so it is left alone.
    if had_keys_before == Ok(false) && known_agents().iter().any(|id| id.as_str() == provider) {
        set_agent_features_enabled(true);
    }
    Ok(())
}

pub fn get_api_key(provider: &str) -> Result<String, String> {
    secret_store::load_secret(provider)
}

/// The API key stored for this provider, or `None` when none is stored.
///
/// A store that cannot be read is an error, never `None`.
pub fn get_api_key_if_present(provider: &str) -> Result<Option<String>, String> {
    secret_store::load_secret_if_present(provider)
}

/// Check whether an API key exists in the keyring for this provider without
/// revealing the value.
///
/// The result is cached for the life of the process. A key added or removed by
/// another process is therefore not observed until restart, which is the same
/// staleness the settings snapshot already carries.
///
/// A store that cannot be read is an error. It is not cached, so the next
/// call reads the store again. A locked keychain would otherwise make every
/// stored key look absent until the process restarts.
pub fn has_api_key(provider: &str) -> Result<bool, String> {
    api_key_presence(provider, StoreRead::Now)
}

/// [`has_api_key`] for a caller that says who is asking. A background check
/// reports a recent failed read again without asking the store, as
/// [`StoreRead`] explains.
fn api_key_presence(provider: &str, read: StoreRead) -> Result<bool, String> {
    if let Ok(cache) = presence_cache().lock() {
        if let Some(present) = cache.get(provider) {
            return Ok(*present);
        }
    }
    if let Some(error) = failed_reads().remembered(provider, read) {
        return Err(error);
    }

    let present = secret_store::load_secret_if_present(provider)
        .map(|key| key.is_some())
        .map_err(|e| format!("could not check for an API key for '{}': {}", provider, e));
    failed_reads().record(provider, &present);
    let present = present?;
    set_cached_presence(provider, present);
    Ok(present)
}

/// Remove a stored API key from the keyring.
pub fn delete_api_key(provider: &str) -> Result<(), String> {
    secret_store::delete_secret(provider)?;
    set_cached_presence(provider, false);

    // Auto-disable agent features once no recognised keys remain. A store
    // that cannot be read may still hold a key, so the setting then stays.
    if known_agents().iter().any(|id| id.as_str() == provider)
        && any_known_api_key_stored(StoreRead::Now) == Ok(false)
    {
        set_agent_features_enabled(false);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::secret_store::{set_test_fault, TestFault};

    // Not one of `known_agents()`, so saving it never changes the
    // agent-features setting.
    const PROVIDER: &str = "credentials-test-provider";

    fn with_gateway(agent: &str, cf_token: Option<&str>, test: impl FnOnce(&std::path::Path)) {
        let home = tempfile::tempdir().expect("tempdir");
        crate::core::with_test_home(home.path().to_path_buf(), || {
            let mut settings = Settings::default();
            settings.agent_gateways.insert(
                agent.to_string(),
                crate::core::agents::GatewayConfig {
                    account_id: "account".to_string(),
                    gateway_id: "gateway".to_string(),
                    cf_token: cf_token.map(str::to_string),
                    cf_token_stored: false,
                },
            );
            write_settings(&settings).expect("write settings");
            let path = crate::core::paths::get_automatic_dir()
                .expect("dir")
                .join("settings.json");
            test(&path);
        });
    }

    fn fault_on(agent: &str) -> String {
        format!("cf_gateway_token_{}", agent)
    }

    #[test]
    fn migration_moves_a_token_out_of_the_settings_file() {
        let agent = "gateway-migrate-moves";
        with_gateway(agent, Some("cf-plaintext-token"), |path| {
            assert_eq!(migrate_gateway_tokens(), Ok(1));

            let raw = std::fs::read_to_string(path).expect("read file");
            assert!(!raw.contains("cf-plaintext-token"), "{raw}");
            assert!(!raw.contains("\"cf_token\""), "{raw}");
            assert_eq!(load_gateway_token(agent), Ok(Some("cf-plaintext-token".to_string())));
            let gateway = read_settings().unwrap().agent_gateways[agent].clone();
            assert!(gateway.cf_token_stored);

            let ready = gateway.with_stored_token(agent).expect("token loads");
            assert_eq!(ready.cf_token.as_deref(), Some("cf-plaintext-token"));

            assert_eq!(migrate_gateway_tokens(), Ok(0), "a second run has nothing to do");
            assert_eq!(load_gateway_token(agent), Ok(Some("cf-plaintext-token".to_string())));
        });
    }

    #[test]
    fn migration_keeps_the_token_when_the_store_fails() {
        let agent = "gateway-migrate-store-fails";
        with_gateway(agent, Some("cf-kept-token"), |path| {
            set_test_fault(&fault_on(agent), Some(TestFault::Unreadable));
            let failed = migrate_gateway_tokens();
            set_test_fault(&fault_on(agent), None);

            let error = failed.expect_err("the store failed");
            assert!(error.contains(agent), "{error}");
            assert!(!error.contains("cf-kept-token"), "{error}");
            let raw = std::fs::read_to_string(path).expect("read file");
            assert!(raw.contains("cf-kept-token"), "{raw}");

            let gateway = read_settings().unwrap().agent_gateways[agent].clone();
            assert!(!gateway.cf_token_stored);
            let ready = gateway.with_stored_token(agent).expect("still usable");
            assert_eq!(ready.cf_token.as_deref(), Some("cf-kept-token"));

            assert_eq!(migrate_gateway_tokens(), Ok(1), "the next start retries");
        });
    }

    #[test]
    fn migration_drops_an_empty_token_without_storing_it() {
        let agent = "gateway-migrate-empty";
        with_gateway(agent, Some(""), |_| {
            assert_eq!(migrate_gateway_tokens(), Ok(0));

            let gateway = read_settings().unwrap().agent_gateways[agent].clone();
            assert_eq!(gateway.cf_token, None);
            assert!(!gateway.cf_token_stored);
        });
    }

    #[test]
    fn a_token_the_store_does_not_give_back_is_not_confirmed() {
        assert_eq!(confirm_token_stored(Ok(Some("t".to_string())), "t"), Ok(()));
        assert!(confirm_token_stored(Ok(Some("other".to_string())), "t").is_err());
        assert!(confirm_token_stored(Ok(None), "t").is_err());
        assert!(confirm_token_stored(Err("locked".to_string()), "t").is_err());
    }

    #[test]
    fn a_token_changed_during_the_move_stays_for_the_next_start() {
        let agent = "gateway-migrate-changed";
        with_gateway(agent, Some("new"), |_| {
            let mut settings = read_settings().unwrap();
            mark_gateway_tokens_moved(&mut settings, &[(agent.to_string(), "old".to_string())]);
            assert_eq!(settings.agent_gateways[agent].cf_token.as_deref(), Some("new"));
        });
    }

    #[test]
    fn saving_a_token_replaces_the_plaintext_copy() {
        let agent = "gateway-save-replaces";
        with_gateway(agent, Some("cf-old-token"), |path| {
            save_gateway_token(agent, "cf-new-token").expect("save");

            let raw = std::fs::read_to_string(path).expect("read file");
            assert!(!raw.contains("cf-old-token") && !raw.contains("cf-new-token"), "{raw}");
            assert!(read_settings().unwrap().agent_gateways[agent].cf_token_stored);
            assert_eq!(load_gateway_token(agent), Ok(Some("cf-new-token".to_string())));

            delete_gateway_token(agent).expect("delete");
            assert_eq!(load_gateway_token(agent), Ok(None));
            assert!(!read_settings().unwrap().agent_gateways[agent].cf_token_stored);
        });
    }

    #[test]
    fn a_token_needs_a_gateway_and_a_value() {
        let agent = "gateway-save-refused";
        with_gateway(agent, None, |_| {
            assert!(save_gateway_token("gateway-save-unknown", "token").is_err());
            assert!(save_gateway_token(agent, "").is_err());
            assert_eq!(load_gateway_token("gateway-save-unknown"), Ok(None));
            assert_eq!(delete_gateway_token("gateway-save-unknown"), Ok(()));
        });
    }

    #[test]
    fn a_refused_delete_keeps_the_record_of_the_token() {
        let agent = "gateway-delete-refused";
        with_gateway(agent, None, |_| {
            save_gateway_token(agent, "cf-token").expect("save");

            set_test_fault(&fault_on(agent), Some(TestFault::DeleteRefused));
            let deleted = delete_gateway_token(agent);
            set_test_fault(&fault_on(agent), None);

            assert!(deleted.is_err());
            assert!(read_settings().unwrap().agent_gateways[agent].cf_token_stored);
            delete_gateway_token(agent).expect("clean up");
        });
    }

    #[test]
    fn api_key_presence_follows_save_and_delete() {
        assert_eq!(has_api_key(PROVIDER), Ok(false));

        save_api_key(PROVIDER, "key").unwrap();
        assert_eq!(has_api_key(PROVIDER), Ok(true));
        assert_eq!(get_api_key(PROVIDER).unwrap(), "key");
        assert_eq!(get_api_key_if_present(PROVIDER), Ok(Some("key".to_string())));

        delete_api_key(PROVIDER).unwrap();
        assert_eq!(has_api_key(PROVIDER), Ok(false));
        assert!(get_api_key(PROVIDER).is_err());
        assert_eq!(get_api_key_if_present(PROVIDER), Ok(None));
    }

    // The store and the presence cache are shared by the whole test process,
    // so each test below uses a provider name no other test uses.

    #[test]
    fn a_store_that_cannot_be_read_is_an_error_and_is_not_remembered() {
        let provider = "credentials-test-unreadable-provider";
        secret_store::store_secret(provider, "key").expect("store key");

        set_test_fault(provider, Some(TestFault::Unreadable));
        let err = has_api_key(provider).expect_err("the store cannot be read");
        assert!(err.contains(provider), "unexpected error: {err}");
        assert!(get_api_key_if_present(provider).is_err());

        // The failed read left nothing in the cache: the key is found as
        // soon as the store can be read.
        set_test_fault(provider, None);
        assert_eq!(has_api_key(provider), Ok(true));
    }

    #[test]
    fn a_stored_key_answers_for_the_group_whatever_the_other_reads_did() {
        let stored = "credentials-test-group-stored-provider";
        let unreadable = "credentials-test-group-unreadable-provider";
        let absent = "credentials-test-group-absent-provider";
        secret_store::store_secret(stored, "key").expect("store key");

        set_test_fault(unreadable, Some(TestFault::Unreadable));
        let with_a_key =
            any_api_key_stored([unreadable, absent, stored].into_iter(), StoreRead::Now);
        let without_a_key = any_api_key_stored([absent, unreadable].into_iter(), StoreRead::Now);
        set_test_fault(unreadable, None);

        assert_eq!(with_a_key, Ok(true));
        // No key was found and one read failed, so "none stored" is not
        // known. The agent-features setting is left alone on this answer.
        assert!(without_a_key.is_err());
        assert_eq!(any_api_key_stored([absent].into_iter(), StoreRead::Now), Ok(false));
    }

    #[test]
    fn a_background_check_does_not_read_a_store_that_just_failed() {
        let provider = "credentials-test-background-provider";
        secret_store::store_secret(provider, "key").expect("store key");
        set_test_fault(provider, Some(TestFault::Unreadable));
        assert!(has_api_key(provider).is_err());
        set_test_fault(provider, None);

        // The store would answer now. The background check does not ask it,
        // so a locked keychain is not prompted for on every timer tick.
        assert!(api_key_presence(provider, StoreRead::Background).is_err());

        // A check the user asked for reads, and its answer is then used by
        // background checks too.
        assert_eq!(has_api_key(provider), Ok(true));
        assert_eq!(api_key_presence(provider, StoreRead::Background), Ok(true));
    }

    #[test]
    fn a_missing_key_is_remembered_as_absent() {
        let provider = "credentials-test-absent-provider";
        assert_eq!(has_api_key(provider), Ok(false));

        // A fault now makes no difference: the answer comes from the cache.
        set_test_fault(provider, Some(TestFault::Unreadable));
        let cached = has_api_key(provider);
        set_test_fault(provider, None);

        assert_eq!(cached, Ok(false));
    }
}
