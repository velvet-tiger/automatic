//! OAuth 2.1 token acquisition for remote MCP servers.
//!
//! Implements the MCP authorization spec:
//! 1. Discover OAuth metadata via Protected Resource Metadata (RFC 9728)
//! 2. Dynamic Client Registration (RFC 7591) to get a client_id
//! 3. PKCE authorization code flow with browser redirect
//! 4. Token exchange and keychain storage
//!
//! The acquired token is stored in the system keychain via [`crate::proxy`]
//! helpers.  It is never written to any file on disk.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tokio::net::TcpListener;
use url::Url;

// ── OAuth metadata types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ProtectedResourceMetadata {
    pub resource: Option<String>,
    /// Single authorization server (legacy / convenience).
    pub authorization_server: Option<String>,
    /// Multiple authorization servers (RFC 9728).
    pub authorization_servers: Option<Vec<String>>,
    pub scopes_supported: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthorizationServerMetadata {
    pub issuer: Option<String>,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: Option<String>,
    pub scopes_supported: Option<Vec<String>>,
    pub response_types_supported: Option<Vec<String>>,
    pub code_challenge_methods_supported: Option<Vec<String>>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistrationResponse {
    pub client_id: String,
    pub client_secret: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
}

/// Stored OAuth credentials — serialized to keychain as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredOAuthCredentials {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_endpoint: String,
    pub expires_in: Option<u64>,
    pub acquired_at: u64,
}

// ── PKCE helpers ─────────────────────────────────────────────────────────────

fn generate_code_verifier() -> String {
    let bytes: [u8; 32] = rand::random();
    URL_SAFE_NO_PAD.encode(bytes)
}

fn generate_code_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

// ── Discovery ────────────────────────────────────────────────────────────────

/// Discover the authorization server URL for a remote MCP server.
///
/// Follows the MCP spec: first tries `/.well-known/oauth-protected-resource`,
/// then falls back to `/.well-known/oauth-authorization-server`.
pub async fn discover_auth_server(mcp_url: &str) -> Result<AuthorizationServerMetadata, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let base = Url::parse(mcp_url).map_err(|e| format!("invalid MCP URL: {}", e))?;

    // Step 1: Try Protected Resource Metadata (RFC 9728).
    let prm_url = {
        let mut u = base.clone();
        let path = u.path().trim_end_matches('/');
        u.set_path(&format!(
            "/.well-known/oauth-protected-resource{}",
            if path.is_empty() || path == "/" {
                ""
            } else {
                path
            }
        ));
        u.to_string()
    };

    let auth_server_url = if let Ok(resp) = client.get(&prm_url).send().await {
        if resp.status().is_success() {
            if let Ok(prm) = resp.json::<ProtectedResourceMetadata>().await {
                // Pick the first authorization server.
                prm.authorization_servers
                    .and_then(|v| v.into_iter().next())
                    .or(prm.authorization_server)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Step 2: Fetch AS metadata.
    let as_base = if let Some(ref url) = auth_server_url {
        Url::parse(url).map_err(|e| format!("invalid auth server URL: {}", e))?
    } else {
        base.clone()
    };

    // Try well-known paths per RFC 8414.
    let path = as_base.path().trim_start_matches('/').trim_end_matches('/');
    let candidates = if path.is_empty() {
        vec![format!(
            "{}/.well-known/oauth-authorization-server",
            as_base.origin().ascii_serialization()
        )]
    } else {
        vec![
            format!(
                "{}/.well-known/oauth-authorization-server/{}",
                as_base.origin().ascii_serialization(),
                path
            ),
            format!(
                "{}/{}/.well-known/oauth-authorization-server",
                as_base.origin().ascii_serialization(),
                path
            ),
            format!(
                "{}/.well-known/oauth-authorization-server",
                as_base.origin().ascii_serialization()
            ),
        ]
    };

    for candidate_url in &candidates {
        if let Ok(resp) = client.get(candidate_url).send().await {
            if resp.status().is_success() {
                if let Ok(metadata) = resp.json::<AuthorizationServerMetadata>().await {
                    return Ok(metadata);
                }
            }
        }
    }

    Err(format!(
        "Could not discover OAuth metadata for {}. Tried: {:?}",
        mcp_url, candidates
    ))
}

// ── Dynamic Client Registration ──────────────────────────────────────────────

pub async fn register_client(
    metadata: &AuthorizationServerMetadata,
    callback_url: &str,
) -> Result<RegistrationResponse, String> {
    let registration_endpoint = metadata
        .registration_endpoint
        .as_ref()
        .ok_or("Authorization server does not support dynamic client registration")?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "client_name": "Automatic",
        "redirect_uris": [callback_url],
        "grant_types": ["authorization_code", "refresh_token"],
        "token_endpoint_auth_method": "none",
        "response_types": ["code"],
    });

    let resp = client
        .post(registration_endpoint)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("registration request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Dynamic client registration failed (HTTP {}): {}",
            status, text
        ));
    }

    resp.json::<RegistrationResponse>()
        .await
        .map_err(|e| format!("failed to parse registration response: {}", e))
}

