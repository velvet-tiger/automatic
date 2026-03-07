id = "rust"
name = "Rust"

[[detect]]
files = ["Cargo.toml"]

config_files = ["Cargo.toml"]

ignore_dirs = ["target", ".cargo"]

entry_points = [
    "src/main.rs",
    "src/lib.rs",
    "src-tauri/src/main.rs",
    "src-tauri/src/lib.rs",
    "src-tauri/src/core.rs",
    "src-tauri/src/core/mod.rs",
]
