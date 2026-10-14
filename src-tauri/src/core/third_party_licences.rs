//! Reads the third-party licence files that release builds ship in the app's
//! `licenses` resource folder, for the Open-source licences dialog.
//!
//! The release workflow puts the files there through
//! `tauri.release.conf.json`. Development builds have no such folder, and the
//! dialog says so. Only the fixed file names below are read, so nothing the
//! caller passes can choose a path.

use serde::Serialize;
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::Path;

/// The folder inside the app's resource directory that holds the files.
pub const LICENCES_DIR: &str = "licenses";

/// The files a release bundle may hold, in the order the dialog shows them.
/// `system-libraries.txt` is only in the Linux AppImage.
const LICENCE_FILES: &[&str] = &[
    "rust-crates.txt",
    "interface.txt",
    "system-libraries.txt",
    "THIRD_PARTY_NOTICES.md",
];

/// The largest file read. The real files are well under 2 MB. The limit
/// keeps a damaged bundle from making the dialog read without end.
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;

/// One licence file and its text.
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct LicenceFile {
    pub file_name: String,
    pub text: String,
}

/// The licence files found in a build.
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct ThirdPartyLicences {
    /// The folder that holds the files, or `None` when this build has none.
    pub folder: Option<String>,
    /// The files found, in display order. Empty when `folder` is `None`.
    pub files: Vec<LicenceFile>,
}

/// Reads the known licence files from `dir`.
///
/// A missing `dir` is not an error: development builds have none. A known
/// file that is missing is skipped, since `system-libraries.txt` exists only
/// on Linux. A `dir` that is a symlink, or a known name that is a symlink, a
/// folder, too large or not UTF-8, is an error, because a release bundle
/// never holds one.
pub fn read_third_party_licences(dir: &Path) -> Result<ThirdPartyLicences, String> {
    match fs::symlink_metadata(dir) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => {
            return Err(format!(
                "licence folder '{}' is not a folder",
                dir.display()
            ))
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Ok(ThirdPartyLicences {
                folder: None,
                files: Vec::new(),
            })
        }
        Err(e) => {
            return Err(format!(
                "could not read licence folder '{}': {}",
                dir.display(),
                e
            ))
        }
    }

    let mut files = Vec::new();
    for name in LICENCE_FILES {
        if let Some(file) = read_licence_file(dir, name)? {
            files.push(file);
        }
    }
    Ok(ThirdPartyLicences {
        folder: Some(dir.display().to_string()),
        files,
    })
}

fn read_licence_file(dir: &Path, name: &str) -> Result<Option<LicenceFile>, String> {
    let path = dir.join(name);
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(format!(
                "could not read licence file '{}': {}",
                path.display(),
                e
            ))
        }
    };
    if !meta.is_file() {
        return Err(format!(
            "licence file '{}' is not a regular file",
            path.display()
        ));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "licence file '{}' is {} bytes, more than the {} byte limit",
            path.display(),
            meta.len(),
            MAX_FILE_BYTES
        ));
    }
    // The limit is applied to the read as well, in case the file grows
    // after the check above.
    let file = fs::File::open(&path)
        .map_err(|e| format!("could not read licence file '{}': {}", path.display(), e))?;
    let mut text = String::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("could not read licence file '{}': {}", path.display(), e))?;
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(format!(
            "licence file '{}' is more than the {} byte limit",
            path.display(),
            MAX_FILE_BYTES
        ));
    }
    Ok(Some(LicenceFile {
        file_name: name.to_string(),
        text,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_folder_means_no_licences() {
        let tmp = tempfile::tempdir().unwrap();
        let result = read_third_party_licences(&tmp.path().join("licenses")).unwrap();
        assert_eq!(
            result,
            ThirdPartyLicences {
                folder: None,
                files: Vec::new()
            }
        );
    }

    #[test]
    fn reads_known_files_in_display_order_and_ignores_others() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("licenses");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("THIRD_PARTY_NOTICES.md"), "notices").unwrap();
        fs::write(dir.join("rust-crates.txt"), "crates").unwrap();
        fs::write(dir.join("interface.txt"), "npm").unwrap();
        fs::write(dir.join("unrelated.txt"), "ignored").unwrap();

        let result = read_third_party_licences(&dir).unwrap();
        assert_eq!(result.folder, Some(dir.display().to_string()));
        let names: Vec<&str> = result.files.iter().map(|f| f.file_name.as_str()).collect();
        assert_eq!(
            names,
            ["rust-crates.txt", "interface.txt", "THIRD_PARTY_NOTICES.md"]
        );
        assert_eq!(result.files[0].text, "crates");
    }

    #[test]
    fn empty_folder_returns_the_folder_and_no_files() {
        let tmp = tempfile::tempdir().unwrap();
        let result = read_third_party_licences(tmp.path()).unwrap();
        assert_eq!(result.folder, Some(tmp.path().display().to_string()));
        assert!(result.files.is_empty());
    }

    #[test]
    fn a_file_where_the_folder_should_be_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("licenses");
        fs::write(&path, "not a folder").unwrap();
        let err = read_third_party_licences(&path).unwrap_err();
        assert!(err.contains("is not a folder"), "{err}");
    }

    #[test]
    fn a_folder_with_a_known_name_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("rust-crates.txt")).unwrap();
        let err = read_third_party_licences(tmp.path()).unwrap_err();
        assert!(err.contains("not a regular file"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_with_a_known_name_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().join("outside.txt");
        fs::write(&outside, "secret").unwrap();
        let dir = tmp.path().join("licenses");
        fs::create_dir(&dir).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("interface.txt")).unwrap();
        let err = read_third_party_licences(&dir).unwrap_err();
        assert!(err.contains("not a regular file"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_folder_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("elsewhere");
        fs::create_dir(&real).unwrap();
        fs::write(real.join("rust-crates.txt"), "crates").unwrap();
        let link = tmp.path().join("licenses");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let err = read_third_party_licences(&link).unwrap_err();
        assert!(err.contains("is not a folder"), "{err}");
    }

    #[test]
    fn text_that_is_not_utf8_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("interface.txt"), [0xff, 0xfe, 0x00]).unwrap();
        let err = read_third_party_licences(tmp.path()).unwrap_err();
        assert!(err.contains("could not read licence file"), "{err}");
    }
}
