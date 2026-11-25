// Where a library MCP server is used, and in-memory edits that add it to or
// remove it from one project or profile. Pure: callers read, save and sync.
// The MCP Servers page uses these through `commands::mcp_servers`.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use super::{get_mcp_servers_dir, is_valid_name, list_mcp_server_configs, Project, ProjectProfile};

/// The server every project gets from the sync. It is never attached or
/// removed by hand.
const AUTOMATIC_MCP_SERVER: &str = "automatic";

/// How one project uses a library MCP server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpServerProjectUse {
    /// The project's display name.
    pub name: String,
    /// The checkout's `local_key`, used to address it. `None` while it has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_key: Option<String>,
    pub directory: String,
    /// `false` when the project lists the server under `disabled_mcp_servers`.
    pub enabled: bool,
    /// The attached profiles that provide the server. Empty means the
    /// project added it itself, so it can be removed from the project.
    pub profiles: Vec<String>,
    /// True when nothing may be written into this checkout.
    pub read_only: bool,
}

/// Every project and profile that uses a library MCP server.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct McpServerUsage {
    pub projects: Vec<McpServerProjectUse>,
    /// Names of the profiles that list the server.
    pub profiles: Vec<String>,
}

fn same_server(a: &str, b: &str) -> bool {
    // The library is case-insensitive: `GitHub` names the file `github`.
    a.eq_ignore_ascii_case(b)
}

fn refuse_automatic(server: &str) -> Result<(), String> {
    if same_server(server, AUTOMATIC_MCP_SERVER) {
        return Err(format!(
            "The '{}' MCP server is added to every project by the sync. It cannot be added or removed by hand.",
            AUTOMATIC_MCP_SERVER
        ));
    }
    Ok(())
}

fn trimmed_server(server: &str) -> Result<&str, String> {
    let server = server.trim();
    if server.is_empty() {
        return Err("The MCP server name is empty.".to_string());
    }
    Ok(server)
}

/// The attached profiles of `project` that provide `server`, sorted.
pub fn profiles_providing_mcp_server(project: &Project, server: &str) -> Vec<String> {
    let mut names: Vec<String> = project
        .profile_contributions
        .iter()
        .filter(|(_, contribution)| contribution.mcp_servers.iter().any(|s| same_server(s, server)))
        .map(|(profile, _)| profile.clone())
        .collect();
    names.sort();
    names
}

/// How `project` uses `server`, or `None` when it does not list it.
/// `local_key` and `read_only` come from the caller, which knows the checkout.
pub fn mcp_server_use_in_project(
    project: &Project,
    server: &str,
    local_key: Option<String>,
) -> Option<McpServerProjectUse> {
    let attached = project.mcp_servers.iter().find(|s| same_server(s, server))?;
    Some(McpServerProjectUse {
        name: project.name.clone(),
        local_key,
        directory: project.directory.clone(),
        // Exact, as the sync reads it.
        enabled: !project.disabled_mcp_servers.contains(attached),
        profiles: profiles_providing_mcp_server(project, server),
        read_only: project.is_read_only_here(),
    })
}

/// Whether `profile` lists `server`.
pub fn profile_lists_mcp_server(profile: &ProjectProfile, server: &str) -> bool {
    profile.mcp_servers.iter().any(|s| same_server(s, server))
}

/// Add `server` to `project`. Returns `false` when it was already attached.
///
/// `server_scope` is the id of the project the server is limited to, and
/// `accepted_id` is the id this checkout accepted project servers for. A
/// server limited to a project is refused unless the two match.
pub fn attach_mcp_server_to_project(
    project: &mut Project,
    server: &str,
    server_scope: Option<&str>,
    accepted_id: Option<&str>,
) -> Result<bool, String> {
    let server = trimmed_server(server)?;
    refuse_automatic(server)?;
    if let Some(owner) = server_scope {
        if accepted_id != Some(owner) {
            return Err(format!(
                "MCP server '{}' is limited to another project, so it cannot be added to '{}'.",
                server, project.name
            ));
        }
    }
    if project.mcp_servers.iter().any(|s| same_server(s, server)) {
        return Ok(false);
    }
    project.mcp_servers.push(server.to_string());
    Ok(true)
}

