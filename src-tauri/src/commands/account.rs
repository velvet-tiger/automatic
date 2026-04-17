// ── Account Commands ──────────────────────────────────────────────────────────
//
// Tauri command bindings for the desktop ↔ webapp OAuth flow.
// See `crate::account` for the underlying implementation.
//
// Gated behind the `authentication` feature flag (see src-tauri/src/core/flags.rs).
// When the flag is off the commands are still registered — so the frontend's
// `invoke()` call does not throw "command not found" — but they short-circuit
// so a production build without the flag cannot execute the OAuth flow or
// report a signed-in status derived from the keychain.

use crate::account::{AccountProfile, AccountStatus};

const FLAG: &str = "authentication";

fn flag_enabled() -> bool {
    crate::core::get_feature_flags().contains_key(FLAG)
}

fn disabled_error() -> String {
    "Authentication is disabled in this build.".to_string()
}

/// Kick off the OAuth login flow, opening the user's browser and
/// returning the authenticated profile once the callback completes.
#[tauri::command]
pub async fn account_login() -> Result<AccountProfile, String> {
    if !flag_enabled() {
        return Err(disabled_error());
    }
    crate::account::login().await
}

/// Revoke the stored refresh token on the webapp (best-effort) and
/// clear all local account credentials from the keychain.
#[tauri::command]
pub async fn account_logout() -> Result<(), String> {
    if !flag_enabled() {
        return Err(disabled_error());
    }
    crate::account::logout().await
}

/// Return the current sign-in status and the webapp URL this build was
/// compiled against.  Does not make any network calls.
///
/// When the `authentication` flag is off, always reports signed-out so the
/// UI has a consistent shape if it ever invokes this directly.
#[tauri::command]
pub fn account_status() -> AccountStatus {
    if !flag_enabled() {
        return AccountStatus {
            signed_in: false,
            profile: None,
            webapp_url: String::new(),
        };
    }
    crate::account::status()
}
