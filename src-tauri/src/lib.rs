pub mod activity;
pub mod agent;
pub mod context;
pub mod core;
pub mod features;
pub mod languages;
pub mod mcp;
pub mod memory;
pub mod oauth;
pub mod plugins;
pub mod proxy;
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
        .setup(|_app| {
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

                // Seed (or refresh) the marketplace catalogue files in
                // ~/.automatic/marketplace/.  `force_reinstall` mirrors the
                // bundled-skills version gate so the files are overwritten
                // whenever the app ships a new release.
                if let Err(e) = core::init_marketplace_files(force_reinstall) {
                    eprintln!("[automatic] marketplace init error: {}", e);
                }

                if let Err(e) = core::install_default_skills_inner(force_reinstall) {
                    eprintln!("[automatic] skill install error: {}", e);
                } else if force_reinstall {
                    // Persist the current version so we don't reinstall next launch.
                    match core::read_settings() {
                        Ok(mut settings) => {
                            settings.bundled_skills_version = Some(APP_VERSION.to_string());
                            if let Err(e) = core::write_settings(&settings) {
                                eprintln!("[automatic] failed to persist bundled_skills_version: {}", e);
                            }
                        }
                        Err(e) => eprintln!("[automatic] failed to read settings after skill install: {}", e),
                    }
                }

                if let Err(e) = core::install_default_templates() {
                    eprintln!("[automatic] template install error: {}", e);
                }
                if let Err(e) = core::install_default_rules() {
                    eprintln!("[automatic] rule install error: {}", e);
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
                                Ok(raw) => {
                                    match serde_json::from_str::<core::Project>(&raw) {
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
                                    }
                                }
                                Err(e) => eprintln!(
                                    "[automatic] failed to read project '{}' for re-sync: {}",
                                    project_name, e
                                ),
                            }
                        }
                    }
                    Err(e) => eprintln!("[automatic] global MCP install error: {}", e),
                }
                // Reconcile tool registry with current plugin enabled states.
                core::reconcile_plugin_tools_on_startup();
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ai_chat,
            ai_chat_with_tools,
            ai_list_models,
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
            has_ai_key,
            delete_api_key,
            list_agents,
            list_agents_with_projects,
            detect_installed_agents,
            detect_agent_global_configs,
            import_agent_global_configs,
            import_agent_global_skills,
            get_skills,
            read_skill,
            save_skill,
            delete_skill,
            sync_skill,
            sync_all_skills,
            reinstall_default_skills,
            get_skill_resources,
            get_templates,
            read_template,
            save_template,
            delete_template,
            get_rules,
            read_rule,
            save_rule,
            delete_rule,
            get_projects_referencing_rule,
            sync_rule_to_project,
            get_project_templates,
            read_project_template,
            save_project_template,
            delete_project_template,
            rename_project_template,
            list_bundled_project_templates,
            read_bundled_project_template,
            import_bundled_project_template,
            search_bundled_project_templates,
            check_template_dependencies,
            get_project_file_info,
            read_project_file,
            save_project_file,
            adopt_instruction_file,
            overwrite_instruction_file,
            get_instruction_file_conflicts,
            ai_generate_instruction,
            ai_update_instruction,
            read_doc_note,
            save_doc_note,
            delete_doc_note,
            get_mcp_servers,
            list_mcp_server_configs,
            read_mcp_server_config,
            save_mcp_server_config,
            delete_mcp_server_config,
            search_mcp_marketplace,
            search_collections,
            get_projects,
            read_project,
            autodetect_project_dependencies,
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
            adopt_stale_skill,
            remove_stale_skill,
            get_project_context,
            get_project_docs,
            read_project_context_raw,
            read_project_docs_raw,
            save_project_context_raw,
            save_project_docs_raw,
            ai_generate_context,
            import_local_skill,
            sync_local_skills,
            read_local_skill,
            save_local_skill,
            install_plugin_marketplace,
            get_sessions,
            list_app_plugins,
            set_app_plugin_enabled,
            is_app_plugin_enabled,
            search_remote_skills,
            fetch_remote_skill_content,
            import_remote_skill,
            get_skill_sources,
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
            subscribe_newsletter,
            authorize_mcp_server,
            has_mcp_oauth_token,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
