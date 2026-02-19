use keyring::Entry;
use std::fs;
use std::path::PathBuf;

fn get_skills_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".claude/skills"))
}

fn is_valid_skill_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && name != "." && name != ".."
}

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
    let skills_dir = get_skills_dir()?;

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
                    if is_valid_skill_name(name) {
                        skills.push(name.to_string());
                    }
                }
            }
        }
    }

    Ok(skills)
}

#[tauri::command]
fn read_skill(name: &str) -> Result<String, String> {
    if !is_valid_skill_name(name) {
        return Err("Invalid skill name".into());
    }
    let skills_dir = get_skills_dir()?;
    let skill_path = skills_dir.join(name).join("SKILL.md");

    if skill_path.exists() {
        fs::read_to_string(skill_path).map_err(|e| e.to_string())
    } else {
        Ok("".to_string()) // Return empty if no file exists yet
    }
}

#[tauri::command]
fn save_skill(name: &str, content: &str) -> Result<(), String> {
    if !is_valid_skill_name(name) {
        return Err("Invalid skill name".into());
    }
    let skills_dir = get_skills_dir()?;
    let skill_dir = skills_dir.join(name);

    if !skill_dir.exists() {
        fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
    }

    let skill_path = skill_dir.join("SKILL.md");
    fs::write(skill_path, content).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_skill(name: &str) -> Result<(), String> {
    if !is_valid_skill_name(name) {
        return Err("Invalid skill name".into());
    }
    let skills_dir = get_skills_dir()?;
    let skill_dir = skills_dir.join(name);

    if skill_dir.exists() {
        fs::remove_dir_all(&skill_dir).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
fn get_mcp_servers() -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
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
            read_skill,
            save_skill,
            delete_skill,
            get_mcp_servers
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
