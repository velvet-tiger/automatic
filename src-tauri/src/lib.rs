use keyring::Entry;
use std::fs;
use std::path::PathBuf;

#[tauri::command]
fn save_api_key(provider: &str, key: &str) -> Result<(), String> {
    let entry = Entry::new("agentic_desktop", provider).map_err(|e| e.to_string())?;
    entry.set_password(key).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_api_key(provider: &str) -> Result<String, String> {
    let entry = Entry::new("agentic_desktop", provider).map_err(|e| e.to_string())?;
    entry.get_password().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_skills() -> Result<Vec<String>, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    let skills_dir = home.join(".claude/skills");

    if !skills_dir.exists() {
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();
    let entries = fs::read_dir(skills_dir).map_err(|e| e.to_string())?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    skills.push(name.to_string());
                }
            }
        }
    }

    Ok(skills)
}

#[tauri::command]
fn get_mcp_servers() -> Result<String, String> {
    // Read the claude_desktop_config.json if it exists
    let home = dirs::home_dir().ok_or("Could not find home directory")?;

    // For Mac
    let config_path = home.join("Library/Application Support/Claude/claude_desktop_config.json");

    if config_path.exists() {
        fs::read_to_string(config_path).map_err(|e| e.to_string())
    } else {
        Ok("{}".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            save_api_key,
            get_api_key,
            get_skills,
            get_mcp_servers
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
