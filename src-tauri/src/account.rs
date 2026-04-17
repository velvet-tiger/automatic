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

use crate::core::KEYCHAIN_SERVICE;

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
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, user).map_err(|e| e.to_string())?;
    entry.set_password(value).map_err(|e| e.to_string())
}

fn keychain_load(user: &str) -> Result<String, String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, user).map_err(|e| e.to_string())?;
    entry.get_password().map_err(|e| e.to_string())
}

fn keychain_delete(user: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, user).map_err(|e| e.to_string())?;
    entry.delete_credential().map_err(|e| e.to_string())
}

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

/// Sign out: best-effort revoke the refresh token on the webapp, then
/// clear all local account credentials from the keychain.
pub async fn logout() -> Result<(), String> {
    let webapp = webapp_url();

    if let Ok(refresh) = keychain_load(REFRESH_TOKEN_USER) {
        if let Ok(client) = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
        {
            let params = [
                ("token", refresh.as_str()),
                ("token_type_hint", "refresh_token"),
            ];
            let _ = client
                .post(format!("{}/oauth/revoke", webapp))
                .form(&params)
                .send()
                .await;
        }
    }

    // Keychain deletes are best-effort — missing entries are not an error
    // for the caller: the user simply ends up signed out.
    let _ = keychain_delete(ACCESS_TOKEN_USER);
    let _ = keychain_delete(REFRESH_TOKEN_USER);
    let _ = keychain_delete(PROFILE_USER);
    Ok(())
}

/// Return the current sign-in status based on what is persisted in the
/// keychain.  Does not make any network calls — a signed-in result means
/// a token is stored, not that it is still valid.
pub fn status() -> AccountStatus {
    let webapp = webapp_url();
    let access = keychain_load(ACCESS_TOKEN_USER).ok();
    let profile_raw = keychain_load(PROFILE_USER).ok();

    match (access, profile_raw) {
        (Some(_), Some(raw)) => match serde_json::from_str::<AccountProfile>(&raw) {
            Ok(profile) => AccountStatus {
                signed_in: true,
                profile: Some(profile),
                webapp_url: webapp,
            },
            Err(_) => AccountStatus {
                signed_in: false,
                profile: None,
                webapp_url: webapp,
            },
        },
        _ => AccountStatus {
            signed_in: false,
            profile: None,
            webapp_url: webapp,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
