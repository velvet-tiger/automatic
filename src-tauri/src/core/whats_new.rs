use serde::{Deserialize, Serialize};

use super::settings::{read_settings, write_settings};

/// A collection of updates for a specific release version.
/// `content` is an array of strings — each string is a paragraph or a
/// bullet-point line (prefixed with `- `). The frontend renders them
/// directly, keeping the tone conversational.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WhatsNewRelease {
    pub version: String,
    pub date: String,
    pub content: Vec<String>,
}

/// Response returned by `get_whats_new`, combining the release entries
/// with whether the user has unseen updates.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WhatsNewResponse {
    pub releases: Vec<WhatsNewRelease>,
    /// True if the latest release version differs from the last version
    /// the user marked as seen.
    pub has_unseen: bool,
}

/// Read the bundled what's-new data and determine whether the user has
/// unseen entries based on their `whats_new_seen_version` setting.
pub fn get_whats_new() -> Result<WhatsNewResponse, String> {
    let raw = include_str!("../../whats-new.json");
    let releases: Vec<WhatsNewRelease> =
        serde_json::from_str(raw).map_err(|e| format!("failed to parse whats-new.json: {}", e))?;

    let settings = read_settings()?;
    let latest_version = releases.first().map(|r| r.version.as_str());
    let has_unseen = match (latest_version, &settings.whats_new_seen_version) {
        (Some(latest), Some(seen)) => latest != seen,
        (Some(_), None) => true,
        _ => false,
    };

    Ok(WhatsNewResponse {
        releases,
        has_unseen,
    })
}

/// Mark the current latest release as seen so the indicator badge
/// is dismissed until the next release ships new entries.
pub fn mark_whats_new_seen() -> Result<(), String> {
    let raw = include_str!("../../whats-new.json");
    let releases: Vec<WhatsNewRelease> =
        serde_json::from_str(raw).map_err(|e| format!("failed to parse whats-new.json: {}", e))?;

    if let Some(latest) = releases.first() {
        let mut settings = read_settings()?;
        if settings
            .whats_new_seen_version
            .as_deref()
            .map(|v| v != latest.version)
            .unwrap_or(true)
        {
            settings.whats_new_seen_version = Some(latest.version.clone());
            write_settings(&settings)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_whats_new_parses() {
        let raw = include_str!("../../whats-new.json");
        let releases: Vec<WhatsNewRelease> =
            serde_json::from_str(raw).expect("whats-new.json should be valid JSON");
        assert!(!releases.is_empty(), "at least one release entry required");

        let first = &releases[0];
        assert!(!first.version.is_empty());
        assert!(!first.date.is_empty());
        assert!(!first.content.is_empty());
    }
}
