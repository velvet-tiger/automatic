// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "mcp-serve" {
        // Run the same startup housekeeping the GUI runs on launch —
        // including the check that repairs a stale `automatic` binary path
        // in a project's .mcp.json and re-syncs it. `mcp-serve` is normally
        // launched directly by the calling agent (e.g. Claude Code) without
        // the GUI ever running, so without this the repair never happens:
        // after an app move or update, the agent's next restart of its MCP
        // servers finds a dead path for `automatic` and everything proxied
        // through it. Runs synchronously (there is no GUI event loop here to
        // block) so the repair completes before the server starts serving.
        automatic_lib::bootstrap::run_startup_housekeeping();

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
