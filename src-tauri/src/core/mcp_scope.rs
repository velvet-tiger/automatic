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
        ScopeFile::Global | ScopeFile::Missing => Ok(None),
        ScopeFile::Unparseable(reason) => Err(format!("MCP server '{}' holds {}", name, reason)),
    }
}

/// `data` for a cloud pull of `name`. A server this install already holds
/// keeps its own scope, so only a local action changes it. A new server keeps
/// the scope it arrives with, so a project server reaches the same project on
/// the user's other machines. That can only narrow who sees it, and a new
/// entry has no local env values to expose.
///
/// The stored file is read raw, so no env value is decrypted.
pub(crate) fn with_pulled_scope(name: &str, data: &str) -> Result<String, String> {
    match read_scope_file(name)? {
        ScopeFile::Missing => Ok(data.to_string()),
        _ => with_stored_scope(name, data),
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
            ScopeFile::Global | ScopeFile::Missing => {}
            ScopeFile::Unparseable(reason) => {
                eprintln!("[automatic] MCP server '{name}' was left out of the scope list: {reason}");
            }
        }
    }
    Ok(scopes)
}

enum ScopeFile {
    Missing,
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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ScopeFile::Missing),
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

/// The project id whose servers to offer in `folder`: the id of the
/// registered checkout whose folder holds it, when that checkout accepted its
/// id's project servers. `None` offers global servers only. Used where only a
/// working folder is known.
pub fn accepted_mcp_scope_id_for_folder(folder: &std::path::Path) -> Option<String> {
    let summaries = super::get_project_summaries().ok()?;
    let summary = super::reading_checkout_of(Some(folder), &summaries)?;
    // Without a key the checkout could only be found by name, which another
    // checkout may share, so it counts as not accepted.
    if summary.local_key.is_empty() {
        return None;
    }
    super::checkout_accepted_mcp_scope_id(&summary.local_key).ok().flatten()
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

/// How a checkout may give a server its project's scope.
#[derive(Debug, PartialEq, Eq)]
enum ScopeClaim {
    /// The checkout already accepted its id's project servers.
    Accepted,
    /// The id owns no project server yet, so this checkout is the first and
    /// is accepted once the write succeeds.
    First,
}

/// The committed `id` of the checkout `checkout`, and how it may scope a
/// server to it.
///
/// A repository chooses its `id`, so a checkout that copies another
/// project's id must not add servers to that project, or take its servers
/// by limiting one. Once the id owns a server, or another checkout accepted
/// it, only a checkout that accepted it may add more. The second condition
/// keeps a copy from claiming the id after the last server was deleted,
/// which would hand it every server the real checkout adds later.
fn scope_claim(checkout: &str) -> Result<(String, ScopeClaim), String> {
    let project_id = super::checkout_project_id(checkout)?;
    require_project_id(&project_id)?;
    if super::checkout_accepted_mcp_scope_id(checkout)?.as_deref() == Some(project_id.as_str()) {
        return Ok((project_id, ScopeClaim::Accepted));
    }
    if list_mcp_server_scopes()?.values().any(|owner| *owner == project_id) {
        return Err(format!(
            "Project '{}' shares its id with another checkout that has project MCP servers. \
             Accept this project's MCP servers in the project first.",
            checkout
        ));
    }
    let others = super::other_checkouts_accepting(checkout, &project_id)?;
    if !others.is_empty() {
        return Err(format!(
            "Project '{}' shares its id with {}, which already uses this project's MCP servers. \
             Create the server there, then accept it in this project.",
            checkout,
            others.join(", ")
        ));
    }
    Ok((project_id, ScopeClaim::First))
}

/// Accept the checkout's own id after a write that made it the first owner.
fn settle_claim(checkout: &str, project_id: &str, claim: ScopeClaim) -> Result<(), String> {
    match claim {
        ScopeClaim::Accepted => Ok(()),
        ScopeClaim::First => super::confirm_project_mcp_servers(checkout, project_id).map_err(|e| {
            format!("The server was saved, but this project could not be set to use it: {}", e)
        }),
    }
}

/// Create a new library server limited to the project of the registered
/// checkout `checkout` (its `local_key` or name).
///
/// Refused when another checkout with the same id owns project servers that
/// this checkout has not accepted. The first server of an id accepts the
/// checkout that created it. See [`scope_claim`].
pub fn create_project_mcp_server(checkout: &str, name: &str, data: &str) -> Result<(), String> {
    let _writes = super::hold_library_writes();
    let (project_id, claim) = scope_claim(checkout)?;
    create_in_scope(&project_id, name, data)?;
    settle_claim(checkout, &project_id, claim)
}

/// Create `name` as a copy of the project server `source`, limited to the
/// same project. The scope comes from the stored `source`, never from the
/// caller, so a copy cannot claim an id for the first time.
pub fn duplicate_project_mcp_server(source: &str, name: &str, data: &str) -> Result<(), String> {
    let _writes = super::hold_library_writes();
    let project_id = read_mcp_server_scope(source)?
        .ok_or_else(|| format!("MCP server '{}' is not limited to a project", source))?;
    create_in_scope(&project_id, name, data)
}

/// Create a new library server limited to `project_id`.
///
/// The user wrote the config, so it is checked like any user save: an env
/// value that looks encrypted but is not is refused. The write is
/// create-only, so an existing server, or a case variant of one, is never
/// replaced. The caller attaches the server to the project.
fn create_in_scope(project_id: &str, name: &str, data: &str) -> Result<(), String> {
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

/// Limit the server `name` to the project of the registered checkout
/// `checkout` (its `local_key` or name), or make it available to every
/// project when `checkout` is `None`.
///
/// Limiting follows the same claim rule as [`create_project_mcp_server`].
pub fn set_mcp_server_scope(name: &str, checkout: Option<&str>) -> Result<(), String> {
    let _writes = super::hold_library_writes();
    let Some(checkout) = checkout else {
        return apply_mcp_server_scope(name, None);
    };
    if !is_valid_name(name) {
        return Err("Invalid server name".into());
    }
    let (project_id, claim) = scope_claim(checkout)?;
    let before = read_mcp_server_scope(name)?;
    apply_mcp_server_scope(name, Some(&project_id))?;
    if before.as_deref() == Some(project_id.as_str()) {
        // Nothing changed, so this was no first claim.
        return Ok(());
    }
    settle_claim(checkout, &project_id, claim)
}

/// Limit the server `name` to the project with `project_id`, or make it
/// available to every project when `project_id` is `None`.
///
/// Limiting is refused while another project, a profile or a global agent
/// config lists the server, because they would lose it on their next sync
/// without a word. The user removes it there first. Automatic's own server
/// cannot be limited.
fn apply_mcp_server_scope(name: &str, project_id: Option<&str>) -> Result<(), String> {
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
            create_project_mcp_server("one", "db", r#"{"command":"npx","env":{"KEY":"v"}}"#)
                .expect("create");
            assert_eq!(read_mcp_server_scope("db").unwrap().as_deref(), Some("p-1"));
            let raw = crate::core::read_mcp_server_config("db").unwrap();
            let config: Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(config["env"]["KEY"], json!("v"), "env round-trips through encryption");

            assert!(create_project_mcp_server("one", "DB", r#"{"command":"npx"}"#).is_err());
            assert!(create_project_mcp_server("one", "automatic", r#"{"command":"npx"}"#).is_err());
            assert!(create_project_mcp_server("", "other", r#"{"command":"npx"}"#).is_err());
            let err = create_project_mcp_server("unknown", "other", r#"{"command":"npx"}"#)
                .unwrap_err();
            assert!(err.contains("not found"), "{err}");
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

            set_mcp_server_scope("srv", Some("one")).expect("limit");
            assert_eq!(read_mcp_server_scope("srv").unwrap().as_deref(), Some("p-1"));
            let config: Value =
                serde_json::from_str(&crate::core::read_mcp_server_config("srv").unwrap()).unwrap();
            assert_eq!(config["env"]["TOKEN"], json!("secret"));

            set_mcp_server_scope("srv", None).expect("make global");
            assert_eq!(read_mcp_server_scope("srv").unwrap(), None);

            assert!(set_mcp_server_scope("automatic", Some("one")).is_err());
            assert!(set_mcp_server_scope("missing", Some("one")).is_err());
        });
    }

    #[test]
    fn limiting_is_refused_while_another_project_uses_the_server() {
        with_temp_home(|home| {
            store("srv", json!({ "command": "npx" }));
            register(home, "mine", "p-mine", &[]);
            // A different case names the same library file.
            register(home, "other", "p-other", &["SRV"]);

            let err = set_mcp_server_scope("srv", Some("mine")).unwrap_err();
            assert!(err.contains("projects other"), "{err}");
            assert_eq!(read_mcp_server_scope("srv").unwrap(), None);

            // The project that uses it may take it.
            set_mcp_server_scope("srv", Some("other")).expect("limit to its user");
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

            let err = set_mcp_server_scope("srv", Some("one")).unwrap_err();
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

            let err = set_mcp_server_scope("srv", Some("one")).unwrap_err();
            assert!(err.contains("global config of claude"), "{err}");
        });
    }

    // ── Checkouts that share a project id ────────────────────────────────────

    fn read(ident: &str) -> crate::core::Project {
        serde_json::from_str(&crate::core::read_project_for_app(ident).expect("read project"))
            .expect("parse project")
    }

    /// Register "real" and "copy", two checkouts of id `p-1` that both list
    /// `db`, and let "real" create `db` with a secret.
    fn real_and_copy(home: &std::path::Path) {
        register(home, "real", "p-1", &["db"]);
        register(home, "copy", "p-1", &["db"]);
        // Give both checkouts a `local_key`, as startup does.
        crate::core::ensure_project_keys().expect("mint keys");
        create_project_mcp_server("real", "db", r#"{"command":"npx","env":{"TOKEN":"real-secret"}}"#)
            .expect("create");
    }

    #[test]
    fn the_first_server_accepts_the_checkout_that_created_it_and_no_other() {
        with_temp_home(|home| {
            real_and_copy(home);

            let real = read("real");
            assert_eq!(real.local_confirmed_project_servers_id.as_deref(), Some("p-1"));
            assert!(real.pending_project_mcp_servers.is_empty());

            let copy = read("copy");
            assert_eq!(copy.local_confirmed_project_servers_id, None);
            assert_eq!(copy.pending_project_mcp_servers, vec!["db".to_string()]);
            assert_eq!(crate::core::accepted_mcp_scope_id(&copy), None);
        });
    }

    #[test]
    fn an_unaccepted_checkout_cannot_add_or_take_servers_for_its_id() {
        with_temp_home(|home| {
            real_and_copy(home);
            store("free", json!({ "command": "npx" }));

            let err = create_project_mcp_server("copy", "planted", r#"{"command":"npx"}"#)
                .unwrap_err();
            assert!(err.contains("Accept"), "{err}");
            assert!(read_mcp_server_scope("planted").unwrap().is_none());

            let err = set_mcp_server_scope("free", Some("copy")).unwrap_err();
            assert!(err.contains("Accept"), "{err}");
            assert_eq!(read_mcp_server_scope("free").unwrap(), None);

            // Once accepted, it may.
            crate::core::confirm_project_mcp_servers("copy", "p-1").expect("accept");
            create_project_mcp_server("copy", "second", r#"{"command":"npx"}"#).expect("create");
            set_mcp_server_scope("free", Some("copy")).expect("limit");
            assert_eq!(read_mcp_server_scope("free").unwrap().as_deref(), Some("p-1"));
        });
    }

    #[test]
    fn the_shared_read_path_does_not_list_pending_servers() {
        with_temp_home(|home| {
            real_and_copy(home);
            let shared: crate::core::Project =
                serde_json::from_str(&crate::core::read_project("copy").unwrap()).unwrap();
            assert!(shared.pending_project_mcp_servers.is_empty());
            assert_eq!(read("copy").pending_project_mcp_servers, vec!["db".to_string()]);
        });
    }

    #[test]
    fn a_copy_cannot_claim_an_id_after_the_last_server_is_deleted() {
        with_temp_home(|home| {
            real_and_copy(home);
            crate::core::delete_mcp_server_config("db").expect("delete");
            assert!(list_mcp_server_scopes().unwrap().is_empty());

            let err = create_project_mcp_server("copy", "planted", r#"{"command":"npx"}"#)
                .unwrap_err();
            assert!(err.contains("real"), "{err}");
            assert_eq!(read("copy").local_confirmed_project_servers_id, None);

            // The checkout that accepted the id may still add servers.
            create_project_mcp_server("real", "next", r#"{"command":"npx"}"#).expect("create");
            assert_eq!(read("copy").pending_project_mcp_servers, vec!["next".to_string()]);
        });
    }

    #[test]
    fn accepting_needs_the_current_id_and_a_new_id_needs_a_new_accept() {
        with_temp_home(|home| {
            real_and_copy(home);

            let err = crate::core::confirm_project_mcp_servers("copy", "p-other").unwrap_err();
            assert!(err.contains("changed"), "{err}");
            assert!(crate::core::confirm_project_mcp_servers("copy", "").is_err());
            assert_eq!(read("copy").local_confirmed_project_servers_id, None);

            // An accept stored for an older id grants nothing under a new one.
            let mut real = read("real");
            real.id = "p-2".to_string();
            assert_eq!(crate::core::accepted_mcp_scope_id(&real), None);
        });
    }

    #[test]
    fn ignore_hides_the_notice_but_keeps_the_servers_hidden() {
        with_temp_home(|home| {
            real_and_copy(home);
            crate::core::ignore_project_mcp_servers("copy", "p-1").expect("ignore");
            let copy = read("copy");
            assert_eq!(copy.local_ignored_project_servers_id.as_deref(), Some("p-1"));
            assert_eq!(copy.pending_project_mcp_servers, vec!["db".to_string()]);
            assert_eq!(crate::core::accepted_mcp_scope_id(&copy), None);

            // Accepting clears the Ignore.
            crate::core::confirm_project_mcp_servers("copy", "p-1").expect("accept");
            assert_eq!(read("copy").local_ignored_project_servers_id, None);
        });
    }

    #[test]
    fn a_committed_config_cannot_grant_an_accept() {
        with_temp_home(|home| {
            real_and_copy(home);
            let config_path = home.join("copy").join(".automatic.json");
            let mut committed: Value =
                serde_json::from_str(&fs::read_to_string(&config_path).expect("read config"))
                    .expect("parse config");
            committed["local_confirmed_project_servers_id"] = json!("p-1");
            fs::write(&config_path, committed.to_string()).expect("plant");

            let copy = read("copy");
            assert_eq!(copy.local_confirmed_project_servers_id, None);
            assert_eq!(crate::core::accepted_mcp_scope_id(&copy), None);

            // Saving the project does not carry the planted value into the pointer.
            crate::core::save_project("copy", &serde_json::to_string(&copy).unwrap())
                .expect("save");
            assert_eq!(read("copy").local_confirmed_project_servers_id, None);
            let saved = fs::read_to_string(&config_path).expect("read config");
            assert!(!saved.contains("local_confirmed_project_servers_id"), "{saved}");
        });
    }

    #[test]
    fn only_the_accepted_checkout_syncs_the_server_and_neither_drifts() {
        with_temp_home(|home| {
            real_and_copy(home);
            for name in ["real", "copy"] {
                let mut project = read(name);
                project.agents = vec!["claude".to_string()];
                crate::core::save_project(name, &serde_json::to_string(&project).unwrap())
                    .expect("save agents");
            }

            let mut real = read("real");
            crate::sync::sync_project_without_autodetect(&mut real).expect("sync real");
            let mut copy = read("copy");
            crate::sync::sync_project_without_autodetect(&mut copy).expect("sync copy");

            let real_file = fs::read_to_string(home.join("real").join(".mcp.json")).expect("real");
            assert!(real_file.contains("real-secret"), "the accepted checkout syncs it");
            let copy_file =
                fs::read_to_string(home.join("copy").join(".mcp.json")).unwrap_or_default();
            assert!(!copy_file.contains("real-secret"), "the copy must not get the secret");
            assert!(!copy_file.contains("\"db\""), "the copy must not list the server: {copy_file}");

            for project in [&real, &copy] {
                let drift = crate::sync::check_project_drift(project).expect("drift");
                assert!(!drift.drifted, "{}: {:?}", project.name, drift);
            }

            // The copy is not told to make the server global.
            let report = crate::sync::check_project_problems(&copy).expect("problems");
            assert!(
                report.problems.iter().all(|p| p.title != "MCP servers from another project"),
                "{:?}",
                report.problems
            );

            // After Accept, the copy syncs it too.
            crate::core::confirm_project_mcp_servers("copy", "p-1").expect("accept");
            let mut copy = read("copy");
            assert!(copy.pending_project_mcp_servers.is_empty());
            crate::sync::sync_project_without_autodetect(&mut copy).expect("sync copy");
            let copy_file = fs::read_to_string(home.join("copy").join(".mcp.json")).expect("copy");
            assert!(copy_file.contains("real-secret"));
        });
    }

    #[test]
    fn folder_lookups_offer_project_servers_only_to_the_accepted_checkout() {
        with_temp_home(|home| {
            real_and_copy(home);
            // Callers pass a canonical folder, as `detect_reading_folder` does.
            let folder = |name: &str| fs::canonicalize(home.join(name)).expect("canonical");
            assert_eq!(accepted_mcp_scope_id_for_folder(&folder("real")).as_deref(), Some("p-1"));
            assert_eq!(accepted_mcp_scope_id_for_folder(&folder("copy")), None);
            assert_eq!(
                list_visible_mcp_server_configs(accepted_mcp_scope_id_for_folder(&folder("copy")).as_deref())
                    .unwrap(),
                Vec::<String>::new()
            );
        });
    }

    #[test]
    fn accepted_ids_are_listed_by_checkout_key() {
        with_temp_home(|home| {
            real_and_copy(home);
            let accepted = crate::core::list_accepted_mcp_scope_ids().expect("list");
            let real_key = read("real").local_key;
            assert_eq!(accepted.len(), 1, "{accepted:?}");
            assert_eq!(accepted.get(&real_key).map(String::as_str), Some("p-1"));
        });
    }

    #[test]
    fn a_checkout_with_no_key_gets_no_project_servers_in_folder_lookups() {
        with_temp_home(|home| {
            register(home, "keyless", "p-9", &["db"]);
            create_project_mcp_server("keyless", "db", r#"{"command":"npx"}"#).expect("create");
            let folder = fs::canonicalize(home.join("keyless")).expect("canonical");
            assert_eq!(accepted_mcp_scope_id_for_folder(&folder), None);
        });
    }

    #[test]
    fn a_duplicate_keeps_the_source_scope_and_cannot_claim_an_id() {
        with_temp_home(|home| {
            real_and_copy(home);
            store("global", json!({ "command": "npx" }));
            duplicate_project_mcp_server("db", "db-copy", r#"{"command":"npx"}"#).expect("dup");
            assert_eq!(read_mcp_server_scope("db-copy").unwrap().as_deref(), Some("p-1"));
            assert!(duplicate_project_mcp_server("global", "g-copy", r#"{"command":"npx"}"#).is_err());
            assert!(duplicate_project_mcp_server("missing", "m-copy", r#"{"command":"npx"}"#).is_err());
        });
    }

    // ── The OAuth proxy serves a project server only to its checkout ────────

    const REMOTE_URL: &str = "https://mcp.example.com/mcp";

    /// `real` and `copy` share id `p-1`, and only `real` accepted it. `other`
    /// has id `p-2` and accepted that. `remote` is an HTTP server of `p-1`
    /// and `open` a global HTTP server. Returns the canonical folder of a
    /// checkout, as `detect_reading_folder` gives it.
    fn proxy_fixture(home: &std::path::Path) -> impl Fn(&str) -> std::path::PathBuf + '_ {
        register(home, "real", "p-1", &[]);
        register(home, "copy", "p-1", &[]);
        register(home, "other", "p-2", &[]);
        fs::create_dir_all(home.join("outside")).expect("mkdir");
        crate::core::ensure_project_keys().expect("mint keys");
        let remote = json!({ "type": "http", "url": REMOTE_URL }).to_string();
        create_project_mcp_server("real", "remote", &remote).expect("create");
        crate::core::confirm_project_mcp_servers("other", "p-2").expect("accept p-2");
        store("open", json!({ "type": "http", "url": REMOTE_URL }));
        move |name| fs::canonicalize(home.join(name)).expect("canonical")
    }

    fn proxy_check(server: &str, folder: Option<&std::path::Path>) -> Result<(), String> {
        let raw = crate::core::read_mcp_server_config(server).expect("read");
        let config: Value = serde_json::from_str(&raw).expect("json");
        crate::proxy::ensure_proxy_may_use(server, &config, folder)
    }

    #[test]
    fn the_proxy_serves_a_project_server_to_the_accepted_checkout() {
        with_temp_home(|home| {
            let folder = proxy_fixture(home);
            proxy_check("remote", Some(&folder("real"))).expect("the accepted checkout");
            let nested = folder("real").join("src");
            fs::create_dir_all(&nested).expect("mkdir");
            proxy_check("remote", Some(&nested)).expect("a folder inside the checkout");
        });
    }

    #[test]
    fn the_proxy_refuses_a_project_server_everywhere_else() {
        with_temp_home(|home| {
            let folder = proxy_fixture(home);
            for name in ["copy", "other", "outside"] {
                let err = proxy_check("remote", Some(&folder(name))).expect_err(name);
                assert!(err.contains("'remote' is a project MCP server"), "{name}: {err}");
                assert!(err.contains(&folder(name).display().to_string()), "{name}: {err}");
                assert!(!err.contains("p-1"), "the message does not name the owner: {err}");
            }
            let err = proxy_check("remote", None).expect_err("no folder");
            assert!(err.contains("an unknown folder"), "{err}");
        });
    }

    #[test]
    fn the_proxy_serves_a_global_server_in_any_folder() {
        with_temp_home(|home| {
            let folder = proxy_fixture(home);
            for name in ["real", "copy", "other", "outside"] {
                proxy_check("open", Some(&folder(name))).expect(name);
            }
            proxy_check("open", None).expect("no folder");
        });
    }

    #[test]
    fn a_refused_proxy_never_reads_the_token() {
        use crate::core::secret_store::{set_test_fault, TestFault};
        with_temp_home(|home| {
            let folder = proxy_fixture(home);
            let entry = crate::proxy::oauth_token_entry("remote", REMOTE_URL);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");

            // A read of the token fails loudly, so a refusal that names the
            // scope shows the proxy stopped before it.
            set_test_fault(&entry, Some(TestFault::Unreadable));
            let refused = runtime
                .block_on(crate::proxy::run_proxy_in("remote", Some(&folder("copy"))))
                .map_err(|e| e.to_string());
            let allowed = runtime
                .block_on(crate::proxy::run_proxy_in("remote", Some(&folder("real"))))
                .map_err(|e| e.to_string());
            set_test_fault(&entry, None);

            let refused = refused.expect_err("the copy is refused");
            assert!(refused.contains("is a project MCP server"), "{refused}");
            let allowed = allowed.expect_err("the token read fails");
            assert!(allowed.contains("Could not read the OAuth token"), "{allowed}");
        });
    }
}
