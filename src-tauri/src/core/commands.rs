use serde::{Deserialize, Serialize};
use std::fs;

use super::paths::get_commands_dir;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserCommandEntry {
    pub id: String,
    #[serde(default)]
    pub description: String,
}

pub fn is_valid_command_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 {
        return false;
    }

    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }

    let mut prev_hyphen = false;
    for c in chars {
        if c == '-' {
            if prev_hyphen {
                return false;
            }
            prev_hyphen = true;
        } else if c.is_ascii_lowercase() || c.is_ascii_digit() {
            prev_hyphen = false;
        } else {
            return false;
        }
    }

    !name.ends_with('-')
}

pub fn list_user_commands() -> Result<Vec<UserCommandEntry>, String> {
    let dir = get_commands_dir()?;

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut commands = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !path.extension().is_some_and(|ext| ext == "md") {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };

        if !is_valid_command_name(stem) {
            continue;
        }

        let description = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| extract_frontmatter_field(&raw, "description"))
            .unwrap_or_default();

        commands.push(UserCommandEntry {
            id: stem.to_string(),
            description,
        });
    }

    Ok(commands)
}

pub fn read_user_command(machine_name: &str) -> Result<String, String> {
    if !is_valid_command_name(machine_name) {
        return Err("Invalid command name".into());
    }

    let path = get_commands_dir()?.join(format!("{machine_name}.md"));
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

pub fn save_user_command(machine_name: &str, content: &str) -> Result<(), String> {
    if !is_valid_command_name(machine_name) {
        return Err(
            "Invalid command name. Use lowercase letters, digits, and hyphens only.".into(),
        );
    }

    let dir = get_commands_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let path = dir.join(format!("{machine_name}.md"));
    fs::write(&path, content).map_err(|e| e.to_string())
}

pub fn rename_user_command(old_name: &str, new_name: &str) -> Result<(), String> {
    if !is_valid_command_name(old_name) {
        return Err("Invalid current command name".into());
    }
    if !is_valid_command_name(new_name) {
        return Err(
            "Invalid new command name. Use lowercase letters, digits, and hyphens only.".into(),
        );
    }
    if old_name == new_name {
        return Ok(());
    }

    let dir = get_commands_dir()?;
    let old_path = dir.join(format!("{old_name}.md"));
    let new_path = dir.join(format!("{new_name}.md"));

    if !old_path.exists() {
        return Err(format!("Command '{old_name}' not found"));
    }
    if new_path.exists() {
        return Err(format!("A command named '{new_name}' already exists"));
    }

    fs::rename(&old_path, &new_path).map_err(|e| format!("Failed to rename command: {e}"))
}

pub fn delete_user_command(machine_name: &str) -> Result<(), String> {
    if !is_valid_command_name(machine_name) {
        return Err("Invalid command name".into());
    }

    let path = get_commands_dir()?.join(format!("{machine_name}.md"));
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn extract_frontmatter_field(content: &str, field: &str) -> Option<String> {
    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return None;
    }

    let after_first = &content[4..];
    let end_marker_pos = after_first
        .find("\n---")
        .or_else(|| after_first.find("\r\n---"))?;
    let yaml_str = &after_first[..end_marker_pos];

    for line in yaml_str.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix(&format!("{field}:")) {
            return Some(
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            );
        }
    }

    None
}
