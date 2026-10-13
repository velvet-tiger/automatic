//! Desktop ↔ webapp authentication.
//!
//! Implements the OAuth 2.0 Authorization Code + PKCE flow against the
//! Automatic webapp (`/oauth/authorize`, `/oauth/token`, `/oauth/revoke`,
//! `/api/me`).  The webapp URL is resolved at compile time from the
//! `AUTOMATIC_WEBAPP_URL` env var, falling back to `http://localhost:3000`
//! in debug builds and `https://tryautomatic.app` in release builds.
//!
//! Access tokens, refresh tokens and the authenticated user's profile are
//! stored in the system keychain — they are never written to disk.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use url::Url;

use crate::core::secret_store;

// ── Constants ────────────────────────────────────────────────────────────────

const CLIENT_ID: &str = "automatic-desktop";

const ACCESS_TOKEN_USER: &str = "automatic_account_access_token";
const REFRESH_TOKEN_USER: &str = "automatic_account_refresh_token";
const PROFILE_USER: &str = "automatic_account_profile";

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountProfile {
    pub user_id: String,
    #[serde(default)]
    pub email: Option<String>,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountStatus {
    pub signed_in: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<AccountProfile>,
    /// The webapp URL this build was compiled against.  Exposed for
    /// transparency so the Settings UI can show which environment the user
    /// is signing into.
    pub webapp_url: String,
}

// ── Webapp URL resolution ────────────────────────────────────────────────────

/// Resolve the webapp base URL for this build.
///
/// Priority:
/// 1. `AUTOMATIC_WEBAPP_URL` env var baked in at compile time.
/// 2. `http://localhost:3000` in debug builds.
/// 3. `https://tryautomatic.app` in release builds.
pub fn webapp_url() -> String {
    if let Some(url) = option_env!("AUTOMATIC_WEBAPP_URL") {
        let trimmed = url.trim().trim_end_matches('/');
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if cfg!(debug_assertions) {
        "http://localhost:3000".to_string()
    } else {
        "https://tryautomatic.app".to_string()
    }
}

// ── OAuth types ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
}

// ── PKCE ─────────────────────────────────────────────────────────────────────

fn generate_code_verifier() -> String {
    let bytes: [u8; 32] = rand::random();
    URL_SAFE_NO_PAD.encode(bytes)
}

fn generate_code_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

// ── Keychain helpers ─────────────────────────────────────────────────────────

fn keychain_store(user: &str, value: &str) -> Result<(), String> {
    secret_store::store_secret(user, value)
}

/// The keychain entries that hold one account's sign-in.
///
/// Sign-out and the status check take these as an argument so that a test
/// can give them entries of its own. The test store is shared by the whole
/// test process.
struct AccountEntries<'a> {
    access_token: &'a str,
    refresh_token: &'a str,
    profile: &'a str,
}

const ACCOUNT_ENTRIES: AccountEntries<'static> = AccountEntries {
    access_token: ACCESS_TOKEN_USER,
    refresh_token: REFRESH_TOKEN_USER,
    profile: PROFILE_USER,
};

// ── Public API ───────────────────────────────────────────────────────────────

