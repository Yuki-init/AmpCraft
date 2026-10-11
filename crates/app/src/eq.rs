//! Equalizer presets in the player binary: the PRESETS button's menu (load, save, delete,
//! reset), auto-load presets for the AUTO button, and .eqf import and export.

use super::*;
use amp_dsp::EqPreset;
use amp_ui::MenuEntry;

use crate::menus::MenuCmd;

/// What the preset name prompt is for.
pub(crate) const PRESET_NAME_PROMPT: &str = "Name for this preset:";

pub(crate) fn presets_file(dirs: &AppDirs) -> PathBuf {
    dirs.config.join("eq-presets.toml")
}

/// The PRESETS button's menu.
pub(crate) fn presets_menu(
    user: &[EqPreset],
    has_track: bool,
    has_auto: bool,
) -> Vec<MenuEntry<MenuCmd>> {
    let mut load: Vec<MenuEntry<MenuCmd>> = amp_dsp::builtin_presets()
        .into_iter()
        .enumerate()
        .map(|(i, p)| MenuEntry::item(p.name, MenuCmd::EqLoadBuiltin(i)))
        .collect();
    if !user.is_empty() {
        load.push(MenuEntry::Separator);
        load.extend(
            user.iter()
                .enumerate()
                .map(|(i, p)| MenuEntry::item(p.name.clone(), MenuCmd::EqLoadUser(i))),
        );
    }
    load.push(MenuEntry::Separator);
    load.push(MenuEntry::item("From file (.eqf)...", MenuCmd::EqLoadFile));
    let save = vec![
        MenuEntry::item("Preset...", MenuCmd::EqSaveAs),
        MenuEntry::item("Auto-load preset for this track", MenuCmd::EqSaveAuto).enabled(has_track),
        MenuEntry::Separator,
        MenuEntry::item("To file (.eqf)...", MenuCmd::EqSaveFile),
    ];
    let mut delete: Vec<MenuEntry<MenuCmd>> = user
        .iter()
        .enumerate()
        .map(|(i, p)| MenuEntry::item(p.name.clone(), MenuCmd::EqDeleteUser(i)))
        .collect();
    if !delete.is_empty() {
        delete.push(MenuEntry::Separator);
    }
    delete.push(
        MenuEntry::item("Auto-load preset for this track", MenuCmd::EqDeleteAuto).enabled(has_auto),
    );
    vec![
        MenuEntry::submenu("Load", load),
        MenuEntry::submenu("Save", save),
        MenuEntry::submenu("Delete", delete),
        MenuEntry::Separator,
        MenuEntry::item("Reset to flat", MenuCmd::EqReset),
    ]
}

impl App {
    /// The playing (or current) track's path, for auto-load presets.
    fn eq_track(&self) -> Option<PathBuf> {
        self.last_status
            .info
            .as_deref()
            .map(|i| i.path.clone())
            .or_else(|| {
                let id = self.current?;
                self.queue_cache
                    .iter()
                    .find(|(q, _)| *q == id)
                    .map(|(_, p)| p.clone())
            })
    }

    pub(crate) fn eq_presets_menu(&self) -> Vec<MenuEntry<MenuCmd>> {
        let track = self.eq_track();
        let has_auto = track
            .as_deref()
            .is_some_and(|t| self.eq_presets.auto_for(t).is_some());
        presets_menu(&self.eq_presets.presets(), track.is_some(), has_auto)
    }

    /// Sets the sliders and the sound to `p` (the ON switch is left alone).
    pub(crate) fn apply_eq_preset(&mut self, p: &EqPreset) {
        self.eq.preamp_db = p.preamp_db;
        self.eq.bands_db = p.bands_db;
        self.eq_controls.set_preamp_db(p.preamp_db);
        for (i, db) in p.bands_db.iter().enumerate() {
            self.eq_controls.set_band_db(i, *db);
        }
        log::info!("equalizer preset: {}", p.name);
    }

    fn current_eq(&self, name: &str) -> EqPreset {
        EqPreset {
            name: name.to_owned(),
            preamp_db: self.eq.preamp_db,
            bands_db: self.eq.bands_db,
        }
    }

    fn save_eq_presets(&self) {
        if let Err(e) = self.eq_presets.save(&presets_file(&self.dirs)) {
            log::warn!("cannot save equalizer presets: {e}");
        }
    }

    /// With AUTO on, loads the auto-load preset saved for `path`, if any.
    pub(crate) fn auto_load_eq(&mut self, path: &std::path::Path) {
        if self.eq.auto
            && let Some(p) = self.eq_presets.auto_for(path)
        {
            self.apply_eq_preset(&p);
        }
    }

