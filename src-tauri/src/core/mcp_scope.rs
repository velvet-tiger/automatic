//! Project MCP servers: library servers that only one project can see.
//!
//! A project server is an ordinary library server whose stored config
//! carries `"_project": "<project id>"`. Storage, env encryption, OAuth and
//! the sync pipeline are the same as for any other library server. The tag
//! only decides which projects are offered the server and which projects
//! sync it. Every other project, profile, template and global agent config
//! treats it as absent.
//!
//! The value is the project's committed `id`, not its name: names are
//! labels, and the id is the same in every checkout and on every machine.
//!
//! The scope changes only through [`set_mcp_server_scope`] and
//! [`create_project_mcp_server`]. Every other user save keeps the stored
//! scope (see [`with_stored_scope`]), so an editor that does not know the
//! field cannot drop it by accident.

use std::collections::BTreeMap;
use std::fs;

use serde_json::Value;

use super::mcp_servers::{
    create_mcp_server_config, get_mcp_servers_dir, list_mcp_server_configs,
    read_mcp_server_config_if_present, save_mcp_server_config, AUTOMATIC_SERVER_NAME,
};
use super::paths::is_valid_name;

/// The internal config field that holds the owning project's id. It starts
/// with `_`, so `strip_internal_fields` keeps it out of agent config files.
pub const PROJECT_SCOPE_FIELD: &str = "_project";

/// The id of the project a server config is limited to, or `None` for a
/// server every project can use. An empty string counts as no scope.
pub fn config_scope(config: &Value) -> Option<&str> {
    config
        .get(PROJECT_SCOPE_FIELD)
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
}

/// Whether the project with `project_id` may use a server with `config`.
///
/// A global server is visible to everyone. A project server is visible only
/// to the project whose id it names. `None`, or a project that has no id
/// yet, sees only global servers.
pub fn config_visible_to(config: &Value, project_id: Option<&str>) -> bool {
    match config_scope(config) {
        None => true,
        Some(owner) => project_id.is_some_and(|id| !id.is_empty() && id == owner),
    }
}

/// The scope of the stored server `name`, or `None` when it is global or not
/// stored.
///
/// This reads the raw file and decrypts nothing, so listing scopes never
/// asks the keychain for the env-encryption key.
pub fn read_mcp_server_scope(name: &str) -> Result<Option<String>, String> {
    match read_scope_file(name)? {
        ScopeFile::Scoped(project_id) => Ok(Some(project_id)),
        ScopeFile::Global => Ok(None),
        ScopeFile::Unparseable(reason) => Err(format!("MCP server '{}' holds {}", name, reason)),
    }
}

/// Every project server in the library, keyed by server name, with the id of
/// the project it belongs to. Global servers are not listed.
///
/// A file that is not valid JSON has no scope this can read, and is left out
/// with a log line. Every reader of the config itself, including sync and
/// the library page, reports that file. Failing here instead would stop
/// unrelated work, such as saving a profile, over one broken server. A file
/// that cannot be read at all is still an error.
pub fn list_mcp_server_scopes() -> Result<BTreeMap<String, String>, String> {
    let mut scopes = BTreeMap::new();
    for name in list_mcp_server_configs()? {
        match read_scope_file(&name)? {
            ScopeFile::Scoped(project_id) => {
                scopes.insert(name, project_id);
            }
            ScopeFile::Global => {}
            ScopeFile::Unparseable(reason) => {
                eprintln!("[automatic] MCP server '{name}' was left out of the scope list: {reason}");
            }
        }
    }
    Ok(scopes)
}

enum ScopeFile {
    Scoped(String),
    Global,
    Unparseable(String),
}

fn read_scope_file(name: &str) -> Result<ScopeFile, String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }
    let path = get_mcp_servers_dir()?.join(format!("{}.json", name));
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ScopeFile::Global),
        Err(e) => return Err(format!("MCP server '{}' could not be read: {}", name, e)),
    };
    match serde_json::from_str::<Value>(&raw) {
        Ok(config) => Ok(match config_scope(&config) {
            Some(id) => ScopeFile::Scoped(id.to_string()),
            None => ScopeFile::Global,
        }),
        Err(e) => Ok(ScopeFile::Unparseable(format!("invalid JSON: {}", e))),
    }
}