/// Run the full OAuth 2.0 + PKCE flow against the Automatic webapp.
///
/// Binds a loopback listener, opens the user's browser to the authorize
/// endpoint, waits for the callback, exchanges the auth code for tokens,
/// fetches the user's profile, and stores everything in the keychain.
/// Returns the authenticated profile on success.
pub async fn login() -> Result<AccountProfile, String> {
    let webapp = webapp_url();

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("failed to bind callback listener: {}", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("failed to get listener address: {}", e))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{}/callback", port);

    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = uuid::Uuid::new_v4().to_string();

    let mut auth_url = Url::parse(&format!("{}/oauth/authorize", webapp))
        .map_err(|e| format!("invalid webapp URL: {}", e))?;
    auth_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state);

    let auth_url_str = auth_url.to_string();
    if open::that(&auth_url_str).is_err() {
        return Err(format!(
            "Failed to open browser. Please visit: {}",
            auth_url_str
        ));
    }

    let (code, returned_state) = crate::oauth::wait_for_callback(listener).await?;
    if returned_state != state {
        return Err("OAuth state mismatch — possible CSRF attack".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;

    let token_params = [
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("client_id", CLIENT_ID),
        ("code_verifier", code_verifier.as_str()),
    ];

    let token_resp = client
        .post(format!("{}/oauth/token", webapp))
        .form(&token_params)
        .send()
        .await
        .map_err(|e| format!("token exchange request failed: {}", e))?;

    if !token_resp.status().is_success() {
        let status = token_resp.status();
        let text = token_resp.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed (HTTP {}): {}", status, text));
    }

    let tokens: TokenResponse = token_resp
        .json()
        .await
        .map_err(|e| format!("failed to parse token response: {}", e))?;

    let profile_resp = client
        .get(format!("{}/api/me", webapp))
        .bearer_auth(&tokens.access_token)
        .send()
        .await
        .map_err(|e| format!("profile fetch failed: {}", e))?;

    if !profile_resp.status().is_success() {
        let status = profile_resp.status();
        let text = profile_resp.text().await.unwrap_or_default();
        return Err(format!("Profile fetch failed (HTTP {}): {}", status, text));
    }

    let profile: AccountProfile = profile_resp
        .json()
        .await
        .map_err(|e| format!("failed to parse profile response: {}", e))?;

    keychain_store(ACCESS_TOKEN_USER, &tokens.access_token)?;
    if let Some(refresh) = &tokens.refresh_token {
        keychain_store(REFRESH_TOKEN_USER, refresh)?;
    }
    let profile_json = serde_json::to_string(&profile)
        .map_err(|e| format!("failed to serialise profile: {}", e))?;
    keychain_store(PROFILE_USER, &profile_json)?;

    Ok(profile)
}

/// Sign out: revoke the refresh token on the webapp, then clear all local
/// account credentials from the keychain.
///
/// An error means the user may still be signed in, and says which secrets
/// are still stored. Success means the keychain confirmed that none are.
pub async fn logout() -> Result<(), String> {
    sign_out_of_keychain(&ACCOUNT_ENTRIES).await
}

/// Revoke the refresh token in `entries` on the webapp and delete the
/// account's secrets.
async fn sign_out_of_keychain(entries: &AccountEntries<'_>) -> Result<(), String> {
    if let Some(refresh) = refresh_token_to_revoke(entries)? {
        revoke_on_webapp(&refresh).await;
    }
    delete_account_secrets(entries)
}

/// The refresh token to revoke before signing out, or `None` when none is
/// stored.
///
/// A keychain that cannot be read is an error, and sign-out stops there with
/// nothing deleted. The token may be stored, and going on would leave it
/// valid on the webapp while the user is told they are signed out.
fn refresh_token_to_revoke(entries: &AccountEntries<'_>) -> Result<Option<String>, String> {
    secret_store::load_secret_if_present(entries.refresh_token).map_err(|e| {
        format!(
            "Could not sign out. The keychain could not be read, so nothing in it was revoked \
             or removed. Any sign-in stored there is still in place: {}",
            e
        )
    })
}

/// Ask the webapp to revoke `refresh`. Best-effort: a webapp that cannot be
/// reached must not stop the user from signing out of this machine.
///
/// Redirects are not followed, as in the sign-in requests. A redirect would
/// post the refresh token again to wherever it pointed.
async fn revoke_on_webapp(refresh: &str) {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return;
    };
    let params = [("token", refresh), ("token_type_hint", "refresh_token")];
    let _ = client
        .post(format!("{}/oauth/revoke", webapp_url()))
        .form(&params)
        .send()
        .await;
}

