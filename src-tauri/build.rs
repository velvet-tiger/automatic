use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn main() {
    pack_library();


    // Forward API keys to the compiler so that option_env!() works in core.rs.
    // Priority order:
    //   1. Shell environment (CI/CD sets this directly — wins automatically via option_env!)
    //   2. .env file in the workspace root (dev convenience)
    //
    // We only parse .env when the key is NOT already in the shell environment,
    // so CI values are never shadowed.
    let keys_to_forward = [
        "ATTIO_API_KEY",
        "AMPLITUDE_API_KEY",
        "VITE_FLAGS",
        "AUTOMATIC_WEBAPP_URL",
    ];
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
    println!("cargo:rerun-if-env-changed=VITE_FLAGS");
    println!("cargo:rerun-if-env-changed=AUTOMATIC_WEBAPP_URL");

    tauri_build::build()
}

/// Pack `automatic-library/` (a git submodule at the app repo root) into
/// `${OUT_DIR}/library.zip`, and write the pinned semver to
/// `${OUT_DIR}/library_version.txt`. Consumed at compile time by
/// `src-tauri/src/core/bundled_library.rs`.
fn pack_library() {
    let library_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has no parent")
        .join("automatic-library");

    if !library_root.exists() {
        panic!(
            "automatic-library submodule missing at {} — run `git submodule update --init`",
            library_root.display()
        );
    }

    let version_path = library_root.join("VERSION");
    let version = fs::read_to_string(&version_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", version_path.display(), e))
        .trim()
        .to_string();
    if version.is_empty() {
        panic!("{} is empty", version_path.display());
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR unset"));
    fs::write(out_dir.join("library_version.txt"), &version)
        .expect("write library_version.txt");

    let zip_path = out_dir.join("library.zip");
    let file = File::create(&zip_path).expect("create library.zip");
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let mut entries: Vec<PathBuf> = Vec::new();
    collect_files(&library_root, &mut entries);
    entries.sort();

    for path in &entries {
        let rel = path
            .strip_prefix(&library_root)
            .expect("entry outside library root")
            .to_string_lossy()
            .replace('\\', "/");
        zip.start_file(&rel, options).expect("zip start_file");
        let mut source = File::open(path).expect("open source file");
        let mut buf = Vec::new();
        source.read_to_end(&mut buf).expect("read source");
        zip.write_all(&buf).expect("zip write");
    }
    zip.finish().expect("zip finish");

    println!("cargo:rerun-if-changed={}", library_root.display());
    println!(
        "cargo:rerun-if-changed={}",
        library_root.join("VERSION").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        library_root.join("manifest.json").display()
    );
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {}", dir.display(), e)) {
        let entry = entry.expect("dir entry");
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == ".git" || name == "node_modules" || name == "scripts" || name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}
