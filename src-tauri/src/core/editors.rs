use serde::Serialize;

// ── Editor Detection & Open ───────────────────────────────────────────────────

/// Known editors with their detection strategy (macOS-first, cross-platform fallback).
#[derive(Debug, Clone, Serialize)]
pub struct EditorInfo {
    /// Stable identifier used when calling `open_in_editor`.
    pub id: String,
    /// Human-readable label shown in the UI.
    pub label: String,
    /// Whether this editor was detected as installed on the current machine.
    pub installed: bool,
}

/// Probe whether a given app bundle path exists OR a CLI command is on PATH.
fn app_installed(app_path: &str, cli_name: Option<&str>) -> bool {
    if std::path::Path::new(app_path).exists() {
        return true;
    }
    if let Some(cli) = cli_name {
        // Use `which` to check if the CLI is on PATH
        std::process::Command::new("which")
            .arg(cli)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else {
        false
    }
}

/// Return all supported editors with their installation status.
pub fn check_installed_editors() -> Vec<EditorInfo> {
    vec![
        EditorInfo {
            id: "finder".into(),
            label: "Finder".into(),
            // Finder is always available on macOS
            installed: cfg!(target_os = "macos"),
        },
        EditorInfo {
            id: "vscode".into(),
            label: "VS Code".into(),
            installed: app_installed("/Applications/Visual Studio Code.app", Some("code")),
        },
        EditorInfo {
            id: "cursor".into(),
            label: "Cursor".into(),
            installed: app_installed("/Applications/Cursor.app", Some("cursor")),
        },
        EditorInfo {
            id: "zed".into(),
            label: "Zed".into(),
            installed: app_installed("/Applications/Zed.app", Some("zed")),
        },
        EditorInfo {
            id: "textmate".into(),
            label: "TextMate".into(),
            installed: app_installed("/Applications/TextMate.app", Some("mate")),
        },
        EditorInfo {
            id: "antigravity".into(),
            label: "Antigravity".into(),
            installed: app_installed("/Applications/Antigravity.app", None),
        },
        EditorInfo {
            id: "xcode".into(),
            label: "Xcode".into(),
            installed: app_installed("/Applications/Xcode.app", Some("xed")),
        },
        // ── JetBrains IDEs ──────────────────────────────────────────────
        EditorInfo {
            id: "intellij".into(),
            label: "IntelliJ IDEA".into(),
            installed: app_installed("/Applications/IntelliJ IDEA.app", Some("idea")),
        },
        EditorInfo {
            id: "phpstorm".into(),
            label: "PhpStorm".into(),
            installed: app_installed("/Applications/PhpStorm.app", Some("phpstorm")),
        },
        EditorInfo {
            id: "webstorm".into(),
            label: "WebStorm".into(),
            installed: app_installed("/Applications/WebStorm.app", Some("webstorm")),
        },
        EditorInfo {
            id: "pycharm".into(),
            label: "PyCharm".into(),
            installed: app_installed("/Applications/PyCharm.app", Some("pycharm")),
        },
        EditorInfo {
            id: "rustrover".into(),
            label: "RustRover".into(),
            installed: app_installed("/Applications/RustRover.app", Some("rustrover")),
        },
        EditorInfo {
            id: "clion".into(),
            label: "CLion".into(),
            installed: app_installed("/Applications/CLion.app", Some("clion")),
        },
        EditorInfo {
            id: "goland".into(),
            label: "GoLand".into(),
            installed: app_installed("/Applications/GoLand.app", Some("goland")),
        },
        EditorInfo {
            id: "datagrip".into(),
            label: "DataGrip".into(),
            installed: app_installed("/Applications/DataGrip.app", Some("datagrip")),
        },
        EditorInfo {
            id: "rider".into(),
            label: "Rider".into(),
            installed: app_installed("/Applications/Rider.app", Some("rider")),
        },
    ]
}

/// Open a directory in the specified editor.
///
/// `editor_id` must match one of the `id` values returned by `check_installed_editors`.
/// `path` must be an absolute directory path.
pub fn open_in_editor(editor_id: &str, path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("No project directory set".into());
    }

    let status = match editor_id {
        "finder" => {
            // `open` on macOS opens Finder at the directory
            std::process::Command::new("open")
                .arg(path)
                .spawn()
                .map_err(|e| format!("Failed to open Finder: {}", e))?;
            return Ok(());
        }
        "vscode" => {
            // Prefer the CLI; fall back to `open -a`
            if which_available("code") {
                std::process::Command::new("code").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "Visual Studio Code", path])
                    .spawn()
            }
        }
        "cursor" => {
            if which_available("cursor") {
                std::process::Command::new("cursor").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "Cursor", path])
                    .spawn()
            }
        }
        "zed" => {
            if which_available("zed") {
                std::process::Command::new("zed").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "Zed", path])
                    .spawn()
            }
        }
        "textmate" => {
            if which_available("mate") {
                std::process::Command::new("mate").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "TextMate", path])
                    .spawn()
            }
        }
        "antigravity" => std::process::Command::new("open")
            .args(["-a", "Antigravity", path])
            .spawn(),
        "xcode" => {
            if which_available("xed") {
                std::process::Command::new("xed").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "Xcode", path])
                    .spawn()
            }
        }
        // ── JetBrains IDEs ──────────────────────────────────────────────
        "intellij" => {
            if which_available("idea") {
                std::process::Command::new("idea").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "IntelliJ IDEA", path])
                    .spawn()
            }
        }
        "phpstorm" => {
            if which_available("phpstorm") {
                std::process::Command::new("phpstorm").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "PhpStorm", path])
                    .spawn()
            }
        }
        "webstorm" => {
            if which_available("webstorm") {
                std::process::Command::new("webstorm").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "WebStorm", path])
                    .spawn()
            }
        }
        "pycharm" => {
            if which_available("pycharm") {
                std::process::Command::new("pycharm").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "PyCharm", path])
                    .spawn()
            }
        }
        "rustrover" => {
            if which_available("rustrover") {
                std::process::Command::new("rustrover").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "RustRover", path])
                    .spawn()
            }
        }
        "clion" => {
            if which_available("clion") {
                std::process::Command::new("clion").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "CLion", path])
                    .spawn()
            }
        }
        "goland" => {
            if which_available("goland") {
                std::process::Command::new("goland").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "GoLand", path])
                    .spawn()
            }
        }
        "datagrip" => {
            if which_available("datagrip") {
                std::process::Command::new("datagrip").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "DataGrip", path])
                    .spawn()
            }
        }
        "rider" => {
            if which_available("rider") {
                std::process::Command::new("rider").arg(path).spawn()
            } else {
                std::process::Command::new("open")
                    .args(["-a", "Rider", path])
                    .spawn()
            }
        }
        other => return Err(format!("Unknown editor id: {}", other)),
    };

    status.map(|_| ()).map_err(|e| e.to_string())
}

