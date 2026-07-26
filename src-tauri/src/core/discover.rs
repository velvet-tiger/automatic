use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use super::discover_data::{
    featured_community_path, read_collections_json, read_featured_community_json,
    read_mcp_servers_json,
};

// ── MCP Server Discover ───────────────────────────────────────────────────────

/// A featured MCP server entry from the Discover catalogue.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FeaturedMcpServer {
    pub slug: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub classification: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    /// Remote transport config (SSE/HTTP), if supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<serde_json::Value>,
    /// Local stdio/command config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<serde_json::Value>,
    /// Authentication requirements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<serde_json::Value>,
}

fn load_featured_mcp_servers() -> Result<Vec<FeaturedMcpServer>, String> {
    let json = read_mcp_servers_json()?;
    serde_json::from_str(&json).map_err(|e| format!("Failed to parse featured MCP servers: {}", e))
}

/// List all featured MCP servers from the Discover catalogue.
/// When `query` is blank, returns all entries.
/// Otherwise, case-insensitive substring match across title, description,
/// provider, classification, and slug.
pub fn search_discover_mcp(query: &str) -> Result<String, String> {
    let servers = load_featured_mcp_servers()?;
    let q = query.trim().to_lowercase();

    let filtered: Vec<&FeaturedMcpServer> = if q.is_empty() {
        servers.iter().collect()
    } else {
        servers
            .iter()
            .filter(|s| {
                s.title.to_lowercase().contains(&q)
                    || s.description.to_lowercase().contains(&q)
                    || s.provider.to_lowercase().contains(&q)
                    || s.classification.to_lowercase().contains(&q)
                    || s.slug.to_lowercase().contains(&q)
            })
            .collect()
    };

    serde_json::to_string(&filtered).map_err(|e| e.to_string())
}

// ── Collections Discover ──────────────────────────────────────────────────────

/// A skill entry inside a collection.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CollectionSkill {
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: String,
}

/// An MCP server entry inside a collection.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CollectionMcpServer {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// A template entry inside a collection.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CollectionTemplate {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// Author metadata for a collection.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CollectionAuthor {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repository_url: String,
}

/// A collection from the Discover catalogue.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub author: CollectionAuthor,
    #[serde(default)]
    pub skills: Vec<CollectionSkill>,
    #[serde(default)]
    pub mcp_servers: Vec<CollectionMcpServer>,
    #[serde(default)]
    pub templates: Vec<CollectionTemplate>,
}

fn load_collections() -> Result<Vec<Collection>, String> {
    let json = read_collections_json()?;
    serde_json::from_str(&json).map_err(|e| format!("Failed to parse collections: {}", e))
}

/// List all collections from the Discover catalogue.
/// When `query` is blank, returns all entries.
/// Otherwise, case-insensitive substring match across name, description, slug,
/// tags, and the display names of contained skills.
pub fn search_collections(query: &str) -> Result<String, String> {
    let collections = load_collections()?;
    let q = query.trim().to_lowercase();

    let filtered: Vec<&Collection> = if q.is_empty() {
        collections.iter().collect()
    } else {
        collections
            .iter()
            .filter(|c| {
                c.name.to_lowercase().contains(&q)
                    || c.description.to_lowercase().contains(&q)
                    || c.slug.to_lowercase().contains(&q)
                    || c.tags.iter().any(|t| t.to_lowercase().contains(&q))
                    || c.skills.iter().any(|s| {
                        s.display_name.to_lowercase().contains(&q)
                            || s.name.to_lowercase().contains(&q)
                    })
            })
            .collect()
    };

    serde_json::to_string(&filtered).map_err(|e| e.to_string())
}

// ── Featured Community ──────────────────────────────────────────────────────

const FEATURED_COMMUNITY_URL: &str = "https://tryautomatic.app/featured-community.json";
const FEATURED_COMMUNITY_MAX_AGE: Duration = Duration::from_secs(3600);

/// Return the featured community JSON, fetching from the remote endpoint if
/// the cached file is older than one hour.  Falls back to the on-disk cache
/// (or bundled seed data) when the network is unavailable.
pub async fn get_featured_community() -> Result<String, String> {
    let path = featured_community_path()?;

    // Check whether the cached file is still fresh.
    let needs_fetch = match path.metadata().and_then(|m| m.modified()) {
        Ok(modified) => {
            SystemTime::now()
                .duration_since(modified)
                .unwrap_or(Duration::MAX)
                > FEATURED_COMMUNITY_MAX_AGE
        }
        Err(_) => true, // file missing or unreadable — fetch
    };

    if needs_fetch {
        match fetch_featured_community_remote().await {
            Ok(json) => {
                // Validate the response is a JSON array before caching.
                if serde_json::from_str::<Vec<serde_json::Value>>(&json).is_ok() {
                    if let Err(e) = std::fs::write(&path, &json) {
                        eprintln!("[automatic] Failed to cache featured-community.json: {}", e);
                    }
                    return Ok(json);
                }
                eprintln!(
                    "[automatic] Remote featured-community.json is not a valid JSON array, using cache"
                );
            }
            Err(e) => {
                eprintln!(
                    "[automatic] Failed to fetch featured-community.json: {}, using cache",
                    e
                );
            }
        }
    }

    // Return the cached/seeded file.
    read_featured_community_json()
}

/// Fetch the featured community JSON from the remote endpoint.
async fn fetch_featured_community_remote() -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .get(FEATURED_COMMUNITY_URL)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    resp.text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))
}
