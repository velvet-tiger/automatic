pub mod activity;
pub mod agent;
pub mod context;
pub mod core;
pub mod languages;
pub mod mcp;
pub mod memory;
pub mod oauth;
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
                if let Err(e) = core::install_default_skills() {
                    eprintln!("[automatic] skill install error: {}", e);
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
            erase_app_data,
            dismiss_welcome,
            save_api_key,
            get_api_key,
            has_api_key,
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
            get_mcp_servers,
            list_mcp_server_configs,
            read_mcp_server_config,
            save_mcp_server_config,
            delete_mcp_server_config,
            get_projects,
            read_project,
            autodetect_project_dependencies,
            save_project,
            rename_project,
            delete_project,
            sync_project,
            get_agent_cleanup_preview,
            remove_agent_from_project,
            check_project_drift,
            get_project_context,
            read_project_context_raw,
            save_project_context_raw,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
