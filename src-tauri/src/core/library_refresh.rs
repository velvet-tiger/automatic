//! Poll the `automatic-library` repository on GitHub for a newer release
//! than the library snapshot compiled into the current binary.
//!
//! Phase 3a only implements the query side: `check_for_update` compares
//! the latest release tag against `bundled_library::version()` and returns
//! metadata for the caller when the release is newer. Download,
//! signature verification, and apply live in `library_refresh_apply`
//! (Phase 3b, blocked on Phase 4 producing signed release artefacts).

use serde::{Deserialize, Serialize};

use super::bundled_library;

const RELEASES_API: &str =
    "https://api.github.com/repos/velvet-tiger/automatic-library/releases/latest";

/// Metadata for a candidate library release newer than the one bundled
/// with the running app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryRelease {
    /// Semver from the tag (leading `v` stripped).
    pub version: String,
    /// Tag as GitHub returned it (`v0.1.1`).
    pub tag: String,
    /// ISO 8601 publish timestamp.
    pub published_at: String,
    /// URL for `library-vX.Y.Z.zip`. `None` when the release did not
    /// attach the expected archive.
    pub archive_url: Option<String>,
    /// URL for `library-vX.Y.Z.zip.minisig`. `None` when the release did
    /// not attach the expected signature.
    pub signature_url: Option<String>,
}

/// Return the newest release that is strictly newer (by semver) than the
/// library bundled with the current binary. Returns `Ok(None)` when the
/// bundled version is already current, when GitHub reports no release
/// yet, or when the tag is not a valid semver.
///
/// Errors describe transport failures (network, HTTP status, JSON parse).
pub async fn check_for_update() -> Result<Option<LibraryRelease>, String> {
    let bundled = bundled_library::version();
    let bundled_semver = semver::Version::parse(bundled).map_err(|e| {
        format!(
            "bundled library version '{}' is not valid semver: {}",
            bundled, e
        )
    })?;

    let latest = fetch_latest_release().await?;
    let Some(latest) = latest else {
        return Ok(None);
    };

    let candidate_semver = match semver::Version::parse(&latest.version) {
        Ok(v) => v,
        Err(_) => return Ok(None), // upstream tag is not semver; ignore
    };

    if candidate_semver > bundled_semver {
        Ok(Some(latest))
    } else {
        Ok(None)
    }
}

/// GitHub Releases API shape (subset of fields we care about).
#[derive(Debug, Deserialize)]
struct RawRelease {
    tag_name: String,
    published_at: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

#[derive(Debug, Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
}

async fn fetch_latest_release() -> Result<Option<LibraryRelease>, String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "automatic-app/",
            env!("CARGO_PKG_VERSION"),
            " (library-refresh)"
        ))
        .build()
        .map_err(|e| format!("reqwest client: {}", e))?;

    let response = client
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("GitHub releases fetch: {}", e))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        // No releases published yet.
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!(
            "GitHub releases returned HTTP {}",
            response.status()
        ));
    }

    let raw: RawRelease = response
        .json()
        .await
        .map_err(|e| format!("GitHub releases JSON parse: {}", e))?;

    if raw.draft || raw.prerelease {
        return Ok(None);
    }

    let version = raw.tag_name.trim_start_matches('v').to_string();
    let archive_name = format!("library-{}.zip", raw.tag_name);
    let signature_name = format!("{}.minisig", archive_name);

    let archive_url = raw
        .assets
        .iter()
        .find(|a| a.name == archive_name)
        .map(|a| a.browser_download_url.clone());
    let signature_url = raw
        .assets
        .iter()
        .find(|a| a.name == signature_name)
        .map(|a| a.browser_download_url.clone());

    Ok(Some(LibraryRelease {
        version,
        tag: raw.tag_name,
        published_at: raw.published_at,
        archive_url,
        signature_url,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Deterministic tests for the parts that don't touch the network.
    // A network-hitting integration test for check_for_update is
    // deferred to Phase 3b so CI stays offline.

    #[test]
    fn library_release_round_trips_serde() {
        let release = LibraryRelease {
            version: "0.1.1".to_string(),
            tag: "v0.1.1".to_string(),
            published_at: "2026-08-24T00:00:00Z".to_string(),
            archive_url: Some("https://example.invalid/library-v0.1.1.zip".to_string()),
            signature_url: Some("https://example.invalid/library-v0.1.1.zip.minisig".to_string()),
        };
        let json = serde_json::to_string(&release).unwrap();
        let round: LibraryRelease = serde_json::from_str(&json).unwrap();
        assert_eq!(round.version, "0.1.1");
        assert_eq!(round.archive_url.as_deref(), Some("https://example.invalid/library-v0.1.1.zip"));
    }
}
