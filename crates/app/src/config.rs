//! The settings file (`config.toml` in the per-OS config folder). Missing fields take their
//! defaults, so older files keep working as settings are added.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub player: PlayerConfig,
    pub equalizer: EqConfig,
    pub windows: WindowsConfig,
    pub library: LibraryConfig,
}

/// Player state restored at the next start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerConfig {
    /// 0.0 to 1.0.
    pub volume: f32,
    /// -1.0 (left) to 1.0 (right).
    pub balance: f32,
    pub shuffle: bool,
    pub repeat: bool,
    /// UI scale, 1 to 3. Unset picks one from the screen at startup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<u8>,
    /// Skin file or folder.
    pub skin: Option<PathBuf>,
    /// Time display shows remaining time.
    pub show_remaining: bool,
    /// Windows stay above other programs.
    pub always_on_top: bool,
    /// "spectrum", "oscilloscope" or "off".
    pub visualizer: String,
    /// Playlist position (0-based) of the track Play starts.
    pub current_track: Option<usize>,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            volume: 0.5,
            balance: 0.0,
            shuffle: false,
            repeat: false,
            scale: None,
            skin: None,
            show_remaining: false,
            always_on_top: false,
            visualizer: "spectrum".into(),
            current_track: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EqConfig {
    pub enabled: bool,
    pub auto: bool,
    pub preamp_db: f32,
    pub bands_db: [f32; 10],
}

/// Where a window was and whether it was shown. Sizes are unscaled (resizable windows only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub visible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    /// Collapsed to the window-shade bar.
    #[serde(default)]
    pub shaded: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowsConfig {
    pub main: Option<WindowState>,
    pub equalizer: Option<WindowState>,
    pub playlist: Option<WindowState>,
    pub library: Option<WindowState>,
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
    fn full_state_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut c = Config::default();
        c.player.volume = 0.8;
        c.player.skin = Some("/skins/a.wsz".into());
        c.player.current_track = Some(3);
        c.equalizer.enabled = true;
        c.equalizer.bands_db[2] = 6.0;
        c.windows.playlist = Some(WindowState {
            x: 100,
            y: 332,
            visible: true,
            width: Some(325),
            height: Some(290),
            shaded: true,
        });
        c.windows.main = Some(WindowState {
            x: 100,
            y: 100,
            visible: true,
            width: None,
            height: None,
            shaded: false,
        });
        c.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), c);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains("width = ") || text.contains("[windows.playlist]"),
            "{text}"
        );
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