// ── Authorization flow ───────────────────────────────────────────────────────

/// Perform the full OAuth 2.1 PKCE flow for a remote MCP server:
///
/// 1. Discover metadata
/// 2. Register client (dynamic registration)
/// 3. Open browser for authorization
/// 4. Listen for callback on localhost
/// 5. Exchange code for token
/// 6. Store token in keychain
///
/// Returns the access token on success.
pub async fn authorize_server(server_name: &str, mcp_url: &str) -> Result<String, String> {
    // 1. Discover OAuth metadata.
    let metadata = discover_auth_server(mcp_url).await?;

    // 2. Choose a callback port and build redirect URI.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("failed to bind callback listener: {}", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("failed to get listener address: {}", e))?
        .port();
    let callback_url = format!("http://127.0.0.1:{}/callback", port);

    // 3. Dynamic client registration.
    let registration = register_client(&metadata, &callback_url).await?;

    // 4. Build authorization URL with PKCE.
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = uuid::Uuid::new_v4().to_string();

    let mut auth_url = Url::parse(&metadata.authorization_endpoint).map_err(|e| e.to_string())?;
    auth_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &registration.client_id)
        .append_pair("redirect_uri", &callback_url)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state)
        .append_pair("resource", mcp_url);

    // Add scopes if the server advertises any.
    if let Some(scopes) = &metadata.scopes_supported {
        if !scopes.is_empty() {
            auth_url
                .query_pairs_mut()
                .append_pair("scope", &scopes.join(" "));
        }
    }

    // 5. Open browser.
    let auth_url_str = auth_url.to_string();
    if open::that(&auth_url_str).is_err() {
        return Err(format!(
            "Failed to open browser. Please visit: {}",
            auth_url_str
        ));
    }

    // 6. Wait for the OAuth callback.
    let (code, returned_state) = wait_for_callback(listener).await?;

    // Verify state.
    if returned_state != state {
        return Err("OAuth state mismatch — possible CSRF attack".to_string());
    }

    // 7. Exchange code for token.
    let token_response = exchange_code(
        &metadata.token_endpoint,
        &code,
        &code_verifier,
        &registration.client_id,
        &callback_url,
        mcp_url,
    )
    .await?;

    // 8. Store in keychain.
    crate::proxy::store_oauth_token(server_name, &token_response.access_token)?;

    // Also store full credentials for future refresh.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let stored = StoredOAuthCredentials {
        client_id: registration.client_id,
        client_secret: registration.client_secret.filter(|s| !s.is_empty()),
        access_token: token_response.access_token.clone(),
        refresh_token: token_response.refresh_token,
        token_endpoint: metadata.token_endpoint.clone(),
        expires_in: token_response.expires_in,
        acquired_at: now,
    };
    let creds_json = serde_json::to_string(&stored)
        .map_err(|e| format!("failed to serialize credentials: {}", e))?;
    crate::proxy::store_oauth_credentials(server_name, &creds_json)?;

    Ok(token_response.access_token)
}

// ── Callback listener ────────────────────────────────────────────────────────

/// Read a full HTTP request from a TCP stream.
/// Reads until we see the end of HTTP headers (`\r\n\r\n`).
async fn read_http_request(stream: &mut tokio::net::TcpStream) -> Result<String, String> {
    let mut buf = Vec::with_capacity(16384);
    let mut tmp = [0u8; 4096];

    loop {
        let n = tokio::io::AsyncReadExt::read(stream, &mut tmp)
            .await
            .map_err(|e| format!("callback read failed: {}", e))?;

        if n == 0 {
            break; // Connection closed.
        }
        buf.extend_from_slice(&tmp[..n]);

        // Check if we've received the full HTTP headers.
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        // Safety limit — no OAuth callback should be larger than 16KB of headers.
        if buf.len() > 16384 {
            break;
        }
    }

    Ok(String::from_utf8_lossy(&buf).to_string())
}

/// Send an HTTP response to the browser.
async fn send_http_response(stream: &mut tokio::net::TcpStream, html: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    let _ = tokio::io::AsyncWriteExt::write_all(stream, response.as_bytes()).await;
}

