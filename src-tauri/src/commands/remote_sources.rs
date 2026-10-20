use crate::core::remote_sources;

// ── Tauri Commands for Remote Sources ───────────────────────────────────────

/// Fetch a remote source manifest without installing.
/// Returns the parsed manifest as JSON for the frontend confirmation UI.
///
/// `dir` specifies a subdirectory within the repo where `automatic.json` lives
/// (monorepo support). When omitted, the repo root is used.
#[tauri::command]
pub fn fetch_remote_source(
    repo: String,
    git_ref: Option<String>,
    dir: Option<String>,
) -> Result<String, String> {
    let pin = git_ref.as_deref().map(remote_sources::pin_for_ref);

    let manifest = remote_sources::fetch_source_manifest(&repo, pin, dir.as_deref())?;
    serde_json::to_string(&manifest).map_err(|e| format!("Failed to serialize manifest: {}", e))
}

/// Install resources from a previously fetched source.
/// `selected` is an optional JSON-encoded SelectedResources.
/// `dir` specifies the subdirectory within the repo (must match what was passed to fetch).
/// `overrides` holds the `type:name` key of each conflict the user confirmed
/// replacing, from [`check_source_conflicts`]. With an unconfirmed conflict
/// nothing is installed and the result lists the conflicts.
#[tauri::command]
pub fn install_remote_source(
    repo: String,
    selected: Option<String>,
    dir: Option<String>,
    overrides: Option<Vec<String>>,
) -> Result<String, String> {
    let base_dir = remote_sources::resolve_base_dir(&repo, dir.as_deref())?;
    let manifest = remote_sources::parse_manifest(&base_dir)?;
    let selected_resources = parse_selected(selected)?;

    let result = remote_sources::install_source(
        &repo,
        &manifest,
        selected_resources,
        dir.as_deref(),
        &overrides.unwrap_or_default(),
    )?;
    serde_json::to_string(&result).map_err(|e| format!("Failed to serialize install result: {}", e))
}

/// Parse an optional JSON-encoded SelectedResources.
fn parse_selected(
    selected: Option<String>,
) -> Result<Option<remote_sources::SelectedResources>, String> {
    selected
        .map(|json| {
            serde_json::from_str(&json)
                .map_err(|e| format!("Failed to parse selected resources: {}", e))
        })
        .transpose()
}

/// Update a previously installed source (pull latest + re-install).
/// Each resource the update removed is pruned the way its delete command
/// prunes it.
#[tauri::command]
pub fn update_remote_source(repo: String) -> Result<String, String> {
    update_remote_source_with(&repo, remote_sources::update_source)
}

/// [`update_remote_source`] with the update step passed in, so a test can
/// run the rest without git.
fn update_remote_source_with(
    repo: &str,
    update: impl FnOnce(&str) -> Result<remote_sources::UpdateResult, String>,
) -> Result<String, String> {
    let result = update(repo)?;
    prune_update_removals(&result);
    serde_json::to_string(&result).map_err(|e| format!("Failed to serialize update result: {}", e))
}

/// Remove a source and all resources it provided. Each removed resource is
/// pruned the way its delete command prunes it. A resource that could not
/// be removed is left everywhere it is used.
#[tauri::command]
pub fn remove_remote_source(repo: String) -> Result<String, String> {
    let result = remote_sources::remove_source(&repo)?;
    for key in &result.removed {
        if let Some((resource_type, name)) = key.split_once(':') {
            prune_removed_resource(resource_type, name);
        }
    }
    serde_json::to_string(&result).map_err(|e| format!("Failed to serialize result: {}", e))
}

fn prune_update_removals(result: &remote_sources::UpdateResult) {
    for (resource_type, names) in &result.removed {
        for name in names {
            prune_removed_resource(resource_type, name);
        }
    }
}

/// Take a resource a source removed out of every place that names it, with
/// the pruning its delete command runs. `resource_type` is the type the
/// source lists it under. Nothing names a template or a collection.
fn prune_removed_resource(resource_type: &str, name: &str) {
    match resource_type {
        "skills" | "skill" => super::skills::prune_skill_references(name),
        "mcp_servers" | "mcp_server" => super::mcp_servers::prune_mcp_server_references(name),
        "rules" | "rule" => super::rules::prune_rule_references(name),
        "commands" | "command" => super::user_commands::prune_user_command_references(name),
        "agents" | "agent" => super::subagents::prune_subagent_references(name),
        _ => {}
    }
}

