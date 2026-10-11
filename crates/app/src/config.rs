//! The settings file (`config.toml` in the per-OS config folder). Missing fields take their
//! defaults, so older files keep working as settings are added.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub library: LibraryConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LibraryConfig {
    /// Scan the OS Music folder.
    pub include_music_folder: bool,
    /// Additional folders to scan.
    pub folders: Vec<PathBuf>,
}

impl Default for LibraryConfig {
    fn default() -> Self {
        Self {
            include_music_folder: true,
            folders: Vec::new(),
        }
    }
}

impl LibraryConfig {
    /// Folders to scan: the Music folder (when enabled) followed by the added ones, without
    /// duplicates.
    pub fn roots(&self, music: Option<&Path>) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = Vec::new();
        let music = music
            .filter(|_| self.include_music_folder)
            .map(Path::to_path_buf);
        for r in music.into_iter().chain(self.folders.iter().cloned()) {
            if !roots.contains(&r) {
                roots.push(r);
            }
        }
        roots
    }

    /// Adds a folder; returns false if it was already listed.
    pub fn add(&mut self, folder: PathBuf) -> bool {
        if self.folders.contains(&folder) {
            return false;
        }
        self.folders.push(folder);
        true
    }

    /// Removes a folder; returns false if it was not listed.
    pub fn remove(&mut self, folder: &Path) -> bool {
        let before = self.folders.len();
        self.folders.retain(|f| f != folder);
        self.folders.len() != before
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{} is not valid: {source}", path.display())]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("cannot write {}: {source}", path.display())]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot encode settings: {0}")]
    Encode(#[from] toml::ser::Error),
}

impl Config {
    /// Reads the file, or returns defaults when it does not exist yet. A file that exists but
    /// cannot be parsed is an error, so it is never silently replaced.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|source| ConfigError::Parse {
                path: path.to_path_buf(),
                source,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Read {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Writes the file atomically (temporary file, then rename), creating the folder.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let text = toml::to_string_pretty(self)?;
        let write_err = |source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(write_err)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).map_err(write_err)?;
        std::fs::rename(&tmp, path).map_err(write_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_defaults_and_save_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg/config.toml");
        let mut c = Config::load(&path).unwrap();
        assert_eq!(c, Config::default());
        assert!(c.library.add(PathBuf::from("/mnt/music")));
        assert!(!c.library.add(PathBuf::from("/mnt/music")));
        c.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), c);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("folders = [\"/mnt/music\"]"), "{text}");
    }

    #[test]
    fn partial_files_fill_defaults_and_bad_files_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[library]\nfolders = [\"/a\"]\n").unwrap();
        let c = Config::load(&path).unwrap();
        assert!(c.library.include_music_folder);
        assert_eq!(c.library.folders, vec![PathBuf::from("/a")]);
        std::fs::write(&path, "[library\nbroken").unwrap();
        assert!(matches!(
            Config::load(&path),
            Err(ConfigError::Parse { .. })
        ));
    }

    #[test]
    fn roots_combine_music_folder_and_added_folders() {
        let mut l = LibraryConfig::default();
        l.add(PathBuf::from("/drive"));
        l.add(PathBuf::from("/home/u/Music"));
        let music = Path::new("/home/u/Music");
        assert_eq!(
            l.roots(Some(music)),
            vec![PathBuf::from("/home/u/Music"), PathBuf::from("/drive")]
        );
        l.include_music_folder = false;
        assert_eq!(
            l.roots(Some(music)),
            vec![PathBuf::from("/drive"), PathBuf::from("/home/u/Music")]
        );
        assert!(l.remove(Path::new("/drive")));
        assert!(!l.remove(Path::new("/drive")));
    }
}
