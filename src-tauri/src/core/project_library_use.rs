//! What a cloned project's config would use from this machine's library.
//!
//! Names in a committed config are resolved against the user's library by
//! sync, so a repository can reach library entries by naming them. This
//! module reports those names before the user adds the project. The config is
//! untrusted: names are escaped and capped here, and nothing is written.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::{
    get_mcp_servers_dir, get_skill_path, list_mcp_server_configs, profile_path, read_hook,
    read_project_files, read_project_profile_parsed, read_rule, read_subagent, read_user_command,
    Project, ProjectProfile, AUTOMATIC_SERVER_NAME, AUTOMATIC_SKILL_NAME,
};

/// Most names returned for one kind. The total is always reported.
const MAX_SHOWN_NAMES_PER_KIND: usize = 50;
/// Most names looked up on disk for one kind, so a config full of names cannot
/// stall a scan. Names past this are counted in `not_checked`.
const MAX_CHECKED_NAMES_PER_KIND: usize = 1000;
/// Longest name returned, in characters.
const MAX_SHOWN_NAME_CHARS: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryItemKind {
    McpServer,
    Hook,
    Skill,
    SubAgent,
    Command,
    Rule,
    Profile,
    Context,
}

/// One name from the config that this machine's library would satisfy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryUseName {
    /// Escaped and shortened for display. Never use it to look anything up.
    pub name: String,
    /// True for an item that carries more risk: a hook, an MCP server that
    /// is remote or has stored `env` or `headers` values, or a profile that
    /// provides one of those or cannot be read.
    pub flagged: bool,
}

/// The matching names of one kind, flagged names first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryUseGroup {
    pub kind: LibraryItemKind,
    /// Every match, including names left out of `names`.
    pub total: usize,
    pub flagged_total: usize,
    pub names: Vec<LibraryUseName>,
    /// Names in the config that were not looked up.
    pub not_checked: usize,
}

/// Result of [`preview_project_library_use`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind")]
pub enum ProjectLibraryUse {
    Readable {
        /// The committed `read_only` hint.
        read_only_hint: bool,
        /// A profile the config names exists here but cannot be read, so what
        /// it would add is unknown. `flagged` alone does not say this.
        has_unreadable_profile: bool,
        /// Only kinds with a match or an unchecked name.
        groups: Vec<LibraryUseGroup>,
    },
    /// The config cannot be read or parsed, so it cannot be imported. The
    /// cause is left out because its text can come from the repository.
    Unreadable,
    /// The folder was not checked. A scan stops previewing past its budget.
    NotChecked,
}

/// Report which names in `directory`'s Automatic config match this machine's
/// library, grouped by kind. Writes nothing and decrypts nothing.
///
/// An `Err` means the library could not be read. A bad config is
/// [`ProjectLibraryUse::Unreadable`].
pub fn preview_project_library_use(directory: &str) -> Result<ProjectLibraryUse, String> {
    if directory.is_empty() {
        return Err("A project directory is required.".into());
    }
    match read_project_files(directory, "") {
        Ok(Some(project)) => {
            let profiles = matched_profiles(&project);
            Ok(ProjectLibraryUse::Readable {
                read_only_hint: project.read_only,
                has_unreadable_profile: profiles.values().any(|p| p.contents.is_none()),
                groups: library_use_groups(&project, &profiles)?,
            })
        }
        Ok(None) => Ok(ProjectLibraryUse::Readable {
            read_only_hint: false,
            has_unreadable_profile: false,
            groups: Vec::new(),
        }),
        Err(_) => Ok(ProjectLibraryUse::Unreadable),
    }
}