/// Wait for the OAuth callback on the given TCP listener.
/// Loops to handle browser preflight/favicon requests, only returning
/// when the real `/callback` with `code` and `state` params arrives.
/// Returns `(code, state)`.
async fn wait_for_callback(listener: TcpListener) -> Result<(String, String), String> {
    // Timeout after 5 minutes to avoid hanging forever.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(300);

    loop {
        // Accept next connection, with timeout.
        let (mut stream, _addr) = tokio::select! {
            result = listener.accept() => {
                result.map_err(|e| format!("callback listener accept failed: {}", e))?
            }
            _ = tokio::time::sleep_until(deadline) => {
                return Err("OAuth callback timed out after 5 minutes".to_string());
            }
        };

        // Read the full HTTP request.
        let request = read_http_request(&mut stream).await?;

        // Extract the request path from the first line.
        let first_line = request.lines().next().unwrap_or("");
        let path = first_line.split_whitespace().nth(1).unwrap_or("");

        // Ignore requests that aren't our callback path (e.g. favicon, preflight).
        if !path.starts_with("/callback") {
            send_http_response(
                &mut stream,
                "<!DOCTYPE html><html><body>Not found</body></html>",
            )
            .await;
            continue;
        }

        // Parse query params from the callback URL.
        let fake_base = format!("http://127.0.0.1{}", path);
        let url = match Url::parse(&fake_base) {
            Ok(u) => u,
            Err(_) => {
                send_http_response(
                    &mut stream,
                    "<!DOCTYPE html><html><body>Bad request</body></html>",
                )
                .await;
                continue;
            }
        };
        let params: HashMap<String, String> = url.query_pairs().into_owned().collect();

        // Check for error response from the auth server.
        if let Some(error) = params.get("error") {
            let desc = params
                .get("error_description")
                .map(|s| s.as_str())
                .unwrap_or("no description");
            let html = format!(
                "<!DOCTYPE html><html><body><h2>Authorization Failed</h2><p>{}: {}</p><p>You can close this tab.</p></body></html>",
                error, desc
            );
            send_http_response(&mut stream, &html).await;
            return Err(format!("OAuth error: {} — {}", error, desc));
        }

        // Extract code and state.
        let code = match params.get("code") {
            Some(c) => c.clone(),
            None => {
                send_http_response(
                    &mut stream,
                    "<!DOCTYPE html><html><body>Missing code parameter</body></html>",
                )
                .await;
                continue;
            }
        };
        let state = match params.get("state") {
            Some(s) => s.clone(),
            None => {
                send_http_response(
                    &mut stream,
                    "<!DOCTYPE html><html><body>Missing state parameter</body></html>",
                )
                .await;
                continue;
            }
        };

        // Send a "processing" page — we haven't confirmed the token exchange yet.
        let html = "<!DOCTYPE html><html><body>\
            <h2>Received!</h2>\
            <p>Exchanging authorization code for token... You can close this tab and return to Automatic.</p>\
            </body></html>";
        send_http_response(&mut stream, html).await;

        return Ok((code, state));
    }
}

// ── Token exchange ───────────────────────────────────────────────────────────

async fn exchange_code(
    token_endpoint: &str,
    code: &str,
    code_verifier: &str,
    client_id: &str,
    redirect_uri: &str,
    resource: &str,
) -> Result<TokenResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", code_verifier),
        ("resource", resource),
    ];

    let resp = client
        .post(token_endpoint)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("token exchange request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed (HTTP {}): {}", status, text));
    }

    resp.json::<TokenResponse>()
        .await
        .map_err(|e| format!("failed to parse token response: {}", e))
}

// ── Token refresh ────────────────────────────────────────────────────────────

/// Attempt to refresh an expired token using stored credentials.
pub async fn refresh_token(server_name: &str) -> Result<String, String> {
    let creds_json = crate::proxy::load_oauth_credentials(server_name)?;
    let creds: StoredOAuthCredentials = serde_json::from_str(&creds_json)
        .map_err(|e| format!("invalid stored credentials: {}", e))?;

    let refresh_token = creds
        .refresh_token
        .as_ref()
        .ok_or("no refresh token available — re-authorization required")?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token.as_str()),
        ("client_id", creds.client_id.as_str()),
    ];

    let resp = client
        .post(&creds.token_endpoint)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("refresh request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Token refresh failed (HTTP {}): {} — re-authorization may be required",
            status, text
        ));
    }

    let token_response: TokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse refresh response: {}", e))?;

    // Update stored credentials.
    crate::proxy::store_oauth_token(server_name, &token_response.access_token)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let updated = StoredOAuthCredentials {
        access_token: token_response.access_token.clone(),
        refresh_token: token_response.refresh_token.or(creds.refresh_token),
        expires_in: token_response.expires_in,
        acquired_at: now,
        ..creds
    };
    let updated_json =
        serde_json::to_string(&updated).map_err(|e| format!("failed to serialize: {}", e))?;
    crate::proxy::store_oauth_credentials(server_name, &updated_json)?;

    Ok(token_response.access_token)
}
