//! Reading skin files from a `.wsz` (zip) archive or an unpacked directory.
//!
//! Files are looked up by lowercase base name, because skins mix cases (`MAIN.BMP`,
//! `Main.bmp`) and are often zipped inside a folder. When the same name appears twice, the
//! one closest to the archive root wins. Nothing is ever extracted to disk.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::Path;

use crate::SkinError;

/// Caps that protect against zip bombs and huge directories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_entries: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 2_000,
            max_file_bytes: 16 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
        }
    }
}

/// The files of one skin, keyed by lowercase base name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkinFiles {
    /// name -> (path depth, contents)
    files: BTreeMap<String, (usize, Vec<u8>)>,
}

impl SkinFiles {
    /// Reads every file from zip archive bytes, within `limits`.
    pub fn from_zip(bytes: &[u8], limits: Limits) -> Result<Self, SkinError> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
        if zip.len() > limits.max_entries {
            return Err(SkinError::TooLarge("too many archive entries"));
        }
        let mut out = Self::default();
        let mut total = 0u64;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            let Ok(name) = entry.name().map(|n| n.into_owned()) else {
                continue;
            };
            let mut data = Vec::new();
            // Read at most one byte past the limit: header sizes can lie.
            (&mut entry)
                .take(limits.max_file_bytes + 1)
                .read_to_end(&mut data)?;
            if data.len() as u64 > limits.max_file_bytes {
                return Err(SkinError::TooLarge("archive entry too large"));
            }
            total += data.len() as u64;
            if total > limits.max_total_bytes {
                return Err(SkinError::TooLarge("archive contents too large"));
            }
            out.add(&name, data);
        }
        Ok(out)
    }

    /// Reads an unpacked skin directory, up to three levels deep, within `limits`.
    pub fn from_dir(dir: &Path, limits: Limits) -> Result<Self, SkinError> {
        let mut out = Self::default();
        let mut total = 0u64;
        let mut count = 0usize;
        let mut stack = vec![(dir.to_path_buf(), String::new(), 0usize)];
        while let Some((path, prefix, depth)) = stack.pop() {
            for entry in std::fs::read_dir(&path)? {
                let entry = entry?;
                count += 1;
                if count > limits.max_entries {
                    return Err(SkinError::TooLarge("too many files in skin directory"));
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                let rel = format!("{prefix}{name}");
                let ft = entry.file_type()?;
                if ft.is_dir() {
                    if depth < 3 {
                        stack.push((entry.path(), format!("{rel}/"), depth + 1));
                    }
                } else if ft.is_file() {
                    let len = entry.metadata()?.len();
                    if len > limits.max_file_bytes {
                        return Err(SkinError::TooLarge("skin file too large"));
                    }
                    total += len;
                    if total > limits.max_total_bytes {
                        return Err(SkinError::TooLarge("skin directory too large"));
                    }
                    out.add(&rel, std::fs::read(entry.path())?);
                }
            }
        }
        Ok(out)
    }

    fn add(&mut self, path: &str, data: Vec<u8>) {
        let path = path.replace('\\', "/");
        // macOS archive metadata, not skin content.
        if path.split('/').any(|c| c == "__MACOSX") {
            return;
        }
        let Some(base) = path.rsplit('/').next().filter(|b| !b.is_empty()) else {
            return;
        };
        if base.starts_with("._") {
            return;
        }
        let depth = path.matches('/').count();
        let key = base.to_lowercase();
        match self.files.get(&key) {
            Some((d, _)) if *d <= depth => {}
            _ => {
                self.files.insert(key, (depth, data));
            }
        }
    }

    /// Contents of the file with this base name, ignoring case and folders.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.files
            .get(&name.to_lowercase())
            .map(|(_, d)| d.as_slice())
    }

    /// Lowercase base names of all files.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in entries {
            w.start_file(*name, SimpleFileOptions::default()).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn lookup_ignores_case_and_folders() {
        let z = zip_of(&[("MySkin/MAIN.BMP", b"a"), ("text.Bmp", b"b")]);
        let f = SkinFiles::from_zip(&z, Limits::default()).unwrap();
        assert_eq!(f.get("main.bmp"), Some(&b"a"[..]));
        assert_eq!(f.get("TEXT.BMP"), Some(&b"b"[..]));
        assert_eq!(f.get("nums_ex.bmp"), None);
    }

    #[test]
    fn shallowest_duplicate_wins() {
        let z = zip_of(&[
            ("deep/dir/main.bmp", b"deep"),
            ("main.bmp", b"top"),
            ("x/MAIN.bmp", b"mid"),
        ]);
        let f = SkinFiles::from_zip(&z, Limits::default()).unwrap();
        assert_eq!(f.get("main.bmp"), Some(&b"top"[..]));
    }

    #[test]
    fn mac_metadata_is_skipped() {
        let z = zip_of(&[
            ("__MACOSX/skin/._main.bmp", b"junk"),
            ("skin/._text.bmp", b"junk"),
        ]);
        let f = SkinFiles::from_zip(&z, Limits::default()).unwrap();
        assert!(f.is_empty());
    }

    #[test]
    fn backslash_paths_are_folders() {
        let z = zip_of(&[("skin\\PLEDIT.TXT", b"x")]);
        let f = SkinFiles::from_zip(&z, Limits::default()).unwrap();
        assert_eq!(f.get("pledit.txt"), Some(&b"x"[..]));
    }

    #[test]
    fn limits_are_enforced() {
        let big = vec![0u8; 2048];
        let z = zip_of(&[("a.bmp", &big), ("b.bmp", &big)]);
        let tight = Limits {
            max_file_bytes: 1024,
            ..Limits::default()
        };
        assert!(matches!(
            SkinFiles::from_zip(&z, tight),
            Err(SkinError::TooLarge(_))
        ));
        let total = Limits {
            max_total_bytes: 3000,
            ..Limits::default()
        };
        assert!(matches!(
            SkinFiles::from_zip(&z, total),
            Err(SkinError::TooLarge(_))
        ));
        let entries = Limits {
            max_entries: 1,
            ..Limits::default()
        };
        assert!(matches!(
            SkinFiles::from_zip(&z, entries),
            Err(SkinError::TooLarge(_))
        ));
    }

    #[test]
    fn not_a_zip_is_an_error() {
        assert!(matches!(
            SkinFiles::from_zip(b"definitely not a zip", Limits::default()),
            Err(SkinError::Zip(_))
        ));
    }

    #[test]
    fn directory_loading() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Inner")).unwrap();
        std::fs::write(dir.path().join("Inner/Main.BMP"), b"m").unwrap();
        std::fs::write(dir.path().join("viscolor.txt"), b"v").unwrap();
        let f = SkinFiles::from_dir(dir.path(), Limits::default()).unwrap();
        assert_eq!(f.get("main.bmp"), Some(&b"m"[..]));
        assert_eq!(f.get("VISCOLOR.TXT"), Some(&b"v"[..]));
    }
}