/// Delete every account secret in `entries`, and fail unless the keychain
/// confirms that each one is gone.
///
/// An entry that was never stored counts as gone. Every entry is tried, so a
/// failure on one does not leave the others behind. The error names each
/// secret that may still be stored.
fn delete_account_secrets(entries: &AccountEntries<'_>) -> Result<(), String> {
    let secrets = [
        (entries.access_token, "the access token"),
        (entries.refresh_token, "the refresh token"),
        (entries.profile, "the profile"),
    ];
    let still_stored: Vec<String> = secrets
        .into_iter()
        .filter_map(|(entry, what)| {
            let problem = crate::proxy::remove_secret(entry).problem()?;
            Some(format!("{}: {}", what, problem))
        })
        .collect();

    if still_stored.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Could not finish signing out. These could not be removed from the keychain, \
             so you may still be signed in. {}",
            still_stored.join("; ")
        ))
    }
}

/// Return the current sign-in status based on what is persisted in the
/// keychain.  Does not make any network calls — a signed-in result means
/// a token is stored, not that it is still valid.
///
/// An error means the keychain could not be read. That is not "signed out":
/// the tokens may be stored.
pub fn status() -> Result<AccountStatus, String> {
    status_of(&ACCOUNT_ENTRIES)
}

fn status_of(entries: &AccountEntries<'_>) -> Result<AccountStatus, String> {
    let stored = |entry: &str| {
        secret_store::load_secret_if_present(entry)
            .map_err(|e| format!("Could not read the sign-in status from the keychain: {}", e))
    };
    let webapp = webapp_url();
    let signed_out = || AccountStatus {
        signed_in: false,
        profile: None,
        webapp_url: webapp.clone(),
    };

    if stored(entries.access_token)?.is_none() {
        return Ok(signed_out());
    }
    let Some(raw) = stored(entries.profile)? else {
        return Ok(signed_out());
    };
    Ok(match serde_json::from_str::<AccountProfile>(&raw) {
        Ok(profile) => AccountStatus {
            signed_in: true,
            profile: Some(profile),
            webapp_url: webapp,
        },
        Err(_) => signed_out(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::secret_store::{set_test_fault, TestFault};

    // ── Sign-out and status against a keychain that fails ────────────────────
    //
    // The test store is shared by the whole test process, so each test has
    // entries of its own. `sign_out` is only called where no refresh token is
    // stored or it cannot be read, so no test sends a request to the webapp.

    /// The entry names of a signed-in account whose entries start with `prefix`.
    struct SignedIn {
        access_token: String,
        refresh_token: String,
        profile: String,
    }

    impl SignedIn {
        fn new(prefix: &str) -> Self {
            let signed_in = SignedIn {
                access_token: format!("{}-access-token", prefix),
                refresh_token: format!("{}-refresh-token", prefix),
                profile: format!("{}-profile", prefix),
            };
            secret_store::store_secret(&signed_in.access_token, "access").expect("store");
            secret_store::store_secret(&signed_in.refresh_token, "refresh").expect("store");
            secret_store::store_secret(
                &signed_in.profile,
                r#"{"user_id":"user_1","display_name":"Test User"}"#,
            )
            .expect("store");
            signed_in
        }

        fn entries(&self) -> AccountEntries<'_> {
            AccountEntries {
                access_token: &self.access_token,
                refresh_token: &self.refresh_token,
                profile: &self.profile,
            }
        }

        fn stored(&self) -> [bool; 3] {
            [&self.access_token, &self.refresh_token, &self.profile].map(|entry| {
                secret_store::load_secret_if_present(entry)
                    .expect("read the test store")
                    .is_some()
            })
        }
    }

    fn sign_out(entries: &AccountEntries<'_>) -> Result<(), String> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(sign_out_of_keychain(entries))
    }

    #[test]
    fn deleting_the_account_secrets_removes_all_three() {
        let account = SignedIn::new("account-test-delete");

        delete_account_secrets(&account.entries()).expect("delete");

        assert_eq!(account.stored(), [false, false, false]);
    }

    #[test]
    fn signing_out_when_nothing_is_stored_succeeds() {
        let entries = AccountEntries {
            access_token: "account-test-nothing-access-token",
            refresh_token: "account-test-nothing-refresh-token",
            profile: "account-test-nothing-profile",
        };

        assert_eq!(sign_out(&entries), Ok(()));
    }

    #[test]
    fn signing_out_without_a_refresh_token_removes_the_rest() {
        let account = SignedIn::new("account-test-no-refresh");
        secret_store::delete_secret(&account.refresh_token).expect("delete");

        assert_eq!(sign_out(&account.entries()), Ok(()));

        assert_eq!(account.stored(), [false, false, false]);
    }

    #[test]
    fn a_refresh_token_that_cannot_be_read_stops_sign_out_before_anything_is_deleted() {
        let account = SignedIn::new("account-test-unreadable-refresh");

        set_test_fault(&account.refresh_token, Some(TestFault::Unreadable));
        let result = sign_out(&account.entries());
        set_test_fault(&account.refresh_token, None);

        let err = result.expect_err("the refresh token cannot be read");
        assert!(err.contains("still in place"), "unexpected error: {err}");
        assert_eq!(account.stored(), [true, true, true], "nothing was deleted");
    }

    #[test]
    fn a_secret_the_keychain_will_not_delete_fails_sign_out_and_is_named() {
        let account = SignedIn::new("account-test-refused-delete");

        set_test_fault(&account.access_token, Some(TestFault::DeleteRefused));
        let result = delete_account_secrets(&account.entries());
        set_test_fault(&account.access_token, None);

        let err = result.expect_err("the access token is still stored");
        assert!(err.contains("the access token"), "unexpected error: {err}");
        assert!(!err.contains("the refresh token"), "unexpected error: {err}");
        assert!(!err.contains("the profile"), "unexpected error: {err}");
        // The secrets that could be deleted were, so less is left behind.
        assert_eq!(account.stored(), [true, false, false]);
    }

    #[test]
    fn a_delete_that_cannot_be_checked_fails_sign_out() {
        let account = SignedIn::new("account-test-unreadable-profile");

        set_test_fault(&account.profile, Some(TestFault::Unreadable));
        let result = delete_account_secrets(&account.entries());
        set_test_fault(&account.profile, None);

        let err = result.expect_err("the profile may still be stored");
        assert!(err.contains("the profile"), "unexpected error: {err}");
        assert_eq!(account.stored(), [false, false, true]);
    }

    #[test]
    fn status_follows_what_is_stored() {
        let account = SignedIn::new("account-test-status");

        let status = status_of(&account.entries()).expect("status");
        assert!(status.signed_in);
        assert_eq!(status.profile.expect("a profile").display_name, "Test User");

        delete_account_secrets(&account.entries()).expect("delete");
        let status = status_of(&account.entries()).expect("status");
        assert!(!status.signed_in);
        assert!(status.profile.is_none());
    }

    #[test]
    fn a_keychain_that_cannot_be_read_is_not_reported_as_signed_out() {
        let account = SignedIn::new("account-test-status-unreadable");

        for entry in [&account.access_token, &account.profile] {
            set_test_fault(entry, Some(TestFault::Unreadable));
            let status = status_of(&account.entries());
            set_test_fault(entry, None);

            assert!(status.is_err(), "a failed read of {entry} is not signed out");
        }
        assert!(status_of(&account.entries()).expect("status").signed_in);
    }

    #[test]
    fn webapp_url_has_no_trailing_slash() {
        assert!(!webapp_url().ends_with('/'));
    }

    #[test]
    fn code_challenge_matches_pkce_spec() {
        // RFC 7636 test vector.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let expected = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert_eq!(generate_code_challenge(verifier), expected);
    }
}
