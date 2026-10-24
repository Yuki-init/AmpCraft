//! The preferences window in the player binary: opening it (O button, Ctrl+P, menus), keeping
//! it in sync with the settings, applying what is clicked, and moving it by its title bar.
//! It is not part of the docked group.

use super::*;
use ampcraft_ui::{PrefsAction, PrefsDown, PrefsState};

/// The preferences window's platform side.
#[derive(Default)]
pub(crate) struct PrefsWin {
    pub id: Option<WindowId>,
    pub visible: bool,
    /// Render hash and size of what was last shown.
    hash: Option<u64>,
    size: Option<Size>,
    /// Where the window is, and the pointer offset inside it while dragging.
    pos: Point,
    drag: Option<Point>,
    /// Output devices, listed when the window opens.
    devices: Vec<(String, String)>,
}

impl App {
    fn prefs_state(&self) -> PrefsState {
        let lib_status = if self.scan.is_some() {
            "Scanning...".to_owned()
        } else {
            let n = self
                .library
                .as_ref()
                .and_then(|l| l.stats().ok())
                .map_or(0, |s| s.tracks);
            format!("{n} tracks in the library")
        };
        PrefsState {
            always_on_top: self.main.always_on_top(),
            scale: self.chosen_scale,
            show_remaining: self.main.show_remaining(),
            repeat: self.main.repeat(),
            shuffle: self.main.shuffle(),
            vis: self.main.vis.mode,
            devices: self.prefs_win.devices.clone(),
            device: self.config.player.device.clone(),
            themes: ampcraft_skin::theme::THEMES
                .iter()
                .map(|t| t.name.to_owned())
                .collect(),
            theme: self.theme_index(),
            system_theme: self.follows_system(),
            skin_file: self
                .skin_path
                .as_ref()
                .filter(|_| self.theme_index().is_none() && !self.follows_system())
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned()),
            include_music: self.config.library.include_music_folder,
            music_dir: self.dirs.music.as_ref().map(|d| d.display().to_string()),
            folders: self
                .config
                .library
                .folders
                .iter()
                .map(|f| f.display().to_string())
                .collect(),
            library_status: lib_status,
        }
    }

    /// Opens the preferences window (or brings it forward), centered on the main window's
    /// screen the first time.
    pub(crate) fn show_prefs(&mut self) -> Result<()> {
        let size = Size::new(
            ampcraft_ui::prefs_window::WIDTH * self.scale,
            ampcraft_ui::prefs_window::HEIGHT * self.scale,
        );
        if self.prefs_win.id.is_none() {
            let main = self.layout.rect(MAIN);
            let screens = self.platform.screens()?;
            let screen = main
                .and_then(|m| {
                    screens
                        .iter()
                        .copied()
                        .find(|s| s.contains(Point::new(m.x, m.y)))
                })
                .or(screens.first().copied())
                .unwrap_or(Rect::new(0, 0, 1920, 1080));
            let pos = Point::new(
                screen.x + (screen.w - size.w).max(0) / 2,
                screen.y + (screen.h - size.h).max(0) / 2,
            );
            let owner = self.wins.iter().find(|w| w.panel == MAIN).map(|w| w.id);
            let id = self.platform.create_window(&WindowSpec {
                title: "Preferences".into(),
                rect: Rect::new(pos.x, pos.y, size.w, size.h),
                kind: WindowKind::Dialog,
                owner,
            })?;
            self.prefs_win.id = Some(id);
            self.prefs_win.pos = pos;
            self.prefs_win.size = Some(size);
        }
        // Devices can come and go; list them each time the window opens.
        self.prefs_win.devices = match self._output {
            Output::Device(_) => ampcraft_audio_out::list_outputs().unwrap_or_default(),
            Output::Null(_) => Vec::new(),
        };
        let state = self.prefs_state();
        self.prefs.set_state(state);
        if let Some(id) = self.prefs_win.id {
            self.prefs_win.visible = true;
            self.prefs_win.hash = None;
            self.platform.set_visible(id, true)?;
            self.platform.raise(id)?;
        }
        self.redraw_prefs()
    }

    pub(crate) fn hide_prefs(&mut self) -> Result<()> {
        if let Some(id) = self.prefs_win.id
            && self.prefs_win.visible
        {
            self.prefs_win.visible = false;
            self.prefs_win.drag = None;
            self.platform.set_visible(id, false)?;
        }
        Ok(())
    }

    /// Renders and shows the window when anything it shows changed.
    pub(crate) fn redraw_prefs(&mut self) -> Result<()> {
        let Some(id) = self.prefs_win.id.filter(|_| self.prefs_win.visible) else {
            return Ok(());
        };
        let state = self.prefs_state();
        self.prefs.set_state(state);
        let scale = self.scale as u32;
        let hash = self.prefs.render_hash(scale);
        if self.prefs_win.hash == Some(hash) {
            return Ok(());
        }
        let frame = self.prefs.render(&self.skin, &mut self.font, scale);
        let size = Size::new(frame.width() as i32, frame.height() as i32);
        if self.prefs_win.size != Some(size) {
            self.platform.resize_window(id, size)?;
            self.prefs_win.size = Some(size);
        }
        self.platform.present(id, size, frame.pixels())?;
        self.prefs_win.hash = Some(hash);
        Ok(())
    }

    /// Handles events for the preferences window. Returns false for other windows.
    pub(crate) fn prefs_event(&mut self, event: &Event) -> Result<bool> {
        let Some(id) = self.prefs_win.id else {
            return Ok(false);
        };
        let scale = self.scale;
        let unscale = |p: Point| Point::new(p.x / scale, p.y / scale);
        match *event {
            Event::ButtonDown {
                window,
                button: MouseButton::Left,
                local,
                root,
                ..
            } if window == id => {
                if self.prefs.pointer_down(unscale(local)) == PrefsDown::DragWindow {
                    self.prefs_win.drag = Some(Point::new(
                        root.x - self.prefs_win.pos.x,
                        root.y - self.prefs_win.pos.y,
                    ));
                }
            }
            Event::Motion {
                window,
                local,
                root,
            } if window == id => {
                if let Some(offset) = self.prefs_win.drag {
                    let pos = Point::new(root.x - offset.x, root.y - offset.y);
                    self.prefs_win.pos = pos;
                    self.platform.move_window(id, pos)?;
                } else if self.prefs.is_interacting() {
                    self.prefs.pointer_move(unscale(local));
                }
            }
            Event::ButtonUp {
                window,
                button: MouseButton::Left,
                local,
                ..
            } if window == id => {
                if self.prefs_win.drag.take().is_none() {
                    for a in self.prefs.pointer_up(unscale(local)) {
                        self.act_prefs(a)?;
                    }
                }
            }
            Event::Key {
                window,
                keysym,
                pressed: true,
                ..
            } if window == id => {
                for a in self.prefs.key(keysym) {
                    self.act_prefs(a)?;
                }
            }
            Event::Focus { window, focused } if window == id => self.prefs.focused = focused,
            Event::Expose { window } if window == id => self.prefs_win.hash = None,
            Event::CloseRequested { window } if window == id => self.hide_prefs()?,
            Event::Configured { window, rect } if window == id => {
                if self.prefs_win.drag.is_none() {
                    self.prefs_win.pos = Point::new(rect.x, rect.y);
                }
            }
            _ => return Ok(false),
        }
        self.redraw_prefs()?;
        Ok(true)
    }

    fn act_prefs(&mut self, action: PrefsAction) -> Result<()> {
        match action {
            PrefsAction::SetAlwaysOnTop(on) => self.set_always_on_top(on)?,
            PrefsAction::SetScale(Some(n)) => {
                let n = n.clamp(1, 3);
                self.chosen_scale = Some(n);
                let n = i32::from(n);
                if n != self.scale {
                    self.set_scale(n)?;
                }
            }
            PrefsAction::SetScale(None) => {
                self.chosen_scale = None;
                let screens = self.platform.screens()?;
                let height = screens.first().map_or(0, |s| s.h);
                let auto = i32::from(ampcraft_platform::auto_scale(height, self.platform.dpi()));
                if auto != self.scale {
                    self.set_scale(auto)?;
                }
            }
            PrefsAction::SetShowRemaining(on) => self.main.set_show_remaining(on),
            PrefsAction::SetRepeat(on) => {
                self.main.set_repeat(on);
                self.act(Action::SetRepeat(on))?;
            }
            PrefsAction::SetShuffle(on) => {
                self.main.set_shuffle(on);
                self.act(Action::SetShuffle(on))?;
            }
            PrefsAction::SetVis(mode) => self.main.vis.mode = mode,
            PrefsAction::SetDevice(device) => {
                self.config.player.device = device;
                self.save_state(false);
            }
            PrefsAction::SetTheme(i) => self.set_theme(i)?,
            PrefsAction::SystemTheme => self.set_system_theme()?,
            PrefsAction::LoadSkinFile => {
                if self.pick_mode.is_none() {
                    let start = self
                        .skin_path
                        .as_ref()
                        .filter(|p| p.is_file())
                        .and_then(|p| p.parent().map(PathBuf::from))
                        .or_else(|| self.dirs.music.clone());
                    if ampcraft::dialog::pick_skin(start, self.picks.0.clone()) {
                        self.pick_mode = Some(PickMode::Skin);
                    }
                }
            }
            PrefsAction::SetIncludeMusic(on) => {
                self.config.library.include_music_folder = on;
                self.save_state(false);
                if !on && let Some(music) = self.dirs.music.clone() {
                    self.forget_folder(&music);
                }
                self.start_scan();
            }
            PrefsAction::AddFolder => self.run_menu(menus::MenuCmd::LibraryAddFolder)?,
            PrefsAction::RemoveFolder(i) => {
                if i < self.config.library.folders.len() {
                    let folder = self.config.library.folders.remove(i);
                    self.save_state(false);
                    self.forget_folder(&folder);
                    self.start_scan();
                }
            }
            PrefsAction::Rescan => self.start_scan(),
            PrefsAction::Close => self.hide_prefs()?,
        }
        Ok(())
    }

    /// Drops a folder's tracks from the library database, unless another library folder
    /// still covers it.
    fn forget_folder(&mut self, folder: &std::path::Path) {
        let roots = self.config.library.roots(self.dirs.music.as_deref());
        if roots.iter().any(|r| folder.starts_with(r)) {
            return;
        }
        if let Some(lib) = &mut self.library {
            match lib.remove_folder(folder) {
                Ok(n) => log::info!("library: forgot {n} tracks under {}", folder.display()),
                Err(e) => log::warn!("cannot remove folder from library: {e}"),
            }
        }
        self.refresh_library();
        let q = self.lib.query();
        if let Err(e) = self.act_lib(q) {
            log::warn!("{e}");
        }
    }
}
