use std::path::Path;

use super::types::{NpmScriptEntry, PackageManager};

/// Detect which package manager governs a directory, from its lockfile.
/// Falls back to npm when only `package.json` is present (no lockfile
/// committed yet). Returns `None` when there is no `package.json` at all.
pub fn detect_package_manager(dir: &Path) -> Option<PackageManager> {
    if dir.join("pnpm-lock.yaml").is_file() {
        return Some(PackageManager::Pnpm);
    }
    if dir.join("yarn.lock").is_file() {
        return Some(PackageManager::Yarn);
    }
    if dir.join("package-lock.json").is_file() {
        return Some(PackageManager::Npm);
    }
    if dir.join("package.json").is_file() {
        return Some(PackageManager::Npm);
    }
    None
}

/// Read the `scripts` object from a directory's `package.json`, sorted by
/// name (this crate's `serde_json` is built without the `preserve_order`
/// feature, so object key order is not preserved from the source file).
pub fn list_npm_scripts(dir: &Path) -> Result<Vec<NpmScriptEntry>, String> {
    let path = dir.join("package.json");
    if !path.is_file() {
        return Err(format!("No package.json in '{}'", dir.display()));
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid package.json: {}", e))?;

    let Some(scripts) = value.get("scripts").and_then(|s| s.as_object()) else {
        return Ok(Vec::new());
    };

    Ok(scripts
        .iter()
        .filter_map(|(name, command)| {
            command.as_str().map(|command| NpmScriptEntry {
                name: name.clone(),
                command: command.to_string(),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(dir: &Path, name: &str, content: &str) {
        std::fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn detects_pnpm_from_lockfile() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "package.json", "{}");
        write(tmp.path(), "pnpm-lock.yaml", "");
        assert_eq!(detect_package_manager(tmp.path()), Some(PackageManager::Pnpm));
    }

    #[test]
    fn detects_yarn_from_lockfile() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "package.json", "{}");
        write(tmp.path(), "yarn.lock", "");
        assert_eq!(detect_package_manager(tmp.path()), Some(PackageManager::Yarn));
    }

    #[test]
    fn falls_back_to_npm_with_only_package_json() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "package.json", "{}");
        assert_eq!(detect_package_manager(tmp.path()), Some(PackageManager::Npm));
    }

    #[test]
    fn none_without_package_json() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(detect_package_manager(tmp.path()), None);
    }

    #[test]
    fn lists_scripts_by_name() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{"scripts": {"dev": "vite", "build": "vite build"}}"#,
        );
        let scripts = list_npm_scripts(tmp.path()).unwrap();
        assert_eq!(scripts.len(), 2);
        assert_eq!(scripts[0].name, "build");
        assert_eq!(scripts[0].command, "vite build");
        assert_eq!(scripts[1].name, "dev");
        assert_eq!(scripts[1].command, "vite");
    }

    #[test]
    fn empty_scripts_when_key_missing() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "package.json", "{}");
        assert_eq!(list_npm_scripts(tmp.path()).unwrap(), Vec::new());
    }

    #[test]
    fn errors_without_package_json() {
        let tmp = TempDir::new().unwrap();
        assert!(list_npm_scripts(tmp.path()).is_err());
    }
}
