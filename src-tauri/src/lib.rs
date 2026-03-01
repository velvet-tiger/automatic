pub mod agent;
pub mod core;
pub mod mcp;
pub mod memory;
pub mod sync;
pub mod context;

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
            resolve_author,
            read_profile,
            save_profile,
            read_settings,
            write_settings,
            save_api_key,
            get_api_key,
            list_agents,
            list_agents_with_projects,
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
            import_local_skill,
            sync_local_skills,
            read_local_skill,
            save_local_skill,
            install_plugin_marketplace,
            get_sessions,
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
            check_installed_editors,
            open_in_editor,
            get_editor_icon,
            track_event,
            restart_app,
            subscribe_newsletter,
        ])
        // `tauri::generate_context!()` expands to a large amount of code that
        // can overflow the Windows main-thread stack (1 MiB) when many plugins,
        // skills and templates are embedded. Generate the context on a temporary
        // thread with an 8 MiB stack — matching the fix Tauri merged in PR #10734
        // — then run the app on the main thread as required by macOS/Windows.
        .run({
            let ctx = std::thread::Builder::new()
                .name("tauri-context-init".into())
                .stack_size(8 * 1024 * 1024)
                .spawn(|| tauri::generate_context!())
                .expect("failed to spawn context init thread")
                .join()
                .unwrap_or_else(|_| {
                    eprintln!("[automatic] tauri context init panicked");
                    std::process::exit(101);
                });
            ctx
        })
        .expect("error while running tauri application");
}
