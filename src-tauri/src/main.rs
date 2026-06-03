// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "mcp-serve" {
        // Ensure Discover catalogue files exist on disk before serving.
        // Uses force=false so an existing (app-written) file is never overwritten;
        // this only seeds the files when they are absent (e.g. first run without
        // the GUI, or the user deleted them).
        if let Err(e) = automatic_lib::core::init_discover_files(false) {
            eprintln!("[automatic] discover init error: {}", e);
        }

        // Run as MCP server on stdio
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(async {
            if let Err(e) = automatic_lib::mcp::run_mcp_server().await {
                eprintln!("MCP server error: {}", e);
                std::process::exit(1);
            }
        });
    } else if args.len() > 2 && args[1] == "mcp-proxy" {
        // Run as a transparent MCP proxy: stdio ↔ remote HTTP with keychain auth
        let server_name = args[2].clone();
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(async {
            if let Err(e) = automatic_lib::proxy::run_proxy(&server_name).await {
                eprintln!("MCP proxy error: {}", e);
                std::process::exit(1);
            }
        });
    } else if args.len() > 1 && automatic_lib::cli::is_cli_verb(&args[1]) {
        // CLI mode: dispatch to the structured command surface. On Windows
        // release builds the binary is linked as a windows subsystem (so the
        // GUI does not show a console); attach to the parent console first
        // so that stdout/stderr are visible to the invoking shell.
        attach_parent_console();
        let code = automatic_lib::cli::run(args);
        std::process::exit(code);
    } else {
        // Default: launch Tauri desktop app
        automatic_lib::run();
    }
}

#[cfg(windows)]
fn attach_parent_console() {
    // Best-effort: if no parent console exists (double-clicked GUI launch
    // that somehow ended up here) the call is a no-op.
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
fn attach_parent_console() {}
