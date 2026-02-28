use std::path::Path;

fn main() {
    // Forward API keys to the compiler so that option_env!() works in core.rs.
    // Priority order:
    //   1. Shell environment (CI/CD sets this directly — wins automatically via option_env!)
    //   2. .env file in the workspace root (dev convenience)
    //
    // We only parse .env when the key is NOT already in the shell environment,
    // so CI values are never shadowed.
    let keys_to_forward = ["ATTIO_API_KEY", "AMPLITUDE_API_KEY"];
    let missing: Vec<&str> = keys_to_forward
        .iter()
        .copied()
        .filter(|k| std::env::var(k).is_err())
        .collect();

    if !missing.is_empty() {
        // .env lives one level up from src-tauri/
        let env_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".env");

        if let Ok(contents) = std::fs::read_to_string(&env_path) {
            for line in contents.lines() {
                let line = line.trim();
                // Skip blank lines and comments
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim();
                    if missing.contains(&key) && !value.is_empty() {
                        println!("cargo:rustc-env={key}={value}");
                    }
                }
            }
        }
    }

    // Re-run if .env or any forwarded env var changes
    println!("cargo:rerun-if-changed=../.env");
    println!("cargo:rerun-if-env-changed=ATTIO_API_KEY");
    println!("cargo:rerun-if-env-changed=AMPLITUDE_API_KEY");

    tauri_build::build()
}