/// Each kind matches the way sync resolves it. MCP servers are compared with
/// the registry listing. The other kinds open a path built from the name, so
/// the file system decides, and a case-insensitive disk matches `GitHub` to
/// `github`. Comparing strings here would miss those.
///
/// A matched profile adds its own entries to the project when it reconciles,
/// so those entries count as if the config listed them.
fn library_use_groups(
    project: &Project,
    profiles: &BTreeMap<String, MatchedProfile>,
) -> Result<Vec<LibraryUseGroup>, String> {
    let servers: BTreeSet<String> = list_mcp_server_configs()?.into_iter().collect();
    let profile_flags = profile_flags(project, profiles, &servers);
    let contents = || profiles.values().filter_map(|p| p.contents.as_ref());

    let server_names: Vec<String> = project
        .enabled_mcp_servers()
        .into_iter()
        .chain(contents().flat_map(|p| enabled_profile_servers(project, p)))
        .collect();
    let hooks = project.hooks.iter().chain(contents().flat_map(|p| &p.hooks));
    let skills = project
        .skills
        .iter()
        .chain(contents().flat_map(|p| &p.skills))
        .filter(|name| name.as_str() != AUTOMATIC_SKILL_NAME);
    let user_agents = project
        .user_agents
        .iter()
        .chain(contents().flat_map(|p| &p.user_agents));
    let user_commands = project
        .user_commands
        .iter()
        .chain(contents().flat_map(|p| &p.user_commands));
    let rules = project
        .file_rules
        .values()
        .flatten()
        .chain(contents().flat_map(|p| &p.rules));

    let groups = [
        // An in-memory lookup, so a padded list cannot push a server past the cap.
        group(LibraryItemKind::McpServer, server_names.iter(), usize::MAX, |name| {
            servers.contains(name).then(|| mcp_server_is_flagged(name))
        }),
        checked_group(LibraryItemKind::Hook, hooks, hook_match),
        checked_group(LibraryItemKind::Skill, skills, |name| {
            (is_single_path_component(name) && matches!(get_skill_path(name), Ok(Some(_))))
                .then_some(false)
        }),
        checked_group(LibraryItemKind::SubAgent, user_agents, |name| {
            read_subagent(name).ok().map(|_| false)
        }),
        checked_group(LibraryItemKind::Command, user_commands, |name| {
            read_user_command(name).ok().map(|_| false)
        }),
        checked_group(LibraryItemKind::Rule, rules, |name| {
            read_rule(name).ok().map(|_| false)
        }),
        checked_group(LibraryItemKind::Profile, project.profiles.iter(), |name| {
            profile_flags.get(&profiles.get(name)?.file).copied()
        }),
        checked_group(LibraryItemKind::Context, project.contexts.iter(), |slug| {
            super::read_context(slug).ok().map(|_| false)
        }),
    ];
    Ok(groups
        .into_iter()
        .filter(|g| g.total > 0 || g.not_checked > 0)
        .collect())
}

fn hook_match(name: &str) -> Option<bool> {
    read_hook(name).ok().map(|_| true)
}

/// A profile file the config's name resolves to.
struct MatchedProfile {
    /// The real path, so spellings of one name share an entry on a
    /// case-insensitive disk.
    file: PathBuf,
    /// `None` when the file cannot be read or parsed.
    contents: Option<ProjectProfile>,
}

/// The profiles the config names that exist here, keyed by the config's
/// spelling. `read_project_profile_parsed` only parses JSON: profiles hold
/// names, so nothing in it decrypts.
fn matched_profiles(project: &Project) -> BTreeMap<String, MatchedProfile> {
    let names: BTreeSet<&String> = project.profiles.iter().collect();
    names
        .into_iter()
        .take(MAX_CHECKED_NAMES_PER_KIND)
        .filter(|name| is_single_path_component(name))
        .filter_map(|name| {
            let path = profile_path(name).ok().filter(|path| path.is_file())?;
            let matched = MatchedProfile {
                file: fs::canonicalize(&path).unwrap_or(path),
                contents: read_project_profile_parsed(name).ok(),
            };
            Some((name.clone(), matched))
        })
        .collect()
}

/// Each profile file's flag, worked out once. The lookups behind a flag are
/// outside the per-kind cap, so spellings of one name must not repeat them.
fn profile_flags(
    project: &Project,
    profiles: &BTreeMap<String, MatchedProfile>,
    servers: &BTreeSet<String>,
) -> BTreeMap<PathBuf, bool> {
    let mut flags = BTreeMap::new();
    for profile in profiles.values() {
        flags
            .entry(profile.file.clone())
            .or_insert_with(|| profile_is_flagged(project, profile.contents.as_ref(), servers));
    }
    flags
}

/// A profile's servers that sync would use. The project's own
/// `disabled_mcp_servers` also switches off what a profile provides.
fn enabled_profile_servers(project: &Project, profile: &ProjectProfile) -> Vec<String> {
    profile
        .mcp_servers
        .iter()
        .filter(|name| name.as_str() != AUTOMATIC_SERVER_NAME)
        .filter(|name| !project.disabled_mcp_servers.contains(name))
        .cloned()
        .collect()
}