/// Remove `server` from `project`, with its disabled entry. Returns `false`
/// when it was not attached.
///
/// Refuses a server an attached profile provides: the profile owns that
/// entry and would put it back on the next save.
pub fn detach_mcp_server_from_project(project: &mut Project, server: &str) -> Result<bool, String> {
    let server = trimmed_server(server)?;
    refuse_automatic(server)?;
    if !project.mcp_servers.iter().any(|s| same_server(s, server)) {
        return Ok(false);
    }
    let providers = profiles_providing_mcp_server(project, server);
    if !providers.is_empty() {
        return Err(format!(
            "MCP server '{}' is provided by profile {} on project '{}'. Remove it from the profile, or detach the profile.",
            server,
            providers.iter().map(|p| format!("'{p}'")).collect::<Vec<_>>().join(", "),
            project.name
        ));
    }
    project.mcp_servers.retain(|s| !same_server(s, server));
    project.disabled_mcp_servers.retain(|s| !same_server(s, server));
    Ok(true)
}

/// Add `server` to `profile`. Returns `false` when it was already listed.
/// The caller still checks that the server is not limited to one project.
pub fn add_mcp_server_to_profile(profile: &mut ProjectProfile, server: &str) -> Result<bool, String> {
    let server = trimmed_server(server)?;
    refuse_automatic(server)?;
    if profile_lists_mcp_server(profile, server) {
        return Ok(false);
    }
    profile.mcp_servers.push(server.to_string());
    Ok(true)
}

/// Remove `server` from `profile`. Returns `false` when it was not listed.
pub fn remove_mcp_server_from_profile(profile: &mut ProjectProfile, server: &str) -> Result<bool, String> {
    let server = trimmed_server(server)?;
    let before = profile.mcp_servers.len();
    profile.mcp_servers.retain(|s| !same_server(s, server));
    Ok(profile.mcp_servers.len() != before)
}

/// What a stored server connects to: the fields that identify it against
/// the Discover catalogue. Holds no env values or headers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct McpServerConnection {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(rename = "_url_template", skip_serializing_if = "Option::is_none")]
    pub url_template: Option<String>,
}

