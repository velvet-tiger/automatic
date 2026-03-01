// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "mcp-serve" {
        // Run as MCP server on stdio
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(8 * 1024 * 1024)
            .build()
            .expect("Failed to create tokio runtime");
        rt.block_on(async {
            if let Err(e) = automatic_lib::mcp::run_mcp_server().await {
                eprintln!("MCP server error: {}", e);
                std::process::exit(1);
            }
        });
    } else {
        // Create an ambient Tokio runtime with an 8MB stack size for all worker
        // and blocking threads. Tauri will detect and use this runtime for async
        // IPC commands. This prevents stack overflows (0xc00000fd) on Windows
        // ARM64 during deep JSON serialization or file I/O in commands like
        // check_project_drift.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(8 * 1024 * 1024)
            .build()
            .expect("Failed to create tokio runtime");
        
        let _guard = rt.enter();

        // Default: launch Tauri desktop app
        automatic_lib::run();
    }
}