/// The library servers to offer the project with `project_id`: every global
/// server plus that project's own. `None` offers global servers only, for
/// lists every project shares, such as profiles and global agent configs.
pub fn list_visible_mcp_server_configs(project_id: Option<&str>) -> Result<Vec<String>, String> {
    let scopes = list_mcp_server_scopes()?;
    Ok(list_mcp_server_configs()?
        .into_iter()
        .filter(|name| match scopes.get(name) {
            None => true,
            Some(owner) => project_id.is_some_and(|id| !id.is_empty() && id == owner),
        })
        .collect())
}

/// The id of the registered project whose folder holds `folder`, if any.
/// Used where only a working folder is known, to decide which project
/// servers to offer.
pub fn project_id_for_folder(folder: &std::path::Path) -> Option<String> {
    let summaries = super::get_project_summaries().ok()?;
    super::reading_checkout_of(Some(folder), &summaries)
        .map(|summary| summary.id)
        .filter(|id| !id.is_empty())
}

/// Fail when any of `names` is a project server. `what` names the list for
/// the message, for example "a profile".
///
/// Profiles, templates and global agent configs are shared by every project,
/// so a server limited to one project cannot go in them.
pub fn ensure_no_project_servers(names: &[String], what: &str) -> Result<(), String> {
    let scopes = list_mcp_server_scopes()?;
    // Case-insensitive, as the library is: `GitHub` names the file `github`.
    let scoped: Vec<&str> = names
        .iter()
        .filter(|name| scopes.keys().any(|scoped| scoped.eq_ignore_ascii_case(name)))
        .map(String::as_str)
        .collect();
    if scoped.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} cannot use project MCP servers, because every project shares it: {}. \
         Make the server available to all projects first.",
        capitalise(what),
        scoped.join(", ")
    ))
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// `data` with its `_project` field replaced by the stored server's scope.
///
/// User saves and source reinstalls go through here. The GUI editor does not
/// know the field and would drop it, and a source must not be able to move a
/// server into or out of a project. A server that is not stored yet keeps no
/// scope. Data that is not a JSON object is returned unchanged, so the save
/// that follows reports the error.
pub(crate) fn with_stored_scope(name: &str, data: &str) -> Result<String, String> {
    let Ok(mut config) = serde_json::from_str::<Value>(data) else {
        return Ok(data.to_string());
    };
    let Some(object) = config.as_object_mut() else {
        return Ok(data.to_string());
    };
    let stored = if is_valid_name(name) {
        read_mcp_server_scope(name)?
    } else {
        None
    };
    match stored {
        Some(project_id) => {
            object.insert(PROJECT_SCOPE_FIELD.to_string(), Value::String(project_id));
        }
        None => {
            if object.remove(PROJECT_SCOPE_FIELD).is_none() {
                return Ok(data.to_string());
            }
        }
    }
    serde_json::to_string(&config).map_err(|e| e.to_string())
}

/// `data` with its scope set to `project_id`, or with no scope when that is
/// `None` or empty. Any `_project` field `data` already carries is replaced,
/// so a config read from a project's files cannot choose its own scope. Data
/// that is not a JSON object is returned unchanged, so the save that follows
/// reports the error.
pub(crate) fn with_scope(data: &str, project_id: Option<&str>) -> String {
    let Ok(mut config) = serde_json::from_str::<Value>(data) else {
        return data.to_string();
    };
    let Some(object) = config.as_object_mut() else {
        return data.to_string();
    };
    match project_id.filter(|id| !id.is_empty()) {
        Some(id) => {
            object.insert(PROJECT_SCOPE_FIELD.to_string(), Value::String(id.to_string()));
        }
        None => {
            if object.remove(PROJECT_SCOPE_FIELD).is_none() {
                return data.to_string();
            }
        }
    }
    config.to_string()
}