    pub(crate) fn run_eq_menu(&mut self, cmd: MenuCmd) -> Result<()> {
        match cmd {
            MenuCmd::EqLoadBuiltin(i) => {
                if let Some(p) = amp_dsp::builtin_presets().get(i) {
                    self.apply_eq_preset(p);
                }
            }
            MenuCmd::EqLoadUser(i) => {
                if let Some(p) = self.eq_presets.presets().get(i) {
                    self.apply_eq_preset(p);
                }
            }
            MenuCmd::EqReset => self.apply_eq_preset(&EqPreset {
                name: "Flat".into(),
                preamp_db: 0.0,
                bands_db: [0.0; 10],
            }),
            MenuCmd::EqSaveAs => {
                amp_app::dialog::ask_text(
                    "Save preset",
                    PRESET_NAME_PROMPT,
                    "My preset",
                    self.names.0.clone(),
                );
            }
            MenuCmd::EqSaveAuto => {
                if let Some(track) = self.eq_track() {
                    self.eq_presets
                        .set_auto(&track, self.eq.preamp_db, self.eq.bands_db);
                    self.save_eq_presets();
                }
            }
            MenuCmd::EqDeleteUser(i) => {
                if let Some(p) = self.eq_presets.presets().get(i) {
                    let name = p.name.clone();
                    self.eq_presets.remove(&name);
                    self.save_eq_presets();
                }
            }
            MenuCmd::EqDeleteAuto => {
                if let Some(track) = self.eq_track()
                    && self.eq_presets.remove_auto(&track)
                {
                    self.save_eq_presets();
                }
            }
            MenuCmd::EqLoadFile | MenuCmd::EqSaveFile if self.pick_mode.is_none() => {
                let start = self.last_dir.clone().or_else(|| self.dirs.music.clone());
                let tx = self.picks.0.clone();
                let (started, mode) = if cmd == MenuCmd::EqLoadFile {
                    (amp_app::dialog::pick_eqf(start, tx), PickMode::EqfLoad)
                } else {
                    (amp_app::dialog::pick_eqf_save(start, tx), PickMode::EqfSave)
                };
                if started {
                    self.pick_mode = Some(mode);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The name typed for "Save > Preset...".
    pub(crate) fn preset_named(&mut self, name: &str) {
        let p = self.current_eq(name);
        self.eq_presets.put(&p);
        self.save_eq_presets();
    }

    /// Loads an .eqf file: its presets join the saved ones and the first is applied.
    pub(crate) fn load_eqf(&mut self, path: &std::path::Path) {
        let result = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| amp_dsp::eqf::parse(&b).map_err(|e| e.to_string()));
        match result {
            Ok(presets) => {
                for p in &presets {
                    self.eq_presets.put(p);
                }
                self.save_eq_presets();
                if let Some(first) = presets.first() {
                    self.apply_eq_preset(first);
                }
            }
            Err(e) => {
                let msg = format!("Cannot load {}: {e}", path.display());
                amp_app::dialog::show_message("Load preset", move || msg);
            }
        }
    }

    /// Writes the current setting as an .eqf file named after the file.
    pub(crate) fn save_eqf(&mut self, mut path: PathBuf) {
        if path.extension().is_none() {
            path.set_extension("eqf");
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Preset".into());
        let bytes = amp_dsp::eqf::write(&[self.current_eq(&name)]);
        if let Err(e) = std::fs::write(&path, bytes) {
            let msg = format!("Cannot save {}: {e}", path.display());
            amp_app::dialog::show_message("Save preset", move || msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(entries: &[MenuEntry<MenuCmd>]) -> Vec<String> {
        entries
            .iter()
            .map(|e| match e {
                MenuEntry::Item(i) => i.label.clone(),
                MenuEntry::Separator => "-".into(),
            })
            .collect()
    }

    fn sub<'a>(entries: &'a [MenuEntry<MenuCmd>], label: &str) -> &'a [MenuEntry<MenuCmd>] {
        entries
            .iter()
            .find_map(|e| match e {
                MenuEntry::Item(i) if i.label == label => Some(i.submenu.as_slice()),
                _ => None,
            })
            .unwrap_or(&[])
    }

    #[test]
    fn presets_menu_layout() {
        let m = presets_menu(&[], false, false);
        assert_eq!(labels(&m), ["Load", "Save", "Delete", "-", "Reset to flat"]);
        let load = labels(sub(&m, "Load"));
        assert_eq!(load.len(), 18 + 2);
        assert_eq!(load[0], "Classical");
        assert_eq!(load.last().unwrap(), "From file (.eqf)...");
        // Nothing of the user's yet: Delete has only the disabled auto-load item.
        let delete = sub(&m, "Delete");
        assert_eq!(delete.len(), 1);
        assert!(matches!(&delete[0], MenuEntry::Item(i) if !i.enabled));

        let mine = EqPreset {
            name: "Mine".into(),
            preamp_db: 0.0,
            bands_db: [0.0; 10],
        };
        let m = presets_menu(std::slice::from_ref(&mine), true, true);
        let load = labels(sub(&m, "Load"));
        assert!(load.contains(&"Mine".to_owned()));
        assert_eq!(
            labels(sub(&m, "Delete")),
            ["Mine", "-", "Auto-load preset for this track"]
        );
        assert!(sub(&m, "Save").iter().all(|e| match e {
            MenuEntry::Item(i) => i.enabled,
            MenuEntry::Separator => true,
        }));
    }
}
