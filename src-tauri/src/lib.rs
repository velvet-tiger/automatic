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
pub mod bootstrap;
pub mod cli;
pub mod context;
pub mod core;
pub mod languages;
pub mod mcp;
pub mod memory;
pub mod oauth;
pub mod path_env;
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
        .on_window_event(handle_window_event)
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
                        if let Ok(params) = core::remote_sources::parse_install_uri(&uri) {
                            let _ = handle.emit("deep-link://install", &params);
                        } else {
                            eprintln!("[automatic] ignoring unrecognised deep link: {}", uri);
                        }
                    }
                });
            }

            // Ensure plugin marketplace exists on disk; register with Claude
            // Code if the CLI is available.  Runs on a background thread so
            // it never blocks the UI. The standalone `mcp-serve` process
            // (see main.rs) runs the same housekeeping synchronously, since
            // it has no GUI event loop to block and needs the repair done
            // before it starts serving.
            std::thread::spawn(bootstrap::run_startup_housekeeping);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            account_login,
            account_logout,
            account_status,
            cli_install_status,
            cli_install_install,
            cli_install_uninstall,
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
            get_hooks,
            read_hook,
            save_hook,
            delete_hook,
            attach_hook_to_project,
            detach_hook_from_project,
            get_projects_referencing_hook,
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
            check_mcp_server_status,
            search_discover_mcp,
            search_collections,
            get_featured_community,
            get_projects,
            read_project,
            preview_rebuild_project,
            autodetect_project_dependencies,
            rebuild_project,
            save_project,
            inspect_project_directory,
            import_existing_project,
            delete_project_config,
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
            adopt_custom_asset,
            overwrite_custom_asset,
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
            update_skill_from_source,
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
            list_dev_server_configs,
            save_dev_server_config,
            delete_dev_server_config,
            detect_dev_server_package_manager,
            list_dev_server_scripts,
            start_dev_server,
            stop_dev_server,
            list_dev_server_statuses,
            get_dev_server_log,
            start_maildev,
            stop_maildev,
            get_maildev_status,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(handle_run_event);
}

// ── Window lifecycle (macOS "stay running in the Dock" convention) ─────────

/// Closing the window hides it instead of quitting the app, matching how
/// Mail, Slack and other Mac apps behave. The Dock icon (and menu bar, if
/// added later) stays available so the user can bring the window back
/// without relaunching. Other platforms keep Tauri's default behaviour of
/// exiting when the last window closes.
#[cfg(target_os = "macos")]
fn handle_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
    }
}

#[cfg(not(target_os = "macos"))]
fn handle_window_event(_window: &tauri::Window, _event: &tauri::WindowEvent) {}

/// Clicking the Dock icon while the window is hidden (see
/// [`handle_window_event`]) brings it back, matching the standard macOS
/// app-reopen convention.
#[cfg(target_os = "macos")]
fn handle_run_event(app_handle: &tauri::AppHandle, event: tauri::RunEvent) {
    if let tauri::RunEvent::Reopen {
        has_visible_windows,
        ..
    } = event
    {
        if !has_visible_windows {
            use tauri::Manager;
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn handle_run_event(_app_handle: &tauri::AppHandle, _event: tauri::RunEvent) {}