/// A profile hides its contents behind one name, so it is flagged for what it
/// provides. A profile that cannot be read could provide anything.
fn profile_is_flagged(
    project: &Project,
    profile: Option<&ProjectProfile>,
    servers: &BTreeSet<String>,
) -> bool {
    let Some(profile) = profile else {
        return true;
    };
    profile.hooks.iter().any(|name| hook_match(name).is_some())
        || enabled_profile_servers(project, profile)
            .iter()
            .any(|name| servers.contains(name) && mcp_server_is_flagged(name))
}

/// Whether `name` is one ordinary path component. `is_valid_name` passes
/// `C:evil`, which on Windows joins as a drive-relative path and leaves the
/// library folder.
fn is_single_path_component(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(part)), None) if part == name
    )
}

/// Whether a stored MCP server needs the user's attention: it is remote, so
/// it can hold a stored sign-in, or it holds `env` or `headers` values. Reads
/// the stored JSON directly: only presence matters, and decrypting or checking
/// for a sign-in would raise a keychain prompt. A config that cannot be read
/// counts as flagged.
fn mcp_server_is_flagged(name: &str) -> bool {
    let Ok(path) = get_mcp_servers_dir().map(|dir| dir.join(format!("{name}.json"))) else {
        return true;
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return true;
    };
    let Ok(config) = serde_json::from_str::<Value>(&raw) else {
        return true;
    };
    let remote = config.get("url").is_some()
        || matches!(config.get("type").and_then(Value::as_str), Some("http" | "sse"));
    remote
        || ["env", "headers"].iter().any(|field| {
            config
                .get(field)
                .and_then(Value::as_object)
                .is_some_and(|values| {
                    values
                        .values()
                        .any(|v| !v.is_null() && v.as_str() != Some(""))
                })
        })
}

fn checked_group<'a>(
    kind: LibraryItemKind,
    names: impl Iterator<Item = &'a String>,
    matched: impl Fn(&str) -> Option<bool>,
) -> LibraryUseGroup {
    group(kind, names, MAX_CHECKED_NAMES_PER_KIND, matched)
}

/// `matched` returns `None` for no match, or whether the match is flagged.
fn group<'a>(
    kind: LibraryItemKind,
    names: impl Iterator<Item = &'a String>,
    max_checked: usize,
    matched: impl Fn(&str) -> Option<bool>,
) -> LibraryUseGroup {
    let unique: BTreeSet<&str> = names.map(String::as_str).collect();
    let not_checked = unique.len().saturating_sub(max_checked);
    let mut matches: Vec<LibraryUseName> = unique
        .into_iter()
        .take(max_checked)
        .filter_map(|name| {
            matched(name).map(|flagged| LibraryUseName {
                name: displayable_name(name),
                flagged,
            })
        })
        .collect();
    matches.sort_by(|a, b| b.flagged.cmp(&a.flagged).then_with(|| a.name.cmp(&b.name)));

    let total = matches.len();
    let flagged_total = matches.iter().filter(|m| m.flagged).count();
    matches.truncate(MAX_SHOWN_NAMES_PER_KIND);
    LibraryUseGroup {
        kind,
        total,
        flagged_total,
        names: matches,
        not_checked,
    }
}

/// Replace characters that could hide or reorder text, and shorten the name.
pub(super) fn displayable_name(name: &str) -> String {
    let mut shown: String = name
        .chars()
        .take(MAX_SHOWN_NAME_CHARS)
        .map(|c| if hides_text(c) { '\u{FFFD}' } else { c })
        .collect();
    if name.chars().count() > MAX_SHOWN_NAME_CHARS {
        shown.push('…');
    }
    shown
}

