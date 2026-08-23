//! Poll the `automatic-library` repository on GitHub for a newer release
//! than the library snapshot compiled into the current binary and, when
//! one is found, download, verify, and apply it.
//!
//! - `check_for_update` — compares the latest release tag against
//!   `bundled_library::version()`.
//! - `download_and_verify` — downloads the archive plus its `.minisig`,
//!   verifies the minisign signature against the public key baked into
//!   the binary at `src-tauri/keys/library.pub`, extracts the archive,
//!   and re-hashes every file listed in the archive's own `manifest.json`
//!   before returning.
//! - `apply` (see the `apply` module) writes the verified content into
//!   `<root>/library/…` and updates `settings.library_version`.

use std::io::{Cursor, Read};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::bundled_library;

/// The public key baked into the binary. Signed release archives are
/// verified against this key; a mismatch causes the whole refresh to
/// abort. Rotate the key by generating a new keypair per
/// `automatic-library/KEYGEN.md` and shipping a new app release with
/// the updated file.
pub const PUBLIC_KEY_PEM: &str = include_str!("../../keys/library.pub");

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
/// library currently installed. The baseline is the higher of the bundled
/// version and `settings.library_version`, so a previous refresh that
/// moved the installed library ahead of the binary snapshot is not
/// re-downloaded on subsequent polls. Returns `Ok(None)` when already
/// current, when GitHub reports no release yet, or when the tag is not
/// valid semver.
///
/// Errors describe transport failures (network, HTTP status, JSON parse).
pub async fn check_for_update() -> Result<Option<LibraryRelease>, String> {
    let baseline = effective_version()?;

    let latest = fetch_latest_release().await?;
    let Some(latest) = latest else {
        return Ok(None);
    };

    let candidate_semver = match semver::Version::parse(&latest.version) {
        Ok(v) => v,
        Err(_) => return Ok(None), // upstream tag is not semver; ignore
    };

    if candidate_semver > baseline {
        Ok(Some(latest))
    } else {
        Ok(None)
    }
}

/// The highest of the bundled library version and the version already
/// installed on disk. Without this, a successful refresh (which moves the
/// installed version ahead of the bundled snapshot) would be re-downloaded
/// on every subsequent poll because the comparison only saw the bundled
/// baseline.
fn effective_version() -> Result<semver::Version, String> {
    let bundled = bundled_library::version();
    let bundled_semver = semver::Version::parse(bundled).map_err(|e| {
        format!(
            "bundled library version '{}' is not valid semver: {}",
            bundled, e
        )
    })?;

    let installed = super::read_settings()
        .ok()
        .and_then(|s| s.library_version)
        .and_then(|v| semver::Version::parse(&v).ok());

    match installed {
        Some(inst) if inst > bundled_semver => Ok(inst),
        _ => Ok(bundled_semver),
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

/// Successfully-verified library archive, ready for `apply` to install.
///
/// Owns a `tempfile::TempDir` so extracted files are cleaned up
/// automatically when the value is dropped.
pub struct VerifiedLibrary {
    /// Temp directory containing the extracted archive tree.
    pub extracted: tempfile::TempDir,
    /// Parsed `manifest.json` from the extracted archive.
    pub manifest: bundled_library::RawManifest,
    /// Semver from the archive's own `VERSION` file (matches the tag
    /// with the leading `v` stripped).
    pub version: String,
}

impl VerifiedLibrary {
    /// Root of the extracted tree. Callers pass this to `apply` as the
    /// source of new content.
    pub fn root(&self) -> &std::path::Path {
        self.extracted.path()
    }
}

/// Download the release archive and its `.minisig`, verify the signature
/// against the pinned public key, extract to a temp directory, then
/// verify every file listed in the archive's manifest matches its
/// recorded sha256. Any failure aborts and leaves nothing installed.
///
/// The archive must contain a top-level `manifest.json` and `VERSION`.
/// Every file whose path appears in the manifest (skill entry file
/// lists, and rule/instruction/subagent/hook `path` fields) is hashed
/// and compared to the manifest's recorded value. Files present in the
/// archive but absent from the manifest are not checked — they may be
/// documentation like `README.md` or `LICENSE`.
pub async fn download_and_verify(
    release: &LibraryRelease,
) -> Result<VerifiedLibrary, String> {
    let archive_url = release
        .archive_url
        .as_ref()
        .ok_or_else(|| format!("release {} has no library archive attached", release.tag))?;
    let sig_url = release
        .signature_url
        .as_ref()
        .ok_or_else(|| format!("release {} has no .minisig attached", release.tag))?;

    let client = http_client()?;
    let archive_bytes = fetch_asset(&client, archive_url).await?;
    let sig_bytes = fetch_asset(&client, sig_url).await?;

    verify_signature(&archive_bytes, &sig_bytes)?;

    let extracted = tempfile::tempdir()
        .map_err(|e| format!("temp dir for extraction: {}", e))?;
    extract_zip(&archive_bytes, extracted.path())?;

    let manifest_path = extracted.path().join("manifest.json");
    let manifest_bytes = std::fs::read(&manifest_path).map_err(|e| {
        format!(
            "extracted archive missing manifest.json at {}: {}",
            manifest_path.display(),
            e
        )
    })?;
    let manifest: bundled_library::RawManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| format!("archive manifest.json failed to parse: {}", e))?;

    let version_path = extracted.path().join("VERSION");
    let version = std::fs::read_to_string(&version_path)
        .map_err(|e| format!("archive missing VERSION: {}", e))?
        .trim()
        .to_string();
    if version != manifest.library_version {
        return Err(format!(
            "archive VERSION ({}) disagrees with manifest.library_version ({})",
            version, manifest.library_version
        ));
    }
    if version != release.version {
        return Err(format!(
            "archive VERSION ({}) disagrees with release tag version ({})",
            version, release.version
        ));
    }

    verify_manifest_hashes(&manifest, extracted.path())?;

    Ok(VerifiedLibrary {
        extracted,
        manifest,
        version,
    })
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!(
            "automatic-app/",
            env!("CARGO_PKG_VERSION"),
            " (library-refresh)"
        ))
        .build()
        .map_err(|e| format!("reqwest client: {}", e))
}