/// List all registered remote sources.
#[tauri::command]
pub fn list_remote_sources() -> Result<String, String> {
    let sources = remote_sources::list_sources()?;
    serde_json::to_string(&sources).map_err(|e| format!("Failed to serialize sources: {}", e))
}

/// Check for conflicts before installing a source: each existing library
/// entry the install would replace that this source does not own.
/// `dir` specifies the subdirectory within the repo (must match what was passed to fetch).
/// `selected` is an optional JSON-encoded SelectedResources; without it
/// every resource the manifest lists is checked.
#[tauri::command]
pub fn check_source_conflicts(
    repo: String,
    dir: Option<String>,
    selected: Option<String>,
) -> Result<String, String> {
    let base_dir = remote_sources::resolve_base_dir(&repo, dir.as_deref())?;
    let manifest = remote_sources::resolve_manifest(&base_dir)?;
    let selected_resources = parse_selected(selected)?;
    let conflicts = remote_sources::check_conflicts(&repo, &manifest, selected_resources.as_ref())?;
    serde_json::to_string(&conflicts).map_err(|e| format!("Failed to serialize conflicts: {}", e))
}

/// Handle an automatic:// deep link URI.
/// Returns the parsed parameters for the frontend to initiate the install flow.
#[tauri::command]
pub fn handle_install_uri(uri: String) -> Result<String, String> {
    let params = remote_sources::parse_install_uri(&uri)?;
    serde_json::to_string(&params).map_err(|e| format!("Failed to serialize: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::projects::propagation_tests::{make_project, read_back, with_temp_home};
    use crate::core;
    use std::fs;

    // MCP server names start with `source-remove-`: the secret store is
    // shared by the whole test process and no other test uses that prefix.
    const REPO: &str = "source-remove/commands";
    const SERVER: &str = "source-remove-cmd-server";
    const SKILL: &str = "source-remove-cmd-skill";
    const RULE: &str = "source-remove-cmd-rule";
    const COMMAND: &str = "source-remove-cmd-command";
    const AGENT: &str = "source-remove-cmd-agent";
    const PROFILE: &str = "source-remove-cmd-profile";

    /// Install a source that provides one MCP server, skill, rule, command
    /// and agent.
    fn install_fixture_source() {
        let clone = remote_sources::source_clone_path(REPO).expect("clone path");
        fs::create_dir_all(clone.join("skill")).expect("create clone");
        fs::write(
            clone.join("skill/SKILL.md"),
            format!("---\nname: {SKILL}\ndescription: A fixture skill\n---\n# Fixture\n"),
        )
        .expect("write skill");
        fs::write(clone.join("server.json"), r#"{"type":"stdio","command":"echo"}"#)
            .expect("write server");
        fs::write(clone.join("rule.md"), "# A rule\n").expect("write rule");
        fs::write(clone.join("command.md"), "# A command\n").expect("write command");
        fs::write(
            clone.join("agent.md"),
            format!("---\nname: {AGENT}\ndescription: A fixture agent\n---\nBody\n"),
        )
        .expect("write agent");
        let resource = |name: &str, path: &str| remote_sources::ManifestResource {
            name: name.to_string(),
            path: path.to_string(),
            description: String::new(),
        };
        let manifest: remote_sources::AutomaticManifest = serde_json::from_value(serde_json::json!({
            "name": "commands", "version": "1.0.0", "description": "Test",
            "skills": { "entries": [{ "name": SKILL, "path": "skill", "description": "x" }] },
        }))
        .expect("manifest");
        let manifest = remote_sources::AutomaticManifest {
            mcp_servers: vec![resource(SERVER, "server.json")],
            rules: vec![resource(RULE, "rule.md")],
            commands: vec![resource(COMMAND, "command.md")],
            agents: vec![resource(AGENT, "agent.md")],
            ..manifest
        };
        let result =
            remote_sources::install_source(REPO, &manifest, None, None, &[]).expect("install");
        assert!(result.skipped.is_empty(), "{:?}", result.warnings);
    }

    /// Save a profile that lists every fixture resource.
    fn save_fixture_profile() {
        let profile = core::ProjectProfile {
            name: PROFILE.to_string(),
            skills: vec![SKILL.to_string()],
            mcp_servers: vec![SERVER.to_string()],
            rules: vec![RULE.to_string()],
            user_commands: vec![COMMAND.to_string()],
            user_agents: vec![AGENT.to_string()],
            ..Default::default()
        };
        let data = serde_json::to_string(&profile).expect("profile json");
        core::save_project_profile(PROFILE, &data).expect("save profile");
    }

    fn mcp_json(dir: &std::path::Path) -> String {
        fs::read_to_string(dir.join(".mcp.json")).unwrap_or_default()
    }

    fn globally_selected(agent_id: &str) -> Vec<String> {
        core::global_mcp::load_global_mcp_state()
            .expect("global state")
            .agents
            .get(agent_id)
            .map(|state| state.selected.clone())
            .unwrap_or_default()
    }

    #[test]
    fn removing_a_source_takes_its_resources_out_of_projects_profiles_and_global_selections() {
        with_temp_home(|_| {
            install_fixture_source();
            save_fixture_profile();
            let (_keep, dir) = make_project("source-remove-project", |p| {
                p.profiles = vec![PROFILE.to_string()];
            });
            super::super::projects::reconcile_projects_referencing_profile(PROFILE);
            crate::sync::global_mcp::apply_global_mcp("cursor", vec![SERVER.to_string()])
                .expect("select globally");

            let before = read_back("source-remove-project");
            assert_eq!(before.skills, vec![SKILL]);
            assert_eq!(before.mcp_servers, vec![SERVER]);
            assert_eq!(before.file_rules["_project"], vec![RULE]);
            assert_eq!(before.user_commands, vec![COMMAND]);
            assert_eq!(before.user_agents, vec![AGENT]);
            assert!(mcp_json(&dir).contains(SERVER), "the project was synced first");
            assert_eq!(globally_selected("cursor"), vec![SERVER]);

            let raw = remove_remote_source(REPO.to_string()).expect("remove");

            let result: remote_sources::RemoveResult = serde_json::from_str(&raw).expect("parse");
            assert_eq!(result.removed.len(), 5, "{:?}", result);
            let after = read_back("source-remove-project");
            assert!(after.skills.is_empty(), "{:?}", after.skills);
            assert!(after.mcp_servers.is_empty(), "{:?}", after.mcp_servers);
            assert!(
                after.file_rules.values().all(|rules| !rules.contains(&RULE.to_string())),
                "{:?}",
                after.file_rules
            );
            assert!(after.user_commands.is_empty(), "{:?}", after.user_commands);
            assert!(after.user_agents.is_empty(), "{:?}", after.user_agents);
            let profile = core::read_project_profile_parsed(PROFILE).expect("read profile");
            assert!(profile.skills.is_empty());
            assert!(profile.mcp_servers.is_empty());
            assert!(profile.rules.is_empty());
            assert!(profile.user_commands.is_empty());
            assert!(profile.user_agents.is_empty());
            assert_eq!(globally_selected("cursor"), Vec::<String>::new());
            assert!(!mcp_json(&dir).contains(SERVER), "{}", mcp_json(&dir));
        });
    }

    #[test]
    fn a_resource_that_could_not_be_removed_stays_in_its_projects() {
        with_temp_home(|_| {
            install_fixture_source();
            // A skill name an older version accepted, refused since, listed
            // on the source and on a project.
            let mut source = remote_sources::get_source(REPO)
                .expect("read")
                .expect("registered");
            source
                .resources
                .entry("skills".to_string())
                .or_default()
                .push("group/skill".to_string());
            let sources_path = core::get_automatic_dir().expect("dir").join("sources.json");
            fs::write(&sources_path, serde_json::to_string(&vec![source]).expect("json"))
                .expect("write sources");
            remote_sources::record_provenance("skill", "group/skill", REPO).expect("provenance");
            let (_keep, _dir) = make_project("source-remove-project", |p| {
                p.skills = vec!["group/skill".to_string(), SKILL.to_string()];
                p.file_rules
                    .insert("_project".to_string(), vec![RULE.to_string()]);
            });

            let raw = remove_remote_source(REPO.to_string()).expect("remove");

            let result: remote_sources::RemoveResult = serde_json::from_str(&raw).expect("parse");
            assert_eq!(result.failed.len(), 1, "{:?}", result);
            assert_eq!(result.failed[0].resource, "skills:group/skill");
            let after = read_back("source-remove-project");
            assert_eq!(after.skills, vec!["group/skill"], "only the removed skill is pruned");
            assert!(after.file_rules.is_empty(), "{:?}", after.file_rules);
        });
    }

    #[test]
    fn the_update_command_prunes_what_the_update_removed() {
        with_temp_home(|_| {
            core::save_rule(RULE, "Rule", "Body.\n").expect("save rule");
            let (_keep, _dir) = make_project("source-remove-project", |p| {
                p.file_rules
                    .insert("_project".to_string(), vec![RULE.to_string()]);
            });
            let mut removed = std::collections::HashMap::new();
            removed.insert("rules".to_string(), vec![RULE.to_string()]);
            let update = |repo: &str| {
                assert_eq!(repo, REPO);
                Ok(remote_sources::UpdateResult {
                    added: Default::default(),
                    updated: Default::default(),
                    new_available: Default::default(),
                    removed,
                    failed: Vec::new(),
                    kept: Vec::new(),
                    warnings: Vec::new(),
                    old_version: "1.0.0".to_string(),
                    new_version: "2.0.0".to_string(),
                })
            };

            let raw = update_remote_source_with(REPO, update).expect("update");

            let result: serde_json::Value = serde_json::from_str(&raw).expect("parse");
            assert_eq!(result["removed"]["rules"], serde_json::json!([RULE]));
            assert!(read_back("source-remove-project").file_rules.is_empty());
        });
    }

    #[test]
    fn the_resources_an_update_removed_are_pruned_from_projects() {
        with_temp_home(|_| {
            core::save_rule(RULE, "Rule", "Body.\n").expect("save rule");
            core::save_rule("source-remove-cmd-kept", "Kept", "Body.\n").expect("save rule");
            let (_keep, _dir) = make_project("source-remove-project", |p| {
                p.file_rules.insert(
                    "_project".to_string(),
                    vec![RULE.to_string(), "source-remove-cmd-kept".to_string()],
                );
            });
            let mut removed = std::collections::HashMap::new();
            removed.insert("rules".to_string(), vec![RULE.to_string()]);
            removed.insert("collection".to_string(), vec!["some-slug".to_string()]);

            prune_update_removals(&remote_sources::UpdateResult {
                added: Default::default(),
                updated: Default::default(),
                new_available: Default::default(),
                removed,
                failed: Vec::new(),
                kept: Vec::new(),
                warnings: Vec::new(),
                old_version: "1.0.0".to_string(),
                new_version: "2.0.0".to_string(),
            });

            let after = read_back("source-remove-project");
            assert_eq!(after.file_rules["_project"], vec!["source-remove-cmd-kept"]);
        });
    }

    #[test]
    fn a_removal_that_fails_at_the_end_prunes_on_the_retry() {
        with_temp_home(|_| {
            install_fixture_source();
            let (_keep, _dir) = make_project("source-remove-project", |p| {
                p.file_rules
                    .insert("_project".to_string(), vec![RULE.to_string()]);
            });
            // Every resource is deleted, then unregistering the source fails.
            remote_sources::fail_sources_writes_after(Some(0));
            let first = remove_remote_source(REPO.to_string());
            remote_sources::fail_sources_writes_after(None);
            assert!(first.is_err(), "{:?}", first);
            assert_eq!(read_back("source-remove-project").file_rules["_project"], vec![RULE]);

            let raw = remove_remote_source(REPO.to_string()).expect("retry");

            let result: remote_sources::RemoveResult = serde_json::from_str(&raw).expect("parse");
            assert!(
                result.removed.contains(&format!("rules:{RULE}")),
                "{:?}",
                result
            );
            assert_eq!(result.removed.len(), 5, "{:?}", result);
            assert!(read_back("source-remove-project").file_rules.is_empty());
            assert_eq!(remote_sources::get_source(REPO), Ok(None));
        });
    }

    #[test]
    fn an_update_that_fails_at_the_end_prunes_on_the_retry() {
        with_temp_home(|_| {
            let clone = remote_sources::source_clone_path(REPO).expect("clone path");
            fs::create_dir_all(&clone).expect("create clone");
            fs::write(clone.join("rule.md"), "# A rule\n").expect("write rule");
            fs::write(clone.join("command.md"), "# A command\n").expect("write command");
            let manifest = |with_rule: bool| {
                let mut manifest = serde_json::json!({
                    "name": "commands", "version": "1.0.0", "description": "Test",
                    "commands": [{ "name": COMMAND, "path": "command.md", "description": "" }],
                });
                if with_rule {
                    manifest["rules"] =
                        serde_json::json!([{ "name": RULE, "path": "rule.md", "description": "" }]);
                }
                fs::write(clone.join("automatic.json"), manifest.to_string()).expect("manifest");
            };
            manifest(true);
            let parsed = remote_sources::parse_manifest(&clone).expect("parse manifest");
            remote_sources::install_source(REPO, &parsed, None, None, &[]).expect("install");
            let (_keep, _dir) = make_project("source-remove-project", |p| {
                p.file_rules
                    .insert("_project".to_string(), vec![RULE.to_string()]);
            });
            manifest(false);
            let reinstall = |repo: &str| {
                let existing = remote_sources::get_source(repo)?.expect("registered");
                remote_sources::reinstall_source(repo, &existing)
            };

            // The reinstall's write and the write of every name succeed. The
            // last write, after the rule is deleted, fails.
            remote_sources::fail_sources_writes_after(Some(2));
            let first = update_remote_source_with(REPO, reinstall);
            remote_sources::fail_sources_writes_after(None);
            assert!(first.is_err(), "{:?}", first);
            assert_eq!(read_back("source-remove-project").file_rules["_project"], vec![RULE]);

            let raw = update_remote_source_with(REPO, reinstall).expect("retry");

            let result: serde_json::Value = serde_json::from_str(&raw).expect("parse");
            assert_eq!(result["removed"]["rules"], serde_json::json!([RULE]), "{}", raw);
            assert!(read_back("source-remove-project").file_rules.is_empty());
            assert_eq!(remote_sources::get_provenance("rule", RULE), Ok(None));
        });
    }

    /// A user save through each save command makes the resource the
    /// user's: the source no longer owns it, and removing the source leaves
    /// the user's version in place (VEL-385).
    #[test]
    fn a_user_save_takes_a_resource_away_from_its_source() {
        with_temp_home(|_| {
            install_fixture_source();
            const TEMPLATE: &str = "source-remove-cmd-template";
            core::save_template(TEMPLATE, r#"{"name":"source-remove-cmd-template"}"#)
                .expect("save template");
            remote_sources::record_provenance("template", TEMPLATE, REPO).expect("own template");

            crate::commands::save_skill(
                SKILL,
                &format!("---\nname: {SKILL}\ndescription: Mine\n---\n# Mine\n"),
            )
            .expect("save skill");
            crate::commands::save_mcp_server_config(SERVER, r#"{"command":"mine"}"#)
                .expect("save server");
            crate::commands::save_rule(RULE, "Rule", "Mine.\n").expect("save rule");
            crate::commands::save_user_command(COMMAND.to_string(), "# Mine\n".to_string())
                .expect("save command");
            crate::commands::save_subagent(
                AGENT.to_string(),
                "Agent".to_string(),
                format!("---\nname: {AGENT}\ndescription: Mine\n---\nMine\n"),
            )
            .expect("save agent");
            crate::commands::save_template(TEMPLATE, r#"{"name":"mine"}"#).expect("save template");

            for (kind, name) in [
                ("skill", SKILL),
                ("mcp_server", SERVER),
                ("rule", RULE),
                ("command", COMMAND),
                ("agent", AGENT),
                ("template", TEMPLATE),
            ] {
                assert_eq!(
                    remote_sources::get_provenance(kind, name),
                    Ok(None),
                    "{kind} '{name}' is still owned by the source"
                );
            }

            let raw = remove_remote_source(REPO.to_string()).expect("remove");
            let result: remote_sources::RemoveResult = serde_json::from_str(&raw).expect("parse");
            assert!(result.removed.is_empty(), "{:?}", result);
            assert!(core::read_rule(RULE).expect("read rule").contains("Mine."));
            assert!(core::read_mcp_server_config(SERVER)
                .expect("read server")
                .contains("mine"));
        });
    }
}