/// `data` limited to the project with `project_id`.
fn scoped_to(data: &str, project_id: &str) -> Result<String, String> {
    let mut config: Value = serde_json::from_str(data).map_err(|e| format!("Invalid JSON: {}", e))?;
    let object = config
        .as_object_mut()
        .ok_or_else(|| "An MCP server config must be a JSON object".to_string())?;
    object.insert(
        PROJECT_SCOPE_FIELD.to_string(),
        Value::String(project_id.to_string()),
    );
    serde_json::to_string(&config).map_err(|e| e.to_string())
}

/// Fail unless `project_id` names a project registered on this machine, so a
/// stale editor or a typo cannot create a server no project can see.
fn require_project_id(project_id: &str) -> Result<(), String> {
    if project_id.trim().is_empty() {
        return Err(
            "This project has no id yet, so it cannot own an MCP server. Restart Automatic so it can assign one, then try again."
                .to_string(),
        );
    }
    let known = super::get_project_summaries()?
        .iter()
        .any(|summary| summary.id == project_id);
    if !known {
        return Err(format!(
            "No project with id '{}' is registered on this machine.",
            project_id
        ));
    }
    Ok(())
}

/// Create a new library server limited to the project with `project_id`.
///
/// The user wrote the config, so it is checked like any user save: an env
/// value that looks encrypted but is not is refused. The write is
/// create-only, so an existing server, or a case variant of one, is never
/// replaced. The caller attaches the server to the project.
pub fn create_project_mcp_server(project_id: &str, name: &str, data: &str) -> Result<(), String> {
    require_project_id(project_id)?;
    if name.eq_ignore_ascii_case(AUTOMATIC_SERVER_NAME) {
        return Err(format!("'{}' is reserved for Automatic's own server", AUTOMATIC_SERVER_NAME));
    }
    if super::contains_ignore_ascii_case(&list_mcp_server_configs()?, name) {
        return Err(format!(
            "An MCP server named '{}' already exists in the library. Choose another name.",
            name
        ));
    }
    let data = scoped_to(data, project_id)?;
    super::mcp_servers::reject_undecryptable_env_values(&data)?;
    create_mcp_server_config(name, &data)
}

/// Where a server is used outside the project it would be limited to.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScopeConflicts {
    pub projects: Vec<String>,
    pub profiles: Vec<String>,
    pub global_agents: Vec<String>,
}

impl ScopeConflicts {
    fn is_empty(&self) -> bool {
        self.projects.is_empty() && self.profiles.is_empty() && self.global_agents.is_empty()
    }

    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if !self.projects.is_empty() {
            parts.push(format!("projects {}", self.projects.join(", ")));
        }
        if !self.profiles.is_empty() {
            parts.push(format!("profiles {}", self.profiles.join(", ")));
        }
        if !self.global_agents.is_empty() {
            parts.push(format!("the global config of {}", self.global_agents.join(", ")));
        }
        parts.join("; ")
    }
}

/// Every place outside the project with `project_id` that lists `name`.
///
/// A project that cannot be read is reported as a conflict, by name: its
/// lists are unknown, and limiting the server could silently take it away.
fn scope_conflicts(name: &str, project_id: &str) -> Result<ScopeConflicts, String> {
    let mut conflicts = ScopeConflicts::default();

    for summary in super::get_project_summaries()? {
        if !summary.id.is_empty() && summary.id == project_id {
            continue;
        }
        let ident = if summary.local_key.is_empty() {
            summary.name.clone()
        } else {
            summary.local_key.clone()
        };
        let listed = super::read_project(&ident)
            .ok()
            .and_then(|raw| serde_json::from_str::<super::Project>(&raw).ok())
            .map(|project| project.mcp_servers.iter().any(|s| s.eq_ignore_ascii_case(name)));
        match listed {
            Some(false) => {}
            Some(true) => conflicts.projects.push(summary.name),
            None => conflicts.projects.push(format!("{} (could not be read)", summary.name)),
        }
    }

    for profile in super::list_project_profiles()? {
        let parsed = super::read_project_profile_parsed(&profile)
            .map_err(|e| format!("Profile '{}' could not be read: {}", profile, e))?;
        if parsed.mcp_servers.iter().any(|s| s.eq_ignore_ascii_case(name)) {
            conflicts.profiles.push(profile);
        }
    }

    let state = super::global_mcp::load_global_mcp_state()?;
    let mut agents: Vec<String> = state
        .agents
        .iter()
        .filter(|(_, record)| record.selected.iter().any(|s| s.eq_ignore_ascii_case(name)))
        .map(|(agent, _)| agent.clone())
        .collect();
    agents.sort();
    conflicts.global_agents = agents;

    Ok(conflicts)
}