async fn fetch_asset(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("GET {}: {}", url, e))?;
    if !response.status().is_success() {
        return Err(format!(
            "GET {} returned HTTP {}",
            url,
            response.status()
        ));
    }
    Ok(response
        .bytes()
        .await
        .map_err(|e| format!("body of {}: {}", url, e))?
        .to_vec())
}

fn verify_signature(archive: &[u8], sig: &[u8]) -> Result<(), String> {
    let pk = minisign_verify::PublicKey::decode(PUBLIC_KEY_PEM.trim())
        .map_err(|e| format!("bundled library public key is malformed: {}", e))?;
    let sig_str = std::str::from_utf8(sig)
        .map_err(|e| format!(".minisig is not UTF-8: {}", e))?;
    let signature = minisign_verify::Signature::decode(sig_str)
        .map_err(|e| format!(".minisig failed to parse: {}", e))?;
    pk.verify(archive, &signature, false)
        .map_err(|e| format!("archive signature verification failed: {}", e))
}

fn extract_zip(bytes: &[u8], out: &std::path::Path) -> Result<(), String> {
    let cursor = Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(cursor)
        .map_err(|e| format!("archive is not a valid zip: {}", e))?;
    for i in 0..zip.len() {
        let mut file = zip
            .by_index(i)
            .map_err(|e| format!("zip entry {} unreadable: {}", i, e))?;
        // Guard against zip-slip: refuse any entry whose enclosed_name
        // does not stay under the output directory. `enclosed_name`
        // rejects absolute paths and `..` components.
        let Some(rel) = file.enclosed_name() else {
            return Err(format!("zip entry {} has an unsafe path", file.name()));
        };
        let dest = out.join(rel);
        if file.is_dir() {
            std::fs::create_dir_all(&dest)
                .map_err(|e| format!("mkdir {}: {}", dest.display(), e))?;
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {}: {}", parent.display(), e))?;
        }
        let mut buf = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut buf)
            .map_err(|e| format!("read zip entry {}: {}", file.name(), e))?;
        std::fs::write(&dest, &buf)
            .map_err(|e| format!("write {}: {}", dest.display(), e))?;
    }
    Ok(())
}

