//! Startup housekeeping shared between the desktop GUI and the standalone
//! `mcp-serve` process.
//!
//! Before this module existed, this logic only ran on a background thread
//! inside the Tauri GUI's `setup` hook. That meant it never ran when Claude
//! Code (or any other agent) launches `automatic mcp-serve` directly without
//! the GUI open — so a stale `automatic` binary path in a project's
//! `.mcp.json` (left behind by an app move or update) was never repaired,
//! and every MCP server proxied through that binary looked dead on the
//! agent's next restart. `mcp-serve` now calls this synchronously before it
//! starts serving; the GUI still runs it on a background thread so it never
//! blocks the window from opening.

use crate::{commands, core, sync};

/// Run all startup housekeeping synchronously.
///
/// Idempotent: every step either no-ops when already done or safely
/// overwrites its own previous output. Failures are reported to stderr and
/// never abort the remaining steps, so one broken migration can't take the
/// whole app down with it.
pub fn run_startup_housekeeping() {
    // Version-gated default-asset reinstall: when the stored library
    // version differs from the current library shipped in the binary,
    // rewrite bundled defaults so on-disk copies match the new release.
    // Phase 2 tracks this on the library's own semver rather than
    // CARGO_PKG_VERSION, so a library release can ship without an app
    // release and vice versa.
    let library_version: &str = core::bundled_library::version();
    let force_reinstall = match core::read_settings() {
        Ok(settings) => settings
            .library_version
            .as_deref()
            .map(|v| v != library_version)
            .unwrap_or(true), // no version stored → treat as upgrade
        Err(_) => true, // can't read settings → safe to overwrite
    };

    // Seed (or refresh) the Discover catalogue files in
    // ~/.automatic/discover/.  `force_reinstall` mirrors the
    // bundled-skills version gate so the files are overwritten
    // whenever the app ships a new release.
    if let Err(e) = core::init_discover_files(force_reinstall) {
        eprintln!("[automatic] discover init error: {}", e);
    }

    // One-time migration: any skill Automatic previously wrote to
    // ~/.agents/skills/ (shared with OpenCode's global auto-load
    // path) is moved into the managed library at
    // ~/.automatic/library/skills/. Idempotent and safe to re-run.
    match core::migrate_agents_skills_to_library() {
        Ok(moved) if !moved.is_empty() => eprintln!(
            "[automatic] migrated {} skill(s) from ~/.agents/skills/ into managed library",
            moved.len()
        ),
        Ok(_) => {}
        Err(e) => eprintln!("[automatic] skill library migration error: {}", e),
    }

    // Move legacy top-level library directories under
    // `~/.automatic/library/`, renaming the three that didn't
    // match the user-facing UI labels (templates → instructions,
    // project_templates → templates, agents → subagents).
    // Idempotent and safe on every restart.
    match core::migrate_top_level_to_library() {
        Ok(moved) if !moved.is_empty() => {
            eprintln!("[automatic] migrated library layout: {:?}", moved)
        }
        Ok(_) => {}
        Err(e) => eprintln!("[automatic] library layout migration error: {}", e),
    }

    // Drop stale project references from group files. Heals data
    // written before delete_project/rename_project started cleaning
    // up their own group entries. Idempotent.
    match core::list_projects() {
        Ok(live) => match core::scrub_orphan_project_references(&live) {
            Ok(affected) if !affected.is_empty() => eprintln!(
                "[automatic] scrubbed orphan project references from groups: {:?}",
                affected
            ),
            Ok(_) => {}
            Err(e) => eprintln!("[automatic] group scrub error: {}", e),
        },
        Err(e) => eprintln!(
            "[automatic] group scrub skipped (list_projects failed): {}",
            e
        ),
    }

    if let Err(e) = core::install_default_skills_inner(force_reinstall) {
        eprintln!("[automatic] skill install error: {}", e);
    } else if force_reinstall {
        // Persist the current library version so we don't reinstall next
        // launch. Older installs may still have `bundled_skills_version`
        // set; leave it untouched — Settings keeps it as a legacy field
        // for round-trip safety and it is no longer read by anything.
        match core::read_settings() {
            Ok(mut settings) => {
                settings.library_version = Some(library_version.to_string());
                if let Err(e) = core::write_settings(&settings) {
                    eprintln!("[automatic] failed to persist library_version: {}", e);
                }
            }
            Err(e) => eprintln!(
                "[automatic] failed to read settings after skill install: {}",
                e
            ),
        }
    }

    if let Err(e) = core::install_default_instructions() {
        eprintln!("[automatic] instruction install error: {}", e);
    }
    if let Err(e) = core::install_default_rules() {
        eprintln!("[automatic] rule install error: {}", e);
    }
    if let Err(e) = core::install_default_subagents() {
        eprintln!("[automatic] sub-agent install error: {}", e);
    }

    // On an actual version upgrade the bundled skills, rules,
    // sub-agents and instructions in the library have just been
    // overwritten with new content. Re-sync every project *immediately*
    // after those writes — before MCP/plugin housekeeping — so the GUI's
    // early drift checks never see a window where the library is current
    // but project copies are still stale. Gated on `force_reinstall` so
    // this only runs once per upgrade.
    if force_reinstall {
        eprintln!(
            "[automatic] bundled assets refreshed; re-syncing all projects to propagate updates"
        );
        commands::resync_all_projects();
    }

    match core::install_plugin_marketplace() {
        Ok(msg) => eprintln!("[automatic] plugin startup: {}", msg),
        Err(e) => eprintln!("[automatic] plugin startup error: {}", e),
    }
    match core::ensure_automatic_in_global_mcp() {
        Ok(projects_to_sync) => {
            // Re-sync any project whose automatic entry was added or whose
            // binary path changed (dev→release or after an app update).
            // This keeps MCP config files and skill directories in sync
            // without requiring the user to press "Sync now".
            for project_name in projects_to_sync {
                match core::read_project(&project_name) {
                    Ok(raw) => match serde_json::from_str::<core::Project>(&raw) {
                        Ok(mut project) => {
                            if let Err(e) = sync::sync_project_without_autodetect(&mut project) {
                                eprintln!(
                                    "[automatic] startup re-sync failed for '{}': {}",
                                    project_name, e
                                );
                            }
                        }
                        Err(e) => eprintln!(
                            "[automatic] failed to parse project '{}' for re-sync: {}",
                            project_name, e
                        ),
                    },
                    Err(e) => eprintln!(
                        "[automatic] failed to read project '{}' for re-sync: {}",
                        project_name, e
                    ),
                }
            }
        }
        Err(e) => eprintln!("[automatic] global MCP install error: {}", e),
    }
    // Reconcile tool/skill/rule registries with current plugin states.
    core::reconcile_plugin_resources_on_startup();
}