/// The connection fields of one stored config value.
pub fn mcp_server_connection_of(config: &Value) -> McpServerConnection {
    let text = |key: &str| config.get(key).and_then(Value::as_str).map(str::to_string);
    McpServerConnection {
        transport: text("type"),
        command: text("command"),
        args: config
            .get("args")
            .and_then(Value::as_array)
            .map(|args| args.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default(),
        url: text("url"),
        url_template: text("_url_template"),
    }
}

/// The connection of every stored server, keyed by name.
///
/// The files are read raw, so no env value is decrypted and the keychain is
/// never asked for the key. A file that is not valid JSON is left out with a
/// log line; the editor reports it when it is opened. A file that cannot be
/// read at all is an error.
pub fn list_mcp_server_connections() -> Result<BTreeMap<String, McpServerConnection>, String> {
    let dir = get_mcp_servers_dir()?;
    let mut out = BTreeMap::new();
    for name in list_mcp_server_configs()? {
        if !is_valid_name(&name) {
            continue;
        }
        let path = dir.join(format!("{}.json", name));
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("MCP server '{}' could not be read: {}", name, e)),
        };
        match serde_json::from_str::<Value>(&raw) {
            Ok(config) => {
                out.insert(name, mcp_server_connection_of(&config));
            }
            Err(e) => eprintln!("[automatic] MCP server '{name}' was left out of the connection list: {e}"),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ProfileContribution;

    fn project(servers: &[&str]) -> Project {
        Project {
            name: "demo".to_string(),
            directory: "/tmp/demo".to_string(),
            mcp_servers: servers.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn with_profile(mut p: Project, profile: &str, servers: &[&str]) -> Project {
        p.profile_contributions.insert(
            profile.to_string(),
            ProfileContribution {
                mcp_servers: servers.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            },
        );
        p
    }

    fn profile(servers: &[&str]) -> ProjectProfile {
        ProjectProfile {
            name: "team".to_string(),
            mcp_servers: servers.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn connection_of_keeps_only_connection_fields() {
        let config = serde_json::json!({
            "type": "stdio", "command": "npx", "args": ["-y", "pkg", 3],
            "env": {"TOKEN": "enc:v1:secret"}, "_url_template": "https://<a>.example/mcp"
        });
        let connection = mcp_server_connection_of(&config);
        assert_eq!(connection.transport.as_deref(), Some("stdio"));
        assert_eq!(connection.args, vec!["-y", "pkg"]);
        let json = serde_json::to_string(&connection).expect("json");
        assert!(!json.contains("TOKEN") && !json.contains("secret"), "{json}");
        assert!(json.contains("\"type\":\"stdio\""), "{json}");
    }

    #[test]
    fn use_in_project_reports_direct_profile_and_disabled_entries() {
        let mut p = with_profile(project(&["github", "Sentry"]), "base", &["sentry"]);
        p.disabled_mcp_servers.push("github".to_string());

        let github = mcp_server_use_in_project(&p, "GitHub", Some("key".into())).expect("listed");
        assert!(!github.enabled);
        assert!(github.profiles.is_empty());
        assert_eq!(github.local_key.as_deref(), Some("key"));

        let sentry = mcp_server_use_in_project(&p, "sentry", None).expect("listed");
        assert!(sentry.enabled);
        assert_eq!(sentry.profiles, vec!["base".to_string()]);

        assert!(mcp_server_use_in_project(&p, "linear", None).is_none());
    }

    #[test]
    fn attach_adds_once_and_ignores_case_variants() {
        let mut p = project(&["github"]);
        assert_eq!(attach_mcp_server_to_project(&mut p, "sentry", None, None), Ok(true));
        assert_eq!(attach_mcp_server_to_project(&mut p, "Sentry", None, None), Ok(false));
        assert_eq!(attach_mcp_server_to_project(&mut p, "GITHUB", None, None), Ok(false));
        assert_eq!(p.mcp_servers, vec!["github", "sentry"]);
    }

    #[test]
    fn attach_refuses_a_server_limited_to_another_project() {
        let mut p = project(&[]);
        assert!(attach_mcp_server_to_project(&mut p, "db", Some("id-a"), None).is_err());
        assert!(attach_mcp_server_to_project(&mut p, "db", Some("id-a"), Some("id-b")).is_err());
        assert!(p.mcp_servers.is_empty());
        assert_eq!(attach_mcp_server_to_project(&mut p, "db", Some("id-a"), Some("id-a")), Ok(true));
    }

    #[test]
    fn attach_and_detach_refuse_automatic_and_empty_names() {
        let mut p = project(&["automatic"]);
        assert!(attach_mcp_server_to_project(&mut p, "Automatic", None, None).is_err());
        assert!(detach_mcp_server_from_project(&mut p, "automatic").is_err());
        assert!(attach_mcp_server_to_project(&mut p, "  ", None, None).is_err());
        assert_eq!(p.mcp_servers, vec!["automatic"]);
    }

    #[test]
    fn detach_removes_the_server_and_its_disabled_entries() {
        let mut p = project(&["github", "sentry"]);
        p.disabled_mcp_servers = vec!["GitHub".to_string(), "github".to_string()];
        assert_eq!(detach_mcp_server_from_project(&mut p, "github"), Ok(true));
        assert_eq!(p.mcp_servers, vec!["sentry"]);
        assert!(p.disabled_mcp_servers.is_empty());
        assert_eq!(detach_mcp_server_from_project(&mut p, "github"), Ok(false));
    }

    #[test]
    fn detach_refuses_a_server_a_profile_provides() {
        let mut p = with_profile(project(&["sentry"]), "base", &["Sentry"]);
        let err = detach_mcp_server_from_project(&mut p, "sentry").expect_err("refused");
        assert!(err.contains("'base'"), "{err}");
        assert_eq!(p.mcp_servers, vec!["sentry"]);
    }

    #[test]
    fn profile_add_and_remove_are_case_insensitive() {
        let mut pr = profile(&["github"]);
        assert_eq!(add_mcp_server_to_profile(&mut pr, "GitHub"), Ok(false));
        assert_eq!(add_mcp_server_to_profile(&mut pr, "sentry"), Ok(true));
        assert!(add_mcp_server_to_profile(&mut pr, "automatic").is_err());
        assert_eq!(remove_mcp_server_from_profile(&mut pr, "GITHUB"), Ok(true));
        assert_eq!(remove_mcp_server_from_profile(&mut pr, "github"), Ok(false));
        assert_eq!(pr.mcp_servers, vec!["sentry"]);
    }
}