fn verify_manifest_hashes(
    manifest: &bundled_library::RawManifest,
    root: &std::path::Path,
) -> Result<(), String> {
    let mut checked: usize = 0;
    for asset in &manifest.assets {
        if let (Some(path), Some(expected)) = (&asset.path, &asset.sha256) {
            hash_check(root, path, expected)?;
            checked += 1;
        }
        if let Some(files) = &asset.files {
            for file in files {
                hash_check(root, &file.path, &file.sha256)?;
                checked += 1;
            }
        }
    }
    if checked == 0 {
        return Err("manifest declared no files to verify".to_string());
    }
    Ok(())
}

fn hash_check(root: &std::path::Path, rel: &str, expected: &str) -> Result<(), String> {
    let path = root.join(rel);
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("archive missing declared file {}: {}", rel, e))?;
    let actual = hex_sha256(&bytes);
    if actual != expected {
        return Err(format!(
            "hash mismatch for {}: expected {}, got {}",
            rel, expected, actual
        ));
    }
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    let hash = Sha256::digest(bytes);
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Install a verified library into `<root>/library/…` and record the
/// new version in `settings.json`.
///
/// Semantics mirror `install_default_*_inner(force: true)`: every
/// library-provided file is overwritten with the new content. App-only
/// content (`bundled_app_skills`, `APP_BUNDLED_RULES` — for example
/// `automatic-service`) is not touched here; the bundled installers
/// on next boot handle it.
///
/// Retired rules named in the archive's `retired.json` are removed via
/// the same cleanup path the boot-time migration uses.
///
/// The `resync_all_projects()` sweep after apply mirrors what
/// `bootstrap` runs after a force-reinstall, so drift picks up
/// immediately instead of waiting for the next launch.
pub fn apply(verified: &VerifiedLibrary) -> Result<(), String> {
    let library_root = verified.root();

    install_skills(&verified.manifest, library_root)?;
    install_rules(&verified.manifest, library_root)?;
    install_instructions(&verified.manifest, library_root)?;
    install_subagents(&verified.manifest, library_root)?;
    apply_retirements(library_root)?;

    persist_library_version(&verified.version)?;

    Ok(())
}

fn install_skills(
    manifest: &bundled_library::RawManifest,
    library_root: &std::path::Path,
) -> Result<(), String> {
    let skills_dir = super::paths::get_library_skills_dir()?;
    for asset in manifest.assets.iter().filter(|a| a.kind == "skill") {
        let root = asset
            .root
            .as_deref()
            .ok_or_else(|| format!("skill {} missing root", asset.id))?;
        let dest_dir = skills_dir.join(&asset.id);
        // Remove any stale copy then re-create so files that were
        // renamed or dropped in the new library don't linger.
        if dest_dir.exists() {
            std::fs::remove_dir_all(&dest_dir)
                .map_err(|e| format!("clear {}: {}", dest_dir.display(), e))?;
        }
        std::fs::create_dir_all(&dest_dir)
            .map_err(|e| format!("mkdir {}: {}", dest_dir.display(), e))?;

        if let Some(files) = &asset.files {
            for file in files {
                let src = library_root.join(&file.path);
                let rel = file
                    .path
                    .strip_prefix(root)
                    .unwrap_or(&file.path)
                    .trim_start_matches('/');
                let dest = dest_dir.join(rel);
                copy_one_file(&src, &dest)?;

                // Re-scan the file that will be the skill's entry
                // point. Signed content is authoritative, but the
                // security scan is defence in depth against a
                // maintainer accidentally shipping a rule that trips
                // one of the injection heuristics.
                if rel.eq_ignore_ascii_case("SKILL.md") {
                    let text = std::fs::read_to_string(&dest)
                        .map_err(|e| format!("read {}: {}", dest.display(), e))?;
                    let scan = super::asset_security::scan_text_asset_report(
                        super::asset_security::AssetKind::Skill,
                        &text,
                    );
                    if scan.blocked() {
                        return Err(scan.to_display_message(&format!(
                            "refreshed library skill '{}'",
                            asset.id
                        )));
                    }
                    let _ =
                        super::skills::record_skill_scan_state(&asset.id, &scan.to_record());
                    let id = format!("automatic/automatic-app/{}", asset.id);
                    let _ = super::skill_store::record_skill_source(
                        &asset.id,
                        "automatic/automatic-app",
                        &id,
                        "bundled",
                    );
                }
            }
        }
    }
    Ok(())
}