/// Limit the server `name` to the project with `project_id`, or make it
/// available to every project when `project_id` is `None`.
///
/// Limiting is refused while another project, a profile or a global agent
/// config lists the server, because they would lose it on their next sync
/// without a word. The user removes it there first. Automatic's own server
/// cannot be limited.
pub fn set_mcp_server_scope(name: &str, project_id: Option<&str>) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }
    if super::is_builtin_mcp_server(name) {
        return Err(format!("'{}' is built in and is always available to every project", name));
    }
    if let Some(id) = project_id {
        require_project_id(id)?;
        let conflicts = scope_conflicts(name, id)?;
        if !conflicts.is_empty() {
            return Err(format!(
                "MCP server '{}' is still used by {}. Remove it there first, then limit it to this project.",
                name,
                conflicts.describe()
            ));
        }
    }

    let raw = read_mcp_server_config_if_present(name)?
        .ok_or_else(|| format!("MCP server '{}' not found", name))?;
    let mut config: Value =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid JSON in config: {}", e))?;
    let object = config
        .as_object_mut()
        .ok_or_else(|| format!("MCP server '{}' is not a JSON object", name))?;
    let current = object
        .get(PROJECT_SCOPE_FIELD)
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string);
    if current.as_deref() == project_id {
        return Ok(());
    }
    match project_id {
        Some(id) => {
            object.insert(PROJECT_SCOPE_FIELD.to_string(), Value::String(id.to_string()));
        }
        None => {
            object.remove(PROJECT_SCOPE_FIELD);
        }
    }
    // The read decrypted the env values, and this writer encrypts them again.
    // The config came from this install's own store, so it is trusted.
    let data = serde_json::to_string(&config).map_err(|e| e.to_string())?;
    save_mcp_server_config(name, &data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::with_test_home;
    use serde_json::json;

    fn with_temp_home(test: impl FnOnce(&std::path::Path)) {
        let home = tempfile::tempdir().expect("tempdir");
        let path = home.path().to_path_buf();
        with_test_home(path.clone(), || test(&path));
    }

    /// Register a project with `id` and `servers`, in its own folder.
    fn register(home: &std::path::Path, name: &str, id: &str, servers: &[&str]) {
        let dir = home.join(name);
        fs::create_dir_all(&dir).expect("mkdir");
        let project = crate::core::Project {
            name: name.to_string(),
            id: id.to_string(),
            directory: dir.to_string_lossy().to_string(),
            mcp_servers: servers.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        crate::core::save_project(name, &serde_json::to_string(&project).unwrap())
            .expect("save project");
    }

    fn store(name: &str, config: Value) {
        save_mcp_server_config(name, &config.to_string()).expect("store server");
    }

    #[test]
    fn config_visibility_follows_the_scope() {
        let global = json!({ "command": "npx" });
        let scoped = json!({ "command": "npx", "_project": "p-1" });
        let blank = json!({ "command": "npx", "_project": "" });

        assert!(config_visible_to(&global, None));
        assert!(config_visible_to(&global, Some("p-1")));
        assert!(config_visible_to(&scoped, Some("p-1")));
        assert!(!config_visible_to(&scoped, Some("p-2")));
        assert!(!config_visible_to(&scoped, None));
        assert!(!config_visible_to(&scoped, Some("")));
        assert!(config_visible_to(&blank, None), "an empty scope is global");
    }

    #[test]
    fn listing_offers_global_servers_and_the_projects_own() {
        with_temp_home(|_| {
            store("shared", json!({ "command": "npx" }));
            store("mine", json!({ "command": "npx", "_project": "p-1" }));
            store("theirs", json!({ "command": "npx", "_project": "p-2" }));

            assert_eq!(
                list_visible_mcp_server_configs(Some("p-1")).unwrap(),
                vec!["mine".to_string(), "shared".to_string()]
            );
            assert_eq!(
                list_visible_mcp_server_configs(None).unwrap(),
                vec!["shared".to_string()]
            );
            let scopes = list_mcp_server_scopes().unwrap();
            assert_eq!(scopes.get("mine").map(String::as_str), Some("p-1"));
            assert!(!scopes.contains_key("shared"));
        });
    }

    #[test]
    fn scope_is_read_without_decrypting_env_values() {
        with_temp_home(|_| {
            let path = crate::core::mcp_servers::write_undecryptable_mcp_server_config("locked");
            let mut config: Value =
                serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            config["_project"] = json!("p-1");
            fs::write(&path, config.to_string()).unwrap();

            assert_eq!(read_mcp_server_scope("locked").unwrap().as_deref(), Some("p-1"));
        });
    }

    #[test]
    fn a_broken_server_file_does_not_stop_the_scope_list() {
        with_temp_home(|_| {
            store("mine", json!({ "command": "npx", "_project": "p-1" }));
            let dir = get_mcp_servers_dir().unwrap();
            fs::write(dir.join("broken.json"), "not json").unwrap();

            let scopes = list_mcp_server_scopes().expect("list despite a broken file");
            assert_eq!(scopes.get("mine").map(String::as_str), Some("p-1"));
            assert!(!scopes.contains_key("broken"));
            assert!(read_mcp_server_scope("broken").is_err(), "a direct read still reports it");
            assert!(ensure_no_project_servers(&["broken".to_string()], "a profile").is_ok());
        });
    }

    #[test]
    fn a_user_save_keeps_the_stored_scope_and_ignores_an_incoming_one() {
        with_temp_home(|_| {
            store("mine", json!({ "command": "npx", "_project": "p-1" }));
            store("shared", json!({ "command": "npx" }));

            let kept: Value = serde_json::from_str(
                &with_stored_scope("mine", r#"{"command":"uvx"}"#).unwrap(),
            )
            .unwrap();
            assert_eq!(kept["_project"], json!("p-1"));

            let moved: Value = serde_json::from_str(
                &with_stored_scope("mine", r#"{"command":"uvx","_project":"p-2"}"#).unwrap(),
            )
            .unwrap();
            assert_eq!(moved["_project"], json!("p-1"));

            let planted: Value = serde_json::from_str(
                &with_stored_scope("shared", r#"{"command":"uvx","_project":"p-2"}"#).unwrap(),
            )
            .unwrap();
            assert!(planted.get("_project").is_none());

            let new: Value =
                serde_json::from_str(&with_stored_scope("new", r#"{"command":"uvx"}"#).unwrap())
                    .unwrap();
            assert!(new.get("_project").is_none());
        });
    }

    #[test]
    fn create_project_server_is_scoped_and_create_only() {
        with_temp_home(|home| {
            register(home, "one", "p-1", &[]);
            create_project_mcp_server("p-1", "db", r#"{"command":"npx","env":{"KEY":"v"}}"#)
                .expect("create");
            assert_eq!(read_mcp_server_scope("db").unwrap().as_deref(), Some("p-1"));
            let raw = crate::core::read_mcp_server_config("db").unwrap();
            let config: Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(config["env"]["KEY"], json!("v"), "env round-trips through encryption");

            assert!(create_project_mcp_server("p-1", "DB", r#"{"command":"npx"}"#).is_err());
            assert!(create_project_mcp_server("p-1", "automatic", r#"{"command":"npx"}"#).is_err());
            assert!(create_project_mcp_server("", "other", r#"{"command":"npx"}"#).is_err());
            let err = create_project_mcp_server("p-unknown", "other", r#"{"command":"npx"}"#)
                .unwrap_err();
            assert!(err.contains("No project"), "{err}");
        });
    }

    #[test]
    fn shared_lists_refuse_project_servers() {
        with_temp_home(|_| {
            store("shared", json!({ "command": "npx" }));
            store("mine", json!({ "command": "npx", "_project": "p-1" }));

            assert!(ensure_no_project_servers(&["shared".to_string()], "a profile").is_ok());
            let err = ensure_no_project_servers(
                &["shared".to_string(), "mine".to_string()],
                "a profile",
            )
            .unwrap_err();
            assert!(err.contains("mine") && err.starts_with("A profile"), "{err}");
            assert!(
                ensure_no_project_servers(&["MINE".to_string()], "a profile").is_err(),
                "a case variant names the same server"
            );
        });
    }

    #[test]
    fn scope_can_be_set_and_cleared_and_keeps_env_values() {
        with_temp_home(|home| {
            register(home, "one", "p-1", &[]);
            store("srv", json!({ "command": "npx", "env": { "TOKEN": "secret" } }));

            set_mcp_server_scope("srv", Some("p-1")).expect("limit");
            assert_eq!(read_mcp_server_scope("srv").unwrap().as_deref(), Some("p-1"));
            let config: Value =
                serde_json::from_str(&crate::core::read_mcp_server_config("srv").unwrap()).unwrap();
            assert_eq!(config["env"]["TOKEN"], json!("secret"));

            set_mcp_server_scope("srv", None).expect("make global");
            assert_eq!(read_mcp_server_scope("srv").unwrap(), None);

            assert!(set_mcp_server_scope("automatic", Some("p-1")).is_err());
            assert!(set_mcp_server_scope("missing", Some("p-1")).is_err());
        });
    }

    #[test]
    fn limiting_is_refused_while_another_project_uses_the_server() {
        with_temp_home(|home| {
            store("srv", json!({ "command": "npx" }));
            register(home, "mine", "p-mine", &[]);
            // A different case names the same library file.
            register(home, "other", "p-other", &["SRV"]);

            let err = set_mcp_server_scope("srv", Some("p-mine")).unwrap_err();
            assert!(err.contains("projects other"), "{err}");
            assert_eq!(read_mcp_server_scope("srv").unwrap(), None);

            // The project that uses it may take it.
            set_mcp_server_scope("srv", Some("p-other")).expect("limit to its user");
            assert_eq!(read_mcp_server_scope("srv").unwrap().as_deref(), Some("p-other"));
        });
    }

    #[test]
    fn limiting_is_refused_while_a_profile_or_global_config_uses_the_server() {
        with_temp_home(|home| {
            register(home, "one", "p-1", &[]);
            store("srv", json!({ "command": "npx" }));
            crate::core::save_project_profile(
                "team",
                &json!({ "name": "team", "mcp_servers": ["srv"] }).to_string(),
            )
            .expect("profile");

            let err = set_mcp_server_scope("srv", Some("p-1")).unwrap_err();
            assert!(err.contains("profiles team"), "{err}");
            assert_eq!(read_mcp_server_scope("srv").unwrap(), None);

            crate::core::save_project_profile(
                "team",
                &json!({ "name": "team", "mcp_servers": [] }).to_string(),
            )
            .expect("profile");
            let mut state = crate::core::global_mcp::load_global_mcp_state().unwrap();
            state.agents.entry("claude".to_string()).or_default().selected =
                vec!["srv".to_string()];
            crate::core::global_mcp::save_global_mcp_state(&state).unwrap();

            let err = set_mcp_server_scope("srv", Some("p-1")).unwrap_err();
            assert!(err.contains("global config of claude"), "{err}");
        });
    }
}
