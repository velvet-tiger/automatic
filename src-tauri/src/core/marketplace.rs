use serde::{Deserialize, Serialize};

// ── MCP Server Marketplace ────────────────────────────────────────────────────

/// A featured MCP server entry from the bundled marketplace catalogue.
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

const FEATURED_MCP_SERVERS_JSON: &str = include_str!("../../featured-mcp-servers.json");

fn load_featured_mcp_servers() -> Result<Vec<FeaturedMcpServer>, String> {
    serde_json::from_str(FEATURED_MCP_SERVERS_JSON)
        .map_err(|e| format!("Failed to parse featured MCP servers: {}", e))
}

/// List all featured MCP servers from the bundled catalogue.
/// When `query` is blank, returns all entries.
/// Otherwise, case-insensitive substring match across title, description, provider,
/// classification, and slug.
pub fn search_mcp_marketplace(query: &str) -> Result<String, String> {
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

// ── Collections Marketplace ───────────────────────────────────────────────────

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

/// A collection from the bundled marketplace catalogue.
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

const COLLECTIONS_JSON: &str = include_str!("../../collections.json");

fn load_collections() -> Result<Vec<Collection>, String> {
    serde_json::from_str(COLLECTIONS_JSON)
        .map_err(|e| format!("Failed to parse collections: {}", e))
}

/// List all collections from the bundled catalogue.
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