fn install_rules(
    manifest: &bundled_library::RawManifest,
    library_root: &std::path::Path,
) -> Result<(), String> {
    let rules_dir = super::rules::get_rules_dir()?;
    if !rules_dir.exists() {
        std::fs::create_dir_all(&rules_dir).map_err(|e| e.to_string())?;
    }
    for asset in manifest.assets.iter().filter(|a| a.kind == "rule") {
        let pack = asset
            .pack
            .as_deref()
            .ok_or_else(|| format!("rule {} missing pack", asset.id))?;
        let path = asset
            .path
            .as_deref()
            .ok_or_else(|| format!("rule {} missing path", asset.id))?;

        let machine_name = super::rules::library_rule_machine_name(pack, &asset.id);
        let display_name =
            super::rules::library_rule_display_name(&machine_name, pack, &asset.id);
        let content = std::fs::read_to_string(library_root.join(path))
            .map_err(|e| format!("read rule {}: {}", path, e))?;

        // Defence in depth (see install_skills comment).
        super::asset_security::enforce_text_asset(
            super::asset_security::AssetKind::Rule,
            &format!("refreshed library rule '{}'", machine_name),
            &content,
        )?;

        let json = super::rules::serialise_bundled_rule(&display_name, &content)?;
        let dest = rules_dir.join(format!("{}.json", machine_name));
        std::fs::write(&dest, json).map_err(|e| format!("write {}: {}", dest.display(), e))?;
    }
    Ok(())
}

fn install_instructions(
    manifest: &bundled_library::RawManifest,
    library_root: &std::path::Path,
) -> Result<(), String> {
    let dir = super::instructions::get_instructions_dir()?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    for asset in manifest.assets.iter().filter(|a| a.kind == "instruction") {
        let path = asset
            .path
            .as_deref()
            .ok_or_else(|| format!("instruction {} missing path", asset.id))?;
        let content = std::fs::read_to_string(library_root.join(path))
            .map_err(|e| format!("read instruction {}: {}", path, e))?;
        super::asset_security::enforce_text_asset(
            super::asset_security::AssetKind::Template,
            &format!("refreshed library instruction '{}'", asset.id),
            &content,
        )?;
        let dest = dir.join(format!("{}.md", asset.id));
        std::fs::write(&dest, content)
            .map_err(|e| format!("write {}: {}", dest.display(), e))?;
    }
    Ok(())
}

fn install_subagents(
    manifest: &bundled_library::RawManifest,
    library_root: &std::path::Path,
) -> Result<(), String> {
    let dir = super::subagents::get_subagents_dir()?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    for asset in manifest.assets.iter().filter(|a| a.kind == "subagent") {
        let pack = asset
            .pack
            .as_deref()
            .ok_or_else(|| format!("subagent {} missing pack", asset.id))?;
        let path = asset
            .path
            .as_deref()
            .ok_or_else(|| format!("subagent {} missing path", asset.id))?;
        let machine_name = format!("{}-{}", pack, asset.id);
        let content = std::fs::read_to_string(library_root.join(path))
            .map_err(|e| format!("read subagent {}: {}", path, e))?;
        super::asset_security::enforce_text_asset(
            super::asset_security::AssetKind::UserAgent,
            &format!("refreshed library subagent '{}'", machine_name),
            &content,
        )?;
        let dest = dir.join(format!("{}.md", machine_name));
        std::fs::write(&dest, content)
            .map_err(|e| format!("write {}: {}", dest.display(), e))?;
    }
    Ok(())
}

fn apply_retirements(library_root: &std::path::Path) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct RetiredFile {
        retired: Vec<RetiredEntry>,
    }
    #[derive(serde::Deserialize)]
    struct RetiredEntry {
        kind: String,
        pack: Option<String>,
        id: String,
    }

    let path = library_root.join("retired.json");
    if !path.exists() {
        return Ok(());
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("read {}: {}", path.display(), e))?;
    let parsed: RetiredFile = serde_json::from_str(&raw)
        .map_err(|e| format!("parse {}: {}", path.display(), e))?;
    let rule_names: Vec<String> = parsed
        .retired
        .iter()
        .filter(|e| e.kind == "rule")
        .map(|e| match &e.pack {
            Some(p) => format!("{}-{}", p, e.id),
            None => e.id.clone(),
        })
        .collect();
    if !rule_names.is_empty() {
        super::rules::remove_retired_rules(&rule_names)?;
    }
    Ok(())
}

