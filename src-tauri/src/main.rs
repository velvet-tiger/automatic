// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "mcp-serve" {
        // Run as MCP server on stdio
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(async {
            if let Err(e) = automatic_lib::mcp::run_mcp_server().await {
                eprintln!("MCP server error: {}", e);
                std::process::exit(1);
            }
        });
    } else {
        // Spawn Tauri on a thread with an explicit 64 MB stack.
        // The default main-thread stack (1–4 MB on Windows ARM64 / Parallels)
        // is too small for Tauri's initialisation when combined with the
        // embedded default skills, templates and rules, causing a stack
        // overflow (0xc00000fd) before the window appears.
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024) // 64 MB
            .spawn(|| {
                automatic_lib::run();
            })
            .expect("Failed to spawn main app thread")
            .join()
            .expect("Main app thread panicked");
    }
}
