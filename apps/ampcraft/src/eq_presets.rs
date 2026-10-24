//! The user's equalizer presets and auto-load presets (per file name, applied when AUTO is
//! on), kept in `eq-presets.toml` in the config folder.

use std::collections::BTreeMap;
use std::path::Path;

use ampcraft_dsp::EqPreset;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Stored {
    preamp_db: f32,
    bands_db: [f32; 10],
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetStore {
    /// Saved presets by name.
    presets: BTreeMap<String, Stored>,
    /// Auto-load presets by file name (as the classic player keys them).
    auto: BTreeMap<String, Stored>,
}

fn to_preset(name: &str, s: &Stored) -> EqPreset {
    EqPreset {
        name: name.to_owned(),
        preamp_db: s.preamp_db.clamp(-12.0, 12.0),
        bands_db: s.bands_db.map(|b| b.clamp(-12.0, 12.0)),
    }
}

/// The key auto-load presets use: the file name without folders.
pub fn auto_key(path: &Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

impl PresetStore {
    /// Reads the store; a missing or unreadable file gives an empty one.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                log::warn!("ignoring {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }

    /// Saved presets, by name.
    pub fn presets(&self) -> Vec<EqPreset> {
        self.presets.iter().map(|(n, s)| to_preset(n, s)).collect()
    }

    /// Saves (or replaces) a preset under its name.
    pub fn put(&mut self, p: &EqPreset) {
        self.presets.insert(
            p.name.clone(),
            Stored {
                preamp_db: p.preamp_db,
                bands_db: p.bands_db,
            },
        );
    }

    pub fn remove(&mut self, name: &str) -> bool {
        self.presets.remove(name).is_some()
    }

    pub fn auto_for(&self, path: &Path) -> Option<EqPreset> {
        let key = auto_key(path)?;
        self.auto.get(&key).map(|s| to_preset(&key, s))
    }

    pub fn set_auto(&mut self, path: &Path, preamp_db: f32, bands_db: [f32; 10]) -> bool {
        let Some(key) = auto_key(path) else {
            return false;
        };
        self.auto.insert(
            key,
            Stored {
                preamp_db,
                bands_db,
            },
        );
        true
    }

    pub fn remove_auto(&mut self, path: &Path) -> bool {
        auto_key(path).is_some_and(|k| self.auto.remove(&k).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(name: &str, first: f32) -> EqPreset {
        let mut bands_db = [0.0; 10];
        bands_db[0] = first;
        EqPreset {
            name: name.into(),
            preamp_db: -2.0,
            bands_db,
        }
    }

    #[test]
    fn presets_and_auto_presets_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cfg/eq-presets.toml");
        let mut s = PresetStore::load(&file);
        assert!(s.presets().is_empty());
        s.put(&preset("Mine", 3.0));
        s.put(&preset("Another", 1.0));
        s.put(&preset("Mine", 6.0));
        assert!(s.set_auto(Path::new("/music/a/01 song.flac"), -1.0, [2.0; 10]));
        s.save(&file).unwrap();

        let back = PresetStore::load(&file);
        let names: Vec<String> = back.presets().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["Another", "Mine"]);
        assert_eq!(back.presets()[1].bands_db[0], 6.0, "replaced");
        // Auto presets follow the file name wherever the file is.
        let auto = back.auto_for(Path::new("/elsewhere/01 song.flac")).unwrap();
        assert_eq!((auto.preamp_db, auto.bands_db), (-1.0, [2.0; 10]));
        assert!(back.auto_for(Path::new("/m/other.flac")).is_none());

        let mut s = back;
        assert!(s.remove("Mine"));
        assert!(!s.remove("Mine"));
        assert!(s.remove_auto(Path::new("01 song.flac")));
    }

    #[test]
    fn broken_files_and_wild_values_are_safe() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("eq-presets.toml");
        std::fs::write(&file, "not [valid").unwrap();
        assert_eq!(PresetStore::load(&file), PresetStore::default());
        std::fs::write(
            &file,
            "[presets.Loud]\npreamp_db = 40.0\nbands_db = [99.0, 0, 0, 0, 0, 0, 0, 0, 0, -99.0]\n",
        )
        .unwrap();
        let p = PresetStore::load(&file).presets().remove(0);
        assert_eq!(p.preamp_db, 12.0);
        assert_eq!((p.bands_db[0], p.bands_db[9]), (12.0, -12.0));
    }
}