fn persist_library_version(version: &str) -> Result<(), String> {
    let mut settings = super::read_settings()?;
    settings.library_version = Some(version.to_string());
    super::write_settings(&settings)
}

fn copy_one_file(src: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {}: {}", parent.display(), e))?;
        }
    }
    std::fs::copy(src, dest)
        .map(|_| ())
        .map_err(|e| format!("copy {} → {}: {}", src.display(), dest.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Deterministic tests for the parts that don't touch the network.
    // A network-hitting integration test for check_for_update is
    // deferred to Phase 3b so CI stays offline.

    #[test]
    fn pinned_public_key_parses() {
        let pk = minisign_verify::PublicKey::decode(PUBLIC_KEY_PEM.trim())
            .expect("bundled library public key should parse");
        // Sanity: keyid + comment shape is present. `decode` succeeding
        // already proves the base64 body is well-formed, so nothing
        // stronger to assert without also having a signed artefact.
        let _ = pk;
    }

    /// Full end-to-end: hits GitHub Releases, downloads the archive
    /// and .minisig, verifies the signature against the pinned public
    /// key, extracts, and rehashes every manifest-listed file.
    /// #[ignore] because it hits the network. Run manually with
    /// `cargo test -- --ignored end_to_end_download_and_verify`.
    #[tokio::test]
    #[ignore]
    async fn end_to_end_download_and_verify() {
        let release = check_for_update()
            .await
            .expect("check_for_update should succeed")
            .expect(
                "expected a newer release than bundled_library version; \
                 bump the library or run with a bundled version older than \
                 the latest release",
            );
        let verified = download_and_verify(&release)
            .await
            .expect("download_and_verify should succeed against a real release");
        assert_eq!(verified.version, release.version);
        assert!(!verified.manifest.assets.is_empty());
    }

    /// Full pipeline against a real release: check → download →
    /// verify → apply → confirm library_version updated on disk.
    /// #[ignore] because it hits the network.
    #[tokio::test]
    #[ignore]
    async fn end_to_end_apply_persists_new_version() {
        use crate::core::paths::with_test_home;

        let temp = tempfile::tempdir().expect("tempdir");
        let temp_path = temp.path().to_path_buf();

        let release = check_for_update()
            .await
            .expect("check should succeed")
            .expect("expected a newer release");
        let verified = download_and_verify(&release)
            .await
            .expect("download should succeed");
        let version = verified.version.clone();

        // apply and settings I/O go through get_automatic_dir(), which
        // with_test_home overrides. Run apply on a blocking thread
        // scope to match the production path, and confirm the version
        // marker landed.
        let applied_version = tokio::task::spawn_blocking(move || {
            with_test_home(temp_path, || -> Result<String, String> {
                apply(&verified)?;
                let s = crate::core::read_settings()?;
                s.library_version.ok_or_else(|| {
                    "library_version was not persisted".to_string()
                })
            })
        })
        .await
        .expect("spawn_blocking")
        .expect("apply chain");
        assert_eq!(applied_version, version);
    }

    #[test]
    fn effective_version_uses_installed_when_ahead_of_bundled() {
        use crate::core::paths::with_test_home;

        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            // Seed a settings.json with library_version ahead of bundled.
            let bundled = bundled_library::version();
            let ahead = {
                let mut v = semver::Version::parse(bundled).unwrap();
                v.minor += 10;
                v.to_string()
            };
            let mut settings = crate::core::Settings::default();
            settings.library_version = Some(ahead.clone());
            crate::core::write_settings(&settings).expect("write settings");

            let eff = effective_version().expect("effective_version");
            assert_eq!(
                eff.to_string(),
                ahead,
                "should return installed version when it is ahead of bundled"
            );
        });
    }

    #[test]
    fn effective_version_falls_back_to_bundled_when_no_installed() {
        use crate::core::paths::with_test_home;

        let temp = tempfile::tempdir().expect("tempdir");
        with_test_home(temp.path().to_path_buf(), || {
            let eff = effective_version().expect("effective_version");
            let bundled = semver::Version::parse(bundled_library::version()).unwrap();
            assert_eq!(
                eff, bundled,
                "should return bundled version when no settings exist"
            );
        });
    }

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