/// Convert the `.icns` file for `editor_id` to a PNG and return it as a
/// base64-encoded string suitable for use in a `data:image/png;base64,...` URL.
///
/// The PNG is cached in `/tmp/automatic-icons/` to avoid re-running `sips` on
/// every call.  Uses macOS `sips` (always available on macOS) to do the
/// conversion.  Returns an error string if the editor id is unknown, the
/// `.icns` file does not exist, or `sips` / IO fails.
pub fn get_editor_icon(editor_id: &str) -> Result<String, String> {
    let icns_path: &str = match editor_id {
        "finder" => "/System/Library/CoreServices/Finder.app/Contents/Resources/Finder.icns",
        "vscode" => "/Applications/Visual Studio Code.app/Contents/Resources/Code.icns",
        "cursor" => "/Applications/Cursor.app/Contents/Resources/Cursor.icns",
        "zed" => "/Applications/Zed.app/Contents/Resources/Zed.icns",
        "textmate" => "/Applications/TextMate.app/Contents/Resources/TextMate.icns",
        "antigravity" => "/Applications/Antigravity.app/Contents/Resources/Antigravity.icns",
        "xcode" => "/Applications/Xcode.app/Contents/Resources/Xcode.icns",
        // JetBrains IDEs
        "intellij" => "/Applications/IntelliJ IDEA.app/Contents/Resources/idea.icns",
        "phpstorm" => "/Applications/PhpStorm.app/Contents/Resources/PhpStorm.icns",
        "webstorm" => "/Applications/WebStorm.app/Contents/Resources/webstorm.icns",
        "pycharm" => "/Applications/PyCharm.app/Contents/Resources/PyCharm.icns",
        "rustrover" => "/Applications/RustRover.app/Contents/Resources/RustRover.icns",
        "clion" => "/Applications/CLion.app/Contents/Resources/CLion.icns",
        "goland" => "/Applications/GoLand.app/Contents/Resources/GoLand.icns",
        "datagrip" => "/Applications/DataGrip.app/Contents/Resources/DataGrip.icns",
        "rider" => "/Applications/Rider.app/Contents/Resources/Rider.icns",
        other => return Err(format!("Unknown editor id: {}", other)),
    };

    if !std::path::Path::new(icns_path).exists() {
        return Err(format!("Icon file not found: {}", icns_path));
    }

    let cache_dir = std::path::Path::new("/tmp/automatic-icons");
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| format!("Failed to create icon cache dir: {}", e))?;

    let out_path = cache_dir.join(format!("{}.png", editor_id));
    let out_str = out_path
        .to_str()
        .ok_or_else(|| "Invalid output path".to_string())?
        .to_string();

    // Convert .icns → PNG if not already cached
    if !out_path.exists() {
        let output = std::process::Command::new("sips")
            .args(["-s", "format", "png", icns_path, "--out", &out_str])
            .output()
            .map_err(|e| format!("Failed to run sips: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("sips failed: {}", stderr));
        }
    }

    // Read the PNG and return it as a base64 data URI so the frontend can
    // embed it directly without needing the Tauri asset protocol.
    let bytes =
        std::fs::read(&out_path).map_err(|e| format!("Failed to read cached icon: {}", e))?;

    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let b64 = STANDARD.encode(&bytes);

    Ok(format!("data:image/png;base64,{}", b64))
}

/// Return true when `name` resolves to an executable via `which`.
fn which_available(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
