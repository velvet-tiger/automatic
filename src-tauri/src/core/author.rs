use serde::{Deserialize, Serialize};

// ── Author Service ────────────────────────────────────────────────────────────
//
// Resolves a raw author descriptor (as stored in item JSON) into a rich
// AuthorProfile suitable for display in the UI.
//
// Supported descriptor types:
//   { "type": "local" }
//     → the user's own work; no network call needed.
//
//   { "type": "github", "repo": "owner/repo" }
//     → fetch the GitHub user profile for "owner" via the public API (no auth).
//       Returns avatar_url, bio, html_url.
//
//   { "type": "provider", "name": "Acme", "url": "https://acme.com",
//     "repository_url": "https://github.com/acme/mcp-server" }
//     → use the stored name + url directly; no network call.
//
// All resolution is best-effort: if the network call fails, sensible defaults
// are returned so the UI never hard-errors on author display.

/// The raw descriptor stored in an item's JSON (`_author` field).
/// Flexible — unknown fields are silently ignored.
#[derive(Debug, Deserialize, Clone)]
pub struct AuthorDescriptor {
    #[serde(rename = "type", default = "default_author_type")]
    pub kind: String,

    // --- github ---
    /// "owner/repo" (github type)
    pub repo: Option<String>,

    // --- provider ---
    pub name: Option<String>,
    pub url: Option<String>,
    pub repository_url: Option<String>,
}

fn default_author_type() -> String {
    "local".to_string()
}

/// Fully-resolved author profile, ready to render.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthorProfile {
    /// Display name (GitHub login, provider name, or "You")
    pub name: String,
    /// One-line bio or tagline. May be empty.
    pub bio: String,
    /// URL to avatar / logo image. May be empty → UI shows initials fallback.
    pub avatar_url: String,
    /// Primary link for the author (GitHub profile, homepage, …)
    pub url: String,
    /// Resolved type tag so the frontend can style accordingly.
    pub kind: String,
}

impl AuthorProfile {
    fn local() -> Self {
        AuthorProfile {
            name: "You".to_string(),
            bio: "Created locally".to_string(),
            avatar_url: String::new(),
            url: String::new(),
            kind: "local".to_string(),
        }
    }

    fn provider(name: String, url: String, bio: String) -> Self {
        AuthorProfile {
            avatar_url: String::new(),
            kind: "provider".to_string(),
            name,
            bio,
            url,
        }
    }
}

/// Resolve an `AuthorDescriptor` into a rich `AuthorProfile`.
/// Network calls are made for `github` type only.
/// All errors produce a safe fallback — never returns `Err`.
pub async fn resolve_author(descriptor: &AuthorDescriptor) -> AuthorProfile {
    match descriptor.kind.as_str() {
        "github" => resolve_github(descriptor).await,
        "provider" => {
            let name = descriptor
                .name
                .clone()
                .unwrap_or_else(|| "Unknown".to_string());
            let url = descriptor
                .url
                .clone()
                .or_else(|| descriptor.repository_url.clone())
                .unwrap_or_default();
            AuthorProfile::provider(name, url, String::new())
        }
        _ => AuthorProfile::local(),
    }
}

/// Parse the JSON string returned by the frontend and resolve it.
pub async fn resolve_author_json(raw: &str) -> Result<AuthorProfile, String> {
    let descriptor: AuthorDescriptor =
        serde_json::from_str(raw).map_err(|e| format!("Invalid author descriptor: {}", e))?;
    Ok(resolve_author(&descriptor).await)
}

// ── GitHub resolution ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GitHubUser {
    login: String,
    name: Option<String>,
    bio: Option<String>,
    avatar_url: String,
    html_url: String,
}

async fn resolve_github(descriptor: &AuthorDescriptor) -> AuthorProfile {
    // Extract owner from "owner/repo"
    let owner = match &descriptor.repo {
        Some(repo) => repo.split('/').next().unwrap_or("").to_string(),
        None => return AuthorProfile::local(),
    };

    if owner.is_empty() {
        return AuthorProfile::local();
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("automatic-desktop/1.0")
        .build()
    {
        Ok(c) => c,
        Err(_) => return github_fallback(&owner, descriptor),
    };

    let url = format!("https://api.github.com/users/{}", owner);
    let resp = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => return github_fallback(&owner, descriptor),
    };

    let user: GitHubUser = match resp.json().await {
        Ok(u) => u,
        Err(_) => return github_fallback(&owner, descriptor),
    };

    AuthorProfile {
        name: user.name.unwrap_or(user.login),
        bio: user.bio.unwrap_or_default(),
        avatar_url: user.avatar_url,
        url: user.html_url,
        kind: "github".to_string(),
    }
}

/// Fallback when GitHub API is unavailable — use what we already know.
fn github_fallback(owner: &str, descriptor: &AuthorDescriptor) -> AuthorProfile {
    let repo = descriptor.repo.as_deref().unwrap_or(owner);
    AuthorProfile {
        name: owner.to_string(),
        bio: repo.to_string(),
        avatar_url: format!("https://github.com/{}.png?size=80", owner),
        url: format!("https://github.com/{}", owner),
        kind: "github".to_string(),
    }
}
