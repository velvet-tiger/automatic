//! Reads the third-party licence files that release builds ship in the app's
//! `licenses` resource folder, for the Open-source licences dialog.
//!
//! The release workflow puts the files there through
//! `tauri.release.conf.json`. Development builds have no such folder, and the
//! dialog says so. Only the fixed file names below are read, so nothing the
//! caller passes can choose a path.

use serde::Serialize;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

/// The folder inside the app's resource directory that holds the files.
pub const LICENCES_DIR: &str = "licenses";

/// The files a release bundle holds, in the order the dialog shows them.
const LICENCE_FILES: &[&str] = &["rust-crates.txt", "interface.txt", "THIRD_PARTY_NOTICES.md"];

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
/// A missing `dir` means a build without licence files, such as a
/// development build, and is not an error. A missing file is skipped. A file
/// that exists but cannot be read is an error.
pub fn read_third_party_licences(dir: &Path) -> Result<ThirdPartyLicences, String> {
    if !dir.is_dir() {
        return Ok(ThirdPartyLicences {
            folder: None,
            files: Vec::new(),
        });
    }

    let mut files = Vec::new();
    for name in LICENCE_FILES {
        let path = dir.join(name);
        match fs::read_to_string(&path) {
            Ok(text) => files.push(LicenceFile {
                file_name: name.to_string(),
                text,
            }),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => {
                return Err(format!(
                    "could not read licence file '{}': {}",
                    path.display(),
                    e
                ))
            }
        }
    }
    Ok(ThirdPartyLicences {
        folder: Some(dir.display().to_string()),
        files,
    })
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
        fs::write(dir.join("unrelated.txt"), "ignored").unwrap();

        let result = read_third_party_licences(&dir).unwrap();
        assert_eq!(result.folder, Some(dir.display().to_string()));
        let names: Vec<&str> = result.files.iter().map(|f| f.file_name.as_str()).collect();
        assert_eq!(names, ["rust-crates.txt", "THIRD_PARTY_NOTICES.md"]);
        assert_eq!(result.files[0].text, "crates");
    }

    #[test]
    fn a_file_that_cannot_be_read_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("interface.txt"), [0xff, 0xfe, 0x00]).unwrap();
        let err = read_third_party_licences(tmp.path()).unwrap_err();
        assert!(err.contains("could not read licence file"), "{err}");
    }
}
