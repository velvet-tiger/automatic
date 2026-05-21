#![allow(clippy::collapsible_else_if)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::manual_flatten)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::never_loop)]
#![allow(clippy::new_without_default)]
#![allow(clippy::nonminimal_bool)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::unnecessary_map_or)]
#![allow(clippy::unnecessary_sort_by)]
#![allow(clippy::unwrap_or_default)]

// Release verification currently treats Clippy warnings as errors. These
// targeted allowances keep the existing codebase releasable without forcing a
// broad, unrelated refactor during release prep.

pub mod account;
pub mod activity;
pub mod agent;
pub mod context;
pub mod core;
pub mod languages;
pub mod mcp;
pub mod memory;
pub mod oauth;
pub mod plugins;
pub mod proxy;
// Re-export the build plugin's features module under its previous top-level
// path so that mcp.rs and other crates can continue to use `crate::features::`.
pub use plugins::build::features;
pub mod recommendations;
pub mod sync;

mod commands;

// ── App Entry ────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use commands::*;

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            // ── Deep-link handler ────────────────────────────���────────────
            // Listens for automatic:// URLs from the OS and emits a Tauri
            // event so the React frontend can show an install dialog.
            {
                use tauri::Emitter;
                use tauri_plugin_deep_link::DeepLinkExt;

                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        let uri = url.as_str().to_string();
                        if let Ok(params) =
                            core::remote_sources::parse_install_uri(&uri)
                        {
                            let _ = handle.emit("deep-link://install", &params);
                        } else {
                            eprintln!("[automatic] ignoring unrecognised deep link: {}", uri);
                        }
                    }
                });
            }

            // Ensure plugin marketplace exists on disk; register with Claude
            // Code if the CLI is available.  Runs on a background thread so
            // it never blocks the UI.
            std::thread::spawn(|| {
                // Version-gated skill reinstall: if the stored version differs
                // from the current binary version, overwrite all bundled skills
                // so on-disk copies always match what shipped in this release.
                const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
                let force_reinstall = match core::read_settings() {
                    Ok(settings) => settings
                        .bundled_skills_version
                        .as_deref()
                        .map(|v| v != APP_VERSION)
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
                    Ok(moved) if !moved.is_empty() => eprintln!(
                        "[automatic] migrated library layout: {:?}",
                        moved
                    ),
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
                    Err(e) => eprintln!("[automatic] group scrub skipped (list_projects failed): {}", e),
                }

                if let Err(e) = core::install_default_skills_inner(force_reinstall) {
                    eprintln!("[automatic] skill install error: {}", e);
                } else if force_reinstall {
                    // Persist the current version so we don't reinstall next launch.
                    match core::read_settings() {
                        Ok(mut settings) => {
                            settings.bundled_skills_version = Some(APP_VERSION.to_string());
                            if let Err(e) = core::write_settings(&settings) {
                                eprintln!(
                                    "[automatic] failed to persist bundled_skills_version: {}",
                                    e
                                );
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
                                        if let Err(e) =
                                            sync::sync_project_without_autodetect(&mut project)
                                        {
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

                // On an actual version upgrade the bundled skills, rules,
                // sub-agents and instructions in the library have just been
                // overwritten with new content. Re-sync every project so any
                // project that references one of these picks up the new
                // library copy on disk. Without this the bundled assets in
                // the library are current but the project copies remain
                // stale, defeating the core invariant Automatic exists to
                // enforce. Gated on `force_reinstall` so this only runs once
                // per upgrade.
                if force_reinstall {
                    eprintln!(
                        "[automatic] bundled assets refreshed; re-syncing all projects to propagate updates"
                    );
                    commands::resync_all_projects();
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            account_login,
            account_logout,
            account_status,
            cloud_build_bundle,
            cloud_sync_library,
            ai_chat,
            ai_chat_with_tools,
            ai_list_models,
            list_agent_models,
            resolve_author,
            read_profile,
            save_profile,
            get_feature_flags,
            read_settings,
            write_settings,
            reset_settings,
            reinstall_defaults,
            erase_app_data,
            dismiss_welcome,
            clear_opencode_cache,
            clean_opencode_snapshots,
            save_api_key,
            get_api_key,
            has_api_key,
            agent_features_enabled,
            delete_api_key,
            list_agents,
            list_agents_with_projects,
            detect_installed_agents,
            detect_agent_global_configs,
            import_agent_global_configs,
            import_agent_global_skills,
            get_skills,
            list_skill_directories,
            read_skill,
            get_skill_scan_state,
            save_skill,
            delete_skill,
            sync_skill,
            sync_all_skills,
            reinstall_default_skills,
            get_skill_resources,
            import_skill_from_local_path,
            import_skill_from_repository,
            import_skill_from_package,
            get_skill_collections,
            set_skill_collection,
            remove_skill_collection,
            get_instructions,
            read_instruction,
            save_instruction,
            delete_instruction,
            get_rules,
            read_rule,
            save_rule,
            delete_rule,
            get_projects_referencing_rule,
            sync_rule_to_project,
            get_templates,
            read_template,
            save_template,
            delete_template,
            rename_template,
            list_bundled_templates,
            read_bundled_template,
            import_bundled_template,
            search_bundled_templates,
            check_template_dependencies,
            apply_templates_to_project,
            get_project_file_info,
            read_project_file,
            save_project_file,
            adopt_instruction_file,
            overwrite_instruction_file,
            inspect_unified_candidates,
            switch_to_unified_mode,
            get_instruction_file_conflicts,
            ai_generate_instruction,
            ai_update_instruction,
            ai_generate_library_asset,
            read_doc_note,
            save_doc_note,
            delete_doc_note,
            get_mcp_servers,
            list_mcp_server_configs,
            read_mcp_server_config,
            save_mcp_server_config,
            delete_mcp_server_config,
            search_discover_mcp,
            search_collections,
            get_featured_community,
            get_projects,
            read_project,
            preview_rebuild_project,
            autodetect_project_dependencies,
            rebuild_project,
            save_project,
            rename_project,
            delete_project,
            sync_project,
            list_groups,
            read_group,
            save_group,
            delete_group,
            groups_for_project,
            get_agent_cleanup_preview,
            remove_agent_from_project,
            check_project_drift,
            check_project_problems,
            adopt_stale_skill,
            remove_stale_skill,
            get_project_context,
            get_project_docs,
            read_project_context_raw,
            read_project_docs_raw,
            save_project_context_raw,
            save_project_docs_raw,
            ai_generate_context,
            install_plugin_marketplace,
            get_sessions,
            scan_asset_content,
            list_app_plugins,
            set_app_plugin_enabled,
            is_app_plugin_enabled,
            get_plugin_locked_resources,
            search_remote_skills,
            fetch_remote_skill_content,
            import_remote_skill,
            get_skill_sources,
            check_skill_update,
            get_project_memories,
            store_memory,
            get_memory,
            list_memories,
            search_memories,
            delete_memory,
            clear_memories,
            get_claude_memory,
            check_installed_editors,
            open_in_editor,
            get_editor_icon,
            get_project_activity,
            get_project_activity_paged,
            get_project_activity_count,
            get_all_activity,
            track_event,
            restart_app,
            open_directory_dialog,
            open_file_dialog,
            subscribe_newsletter,
            unsubscribe_newsletter,
            authorize_mcp_server,
            has_mcp_oauth_token,
            get_mcp_oauth_token_status,
            revoke_mcp_oauth_token,
            refresh_mcp_oauth_token,
            add_recommendation,
            get_recommendation,
            list_recommendations,
            list_all_pending_recommendations,
            dismiss_recommendation,
            action_recommendation,
            delete_recommendation,
            clear_recommendations,
            count_recommendations,
            evaluate_project_recommendations,
            ai_generate_project_recommendations,
            get_ai_recommendations_timestamp,
            ai_suggest_skills,
            ai_suggest_mcp_servers,
            list_recommendations_by_source,
            list_tools,
            read_tool,
            save_tool,
            delete_tool,
            list_tools_with_detection,
            autodetect_tools_for_project,
            invoke_tool_command,
            get_task_log,
            append_task_log,
            list_features,
            get_feature,
            get_feature_with_updates,
            create_feature,
            update_feature,
            set_feature_state,
            move_feature,
            delete_feature,
            archive_feature,
            unarchive_feature,
            add_feature_update,
            get_feature_updates,
            estimate_tokens,
            get_subagents,
            read_subagent,
            save_subagent,
            delete_subagent,
            get_projects_referencing_subagent,
            get_user_commands,
            read_user_command,
            save_user_command,
            delete_user_command,
            rename_user_command,
            is_analytics_configured,
            get_whats_new,
            mark_whats_new_seen,
            fetch_remote_source,
            install_remote_source,
            update_remote_source,
            remove_remote_source,
            list_remote_sources,
            check_source_conflicts,
            handle_install_uri,
            get_recently_added_items,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