/// Controls, every Unicode format (Cf) character, and the default-ignorable
/// characters that draw nothing.
fn hides_text(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00AD}'
                | '\u{034F}'
                | '\u{061C}'
                | '\u{0600}'..='\u{0605}'
                | '\u{06DD}'
                | '\u{070F}'
                | '\u{0890}'..='\u{0891}'
                | '\u{08E2}'
                | '\u{115F}'..='\u{1160}'
                | '\u{17B4}'..='\u{17B5}'
                | '\u{180B}'..='\u{180F}'
                | '\u{200B}'..='\u{200F}'
                | '\u{2028}'..='\u{202E}'
                | '\u{2060}'..='\u{206F}'
                | '\u{3164}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{FEFF}'
                | '\u{FFA0}'
                | '\u{FFF0}'..='\u{FFFB}'
                | '\u{110BD}'
                | '\u{110CD}'
                | '\u{13430}'..='\u{1343F}'
                | '\u{1BCA0}'..='\u{1BCA3}'
                | '\u{1D173}'..='\u{1D17A}'
                | '\u{E0000}'..='\u{E0FFF}'
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::hooks::HookHandler;
    use crate::core::paths::with_test_home;
    use crate::core::{
        get_library_skills_dir, get_profiles_dir, get_subagents_dir, save_hook,
        save_mcp_server_config, save_rule, save_user_command, write_undecryptable_mcp_server_config,
    };
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    fn write_config(dir: &Path, config: Value) {
        fs::write(dir.join(".automatic.json"), config.to_string()).expect("write config");
    }

    fn preview(dir: &Path) -> ProjectLibraryUse {
        preview_project_library_use(dir.to_str().expect("utf-8 path")).expect("preview")
    }

    fn groups(dir: &Path) -> Vec<LibraryUseGroup> {
        match preview(dir) {
            ProjectLibraryUse::Readable { groups, .. } => groups,
            other => panic!("the config should be readable: {other:?}"),
        }
    }

    fn group_of(dir: &Path, kind: LibraryItemKind) -> Option<LibraryUseGroup> {
        groups(dir).into_iter().find(|g| g.kind == kind)
    }

    fn seed_skill(name: &str) {
        let dir = get_library_skills_dir().expect("skills dir").join(name);
        fs::create_dir_all(&dir).expect("skill dir");
        fs::write(dir.join("SKILL.md"), "---\nname: x\n---\nbody").expect("skill");
    }

    fn seed_library() {
        save_mcp_server_config(
            "with-env",
            r#"{"command":"npx","env":{"API_KEY":"secret-value-123"}}"#,
        )
        .expect("server");
        save_mcp_server_config("with-headers", r#"{"type":"http","url":"https://x.test","headers":{"Authorization":"Bearer abc"}}"#)
            .expect("server");
        save_mcp_server_config("plain", r#"{"command":"npx","env":{}}"#).expect("server");
        save_hook(
            "log-session",
            "Log Session",
            "claude",
            "SessionStart",
            None,
            HookHandler::Command {
                command: "echo session".into(),
            },
            Some(30),
        )
        .expect("hook");
        seed_skill("shared-skill");
        save_rule("shared-rule", "Shared Rule", "Be kind.").expect("rule");
        save_user_command("shared-command", "Do it.").expect("command");
        let agents = get_subagents_dir().expect("agents dir");
        fs::create_dir_all(&agents).expect("agents dir");
        fs::write(agents.join("shared-agent.md"), "---\nname: shared-agent\n---\nbody")
            .expect("agent");
        let profiles = get_profiles_dir().expect("profiles dir");
        fs::create_dir_all(&profiles).expect("profiles dir");
        fs::write(profiles.join("shared-profile.json"), r#"{"name":"shared-profile"}"#)
            .expect("profile");
    }

    fn full_config() -> Value {
        serde_json::json!({
            "name": "cloned",
            "mcp_servers": ["automatic", "with-env", "with-headers", "plain", "absent"],
            "hooks": ["log-session", "absent-hook"],
            "skills": ["automatic", "shared-skill", "absent-skill"],
            "user_agents": ["shared-agent", "absent-agent"],
            "user_commands": ["shared-command", "absent-command"],
            "file_rules": { "_unified": ["shared-rule"], "CLAUDE.md": ["shared-rule", "absent-rule"] },
            "profiles": ["shared-profile", "absent-profile"],
        })
    }

    fn snapshot(root: &Path) -> BTreeMap<PathBuf, (Vec<u8>, std::time::SystemTime)> {
        let mut files = BTreeMap::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(dir) = pending.pop() {
            for entry in fs::read_dir(&dir).expect("read dir").flatten() {
                let path = entry.path();
                let meta = fs::symlink_metadata(&path).expect("metadata");
                if meta.is_dir() {
                    files.insert(path.clone(), (Vec::new(), meta.modified().expect("mtime")));
                    pending.push(path);
                } else {
                    let bytes = fs::read(&path).expect("read file");
                    files.insert(path, (bytes, meta.modified().expect("mtime")));
                }
            }
        }
        files
    }

    #[test]
    fn reports_only_names_this_library_holds_and_flags_the_risky_ones() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        with_test_home(home.path().to_path_buf(), || {
            seed_library();
            write_config(project.path(), full_config());

            let found = groups(project.path());
            let summary: Vec<(LibraryItemKind, usize, usize)> = found
                .iter()
                .map(|g| (g.kind, g.total, g.flagged_total))
                .collect();
            assert_eq!(
                summary,
                vec![
                    (LibraryItemKind::McpServer, 3, 2),
                    (LibraryItemKind::Hook, 1, 1),
                    (LibraryItemKind::Skill, 1, 0),
                    (LibraryItemKind::SubAgent, 1, 0),
                    (LibraryItemKind::Command, 1, 0),
                    (LibraryItemKind::Rule, 1, 0),
                    (LibraryItemKind::Profile, 1, 0),
                ]
            );
            let servers: Vec<(&str, bool)> = found[0]
                .names
                .iter()
                .map(|n| (n.name.as_str(), n.flagged))
                .collect();
            assert_eq!(
                servers,
                vec![("with-env", true), ("with-headers", true), ("plain", false)]
            );
        });
    }

    #[test]
    fn writes_nothing_to_the_home_or_the_project() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        with_test_home(home.path().to_path_buf(), || {
            seed_library();
            let mut config = full_config();
            // A cloned config can name these. The preview must not copy them
            // into user registries.
            config["skill_sources"] = serde_json::json!({
                "shared-skill": { "source": "evil/repo", "id": "x", "kind": "github" }
            });
            write_config(project.path(), config);
            let home_before = snapshot(home.path());
            let project_before = snapshot(project.path());

            preview(project.path());

            assert_eq!(snapshot(home.path()), home_before);
            assert_eq!(snapshot(project.path()), project_before);
        });
    }

    #[test]
    fn never_decrypts_and_never_returns_env_values() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        with_test_home(home.path().to_path_buf(), || {
            seed_library();
            // Any decrypt of this entry fails, so a passing preview proves
            // the path has no decrypt in it.
            write_undecryptable_mcp_server_config("locked");
            write_config(
                project.path(),
                serde_json::json!({ "name": "x", "mcp_servers": ["locked", "with-env"] }),
            );

            let result = preview(project.path());

            let group = match &result {
                ProjectLibraryUse::Readable { groups, .. } => &groups[0],
                other => panic!("the config should be readable: {other:?}"),
            };
            assert_eq!((group.total, group.flagged_total), (2, 2));
            let json = serde_json::to_string(&result).expect("serialize");
            assert!(!json.contains("secret-value-123"), "{json}");
            assert!(!json.contains("API_KEY"), "{json}");
            assert!(!json.contains("enc:v1"), "{json}");
        });
    }

    #[test]
    fn disabled_servers_and_the_automatic_entries_are_left_out() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        with_test_home(home.path().to_path_buf(), || {
            seed_library();
            seed_skill(AUTOMATIC_SKILL_NAME);
            write_config(
                project.path(),
                serde_json::json!({
                    "name": "x",
                    "mcp_servers": ["automatic", "with-env"],
                    "disabled_mcp_servers": ["with-env"],
                    "skills": ["automatic"],
                }),
            );

            assert!(groups(project.path()).is_empty());
        });
    }

    #[test]
    fn case_only_differences_match_as_sync_would() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        with_test_home(home.path().to_path_buf(), || {
            seed_library();
            write_config(
                project.path(),
                serde_json::json!({
                    "name": "x",
                    "mcp_servers": ["WITH-ENV"],
                    "skills": ["Shared-Skill"],
                }),
            );
            // The disk decides: true on a default macOS volume, false on
            // Linux. Sync resolves the skill through the same path.
            let skill_resolves =
                matches!(crate::core::read_skill_raw("Shared-Skill"), Ok(body) if !body.is_empty());

            // Sync compares MCP names with the listing, so case matters.
            assert_eq!(group_of(project.path(), LibraryItemKind::McpServer), None);
            let skills = group_of(project.path(), LibraryItemKind::Skill);
            assert_eq!(skills.is_some(), skill_resolves);
            if let Some(skills) = skills {
                assert_eq!(skills.names[0].name, "Shared-Skill");
            }
        });
    }

    #[test]
    fn a_long_list_is_capped_with_flagged_names_first_and_counted() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        with_test_home(home.path().to_path_buf(), || {
            let mut names: Vec<String> = Vec::new();
            for i in 0..60 {
                let name = format!("a-server-{i:02}");
                save_mcp_server_config(&name, r#"{"command":"npx"}"#).expect("server");
                names.push(name);
            }
            save_mcp_server_config("z-risky", r#"{"command":"npx","env":{"K":"v"}}"#)
                .expect("server");
            names.push("z-risky".to_string());
            let hooks: Vec<String> = (0..MAX_CHECKED_NAMES_PER_KIND + 5)
                .map(|i| format!("h{i}"))
                .collect();
            write_config(
                project.path(),
                serde_json::json!({ "name": "x", "mcp_servers": names, "hooks": hooks }),
            );

            let servers = group_of(project.path(), LibraryItemKind::McpServer).expect("servers");
            assert_eq!(servers.total, 61);
            assert_eq!(servers.names.len(), MAX_SHOWN_NAMES_PER_KIND);
            assert_eq!(servers.names[0].name, "z-risky");
            assert_eq!(servers.not_checked, 0);
            let hooks = group_of(project.path(), LibraryItemKind::Hook).expect("hooks");
            assert_eq!(hooks.total, 0);
            assert_eq!(hooks.not_checked, 5);
        });
    }

    #[test]
    fn odd_names_are_escaped_and_shortened() {
        let long = "n".repeat(MAX_SHOWN_NAME_CHARS + 20);
        for hidden in [
            '\u{034F}', '\u{061C}', '\u{115F}', '\u{180E}', '\u{1BCA0}', '\u{1BCA3}', '\u{3164}',
            '\u{FE00}', '\u{FE0F}', '\u{FFF9}', '\u{FFFB}', '\u{E0000}', '\u{E0FFF}', '\u{0890}', '\u{0891}', '\u{08E2}',
            '\u{110BD}', '\u{110CD}', '\u{13430}', '\u{1343F}',
        ] {
            assert_eq!(displayable_name(&format!("a{hidden}b")), "a\u{FFFD}b");
        }
        assert_eq!(displayable_name("a\u{202E}b\nc\u{200B}"), "a\u{FFFD}b\u{FFFD}c\u{FFFD}");
        assert_eq!(displayable_name("<b>plain</b>"), "<b>plain</b>");
        let shown = displayable_name(&long);
        assert_eq!(shown.chars().count(), MAX_SHOWN_NAME_CHARS + 1);
        assert!(shown.ends_with('…'));
    }

    #[test]
    fn a_bad_config_is_reported_as_unreadable_without_its_text() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        with_test_home(home.path().to_path_buf(), || {
            fs::write(project.path().join(".automatic.json"), "<<<<<<< HEAD\n{").expect("write");
            assert_eq!(preview(project.path()), ProjectLibraryUse::Unreadable);

            write_config(project.path(), serde_json::json!({ "name": "x", "skills": "nope" }));
            assert_eq!(preview(project.path()), ProjectLibraryUse::Unreadable);
        });
    }

    fn save_profile(name: &str, profile: Value) {
        let dir = get_profiles_dir().expect("profiles dir");
        fs::create_dir_all(&dir).expect("profiles dir");
        fs::write(dir.join(format!("{name}.json")), profile.to_string()).expect("profile");
    }

    #[test]
    fn a_profile_counts_what_it_provides_and_is_flagged_for_it() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        with_test_home(home.path().to_path_buf(), || {
            seed_library();
            save_profile(
                "risky",
                serde_json::json!({
                    "name": "risky",
                    "hooks": ["log-session"],
                    "mcp_servers": ["with-env", "with-headers", "automatic"],
                    "skills": ["shared-skill"],
                    "user_agents": ["shared-agent"],
                    "user_commands": ["shared-command"],
                    "rules": ["shared-rule"],
                }),
            );
            write_config(
                project.path(),
                serde_json::json!({
                    "name": "x",
                    "profiles": ["risky"],
                    "disabled_mcp_servers": ["with-headers"],
                }),
            );

            let found = groups(project.path());
            let summary: Vec<(LibraryItemKind, usize, usize)> = found
                .iter()
                .map(|g| (g.kind, g.total, g.flagged_total))
                .collect();
            assert_eq!(
                summary,
                vec![
                    (LibraryItemKind::McpServer, 1, 1),
                    (LibraryItemKind::Hook, 1, 1),
                    (LibraryItemKind::Skill, 1, 0),
                    (LibraryItemKind::SubAgent, 1, 0),
                    (LibraryItemKind::Command, 1, 0),
                    (LibraryItemKind::Rule, 1, 0),
                    (LibraryItemKind::Profile, 1, 1),
                ]
            );
        });
    }

    #[test]
    fn a_flagged_readable_profile_is_not_reported_as_unreadable() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        with_test_home(home.path().to_path_buf(), || {
            seed_library();
            save_profile("risky", serde_json::json!({ "name": "risky", "hooks": ["log-session"] }));
            write_config(project.path(), serde_json::json!({ "name": "x", "profiles": ["risky"] }));

            assert!(matches!(
                preview(project.path()),
                ProjectLibraryUse::Readable { has_unreadable_profile: false, .. }
            ));
        });
    }

    #[test]
    fn spellings_of_one_profile_file_share_one_flag() {
        let home = tempfile::tempdir().expect("home");
        with_test_home(home.path().to_path_buf(), || {
            save_profile("calm", serde_json::json!({ "name": "calm" }));
            let project = Project {
                profiles: vec!["CALM".into(), "Calm".into(), "calm".into()],
                ..Default::default()
            };

            let profiles = matched_profiles(&project);
            let flags = profile_flags(&project, &profiles, &BTreeSet::new());

            // A case-sensitive disk matches one spelling, a folding disk all
            // three. Either way there is one file and one flag.
            assert!(!profiles.is_empty());
            assert_eq!(flags.len(), 1);
        });
    }

    #[test]
    fn an_empty_profile_is_not_flagged_and_an_unreadable_one_is() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        with_test_home(home.path().to_path_buf(), || {
            save_profile("calm", serde_json::json!({ "name": "calm" }));
            let dir = get_profiles_dir().expect("profiles dir");
            fs::write(dir.join("broken.json"), "{ not json").expect("profile");
            write_config(
                project.path(),
                serde_json::json!({ "name": "x", "profiles": ["calm", "broken", "absent"] }),
            );

            let profiles = group_of(project.path(), LibraryItemKind::Profile).expect("profiles");
            let names: Vec<(&str, bool)> = profiles
                .names
                .iter()
                .map(|n| (n.name.as_str(), n.flagged))
                .collect();
            assert_eq!(names, vec![("broken", true), ("calm", false)]);
            assert!(matches!(
                preview(project.path()),
                ProjectLibraryUse::Readable { has_unreadable_profile: true, .. }
            ));
        });
    }

    #[test]
    fn a_remote_server_is_flagged_with_no_stored_values() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        with_test_home(home.path().to_path_buf(), || {
            save_mcp_server_config("typed", r#"{"type":"sse","url":"https://x.test"}"#)
                .expect("server");
            save_mcp_server_config("url-only", r#"{"url":"https://x.test"}"#).expect("server");
            save_mcp_server_config("local", r#"{"command":"npx"}"#).expect("server");
            write_config(
                project.path(),
                serde_json::json!({ "name": "x", "mcp_servers": ["typed", "url-only", "local"] }),
            );

            let servers = group_of(project.path(), LibraryItemKind::McpServer).expect("servers");
            assert_eq!((servers.total, servers.flagged_total), (3, 2));
        });
    }

    #[test]
    fn only_a_single_ordinary_component_is_looked_up() {
        assert!(is_single_path_component("shared-skill"));
        for name in ["", ".", "..", "a/b", "/abs", "a/"] {
            assert!(!is_single_path_component(name), "{name}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_drive_relative_name_is_refused() {
        assert!(!is_single_path_component("C:evil"));
    }

    #[test]
    fn reports_the_committed_read_only_hint() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        with_test_home(home.path().to_path_buf(), || {
            write_config(project.path(), serde_json::json!({ "name": "x", "read_only": true }));

            assert_eq!(
                preview(project.path()),
                ProjectLibraryUse::Readable {
                    read_only_hint: true,
                    has_unreadable_profile: false,
                    groups: Vec::new(),
                }
            );
        });
    }

    #[test]
    fn an_empty_directory_is_refused() {
        assert!(preview_project_library_use("").is_err());
    }
}
