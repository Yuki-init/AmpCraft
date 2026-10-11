//! The popup menus of the player binary: the main menu (right-click on the main window, with
//! the options menu as a submenu) and the visualization menu (V button, right-click on the
//! visualizer), what their items do, and the popup windows that show them.

use super::*;
use amp_ui::{MenuEntry, MenuEvent, MenuStack};

/// What a menu item does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum MenuCmd {
    About,
    Preferences,
    OpenFiles,
    AddFiles,
    FileInfo,
    TogglePlaylist,
    ToggleEqualizer,
    ToggleLibrary,
    Theme(usize),
    ShowRemaining(bool),
    ToggleRemaining,
    AlwaysOnTop,
    DoubleSize,
    Repeat,
    Shuffle,
    Vis(VisMode),
    Playback(Action),
    Exit,
    SortByTitle,
    SortByFileName,
    SortByPath,
    ReverseList,
    RandomizeList,
    RemoveMissing,
    RemoveDuplicates,
    RefreshTitles,
    LibraryAddFolder,
    LibraryRescan,
    EqLoadBuiltin(usize),
    EqLoadUser(usize),
    EqLoadFile,
    EqSaveAs,
    EqSaveAuto,
    EqSaveFile,
    EqDeleteUser(usize),
    EqDeleteAuto,
    EqReset,
}

/// The settings menus show ticks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct MenuState {
    pub playlist: bool,
    pub equalizer: bool,
    pub library: bool,
    pub remaining: bool,
    pub on_top: bool,
    pub double: bool,
    pub repeat: bool,
    pub shuffle: bool,
    pub vis: VisMode,
    /// Index into the built-in themes, if one is in use.
    pub theme: Option<usize>,
    /// The playlist has a track to show info for.
    pub has_track: bool,
}

pub(crate) fn skins_menu(s: &MenuState) -> Vec<MenuEntry<MenuCmd>> {
    amp_skin::theme::THEMES
        .iter()
        .enumerate()
        .map(|(i, t)| MenuEntry::item(t.name, MenuCmd::Theme(i)).radio(s.theme == Some(i)))
        .chain([
            MenuEntry::Separator,
            MenuEntry::item("Next skin", MenuCmd::Theme(usize::MAX)).shortcut("Alt+S"),
        ])
        .collect()
}

pub(crate) fn options_menu(s: &MenuState) -> Vec<MenuEntry<MenuCmd>> {
    vec![
        MenuEntry::item("Preferences...", MenuCmd::Preferences).shortcut("Ctrl+P"),
        MenuEntry::submenu("Skins", skins_menu(s)),
        MenuEntry::Separator,
        MenuEntry::item("Time elapsed", MenuCmd::ShowRemaining(false))
            .radio(!s.remaining)
            .shortcut("Ctrl+T"),
        MenuEntry::item("Time remaining", MenuCmd::ShowRemaining(true))
            .radio(s.remaining)
            .shortcut("Ctrl+T"),
        MenuEntry::item("Always on top", MenuCmd::AlwaysOnTop)
            .checked(s.on_top)
            .shortcut("Ctrl+A"),
        MenuEntry::item("Double size", MenuCmd::DoubleSize)
            .checked(s.double)
            .shortcut("Ctrl+D"),
        MenuEntry::Separator,
        MenuEntry::item("Repeat", MenuCmd::Repeat)
            .checked(s.repeat)
            .shortcut("R"),
        MenuEntry::item("Shuffle", MenuCmd::Shuffle)
            .checked(s.shuffle)
            .shortcut("S"),
    ]
}

pub(crate) fn vis_menu(s: &MenuState) -> Vec<MenuEntry<MenuCmd>> {
    [
        ("Spectrum analyzer", VisMode::Spectrum),
        ("Oscilloscope", VisMode::Oscilloscope),
        ("Off", VisMode::Off),
    ]
    .into_iter()
    .map(|(label, mode)| MenuEntry::item(label, MenuCmd::Vis(mode)).radio(s.vis == mode))
    .collect()
}

pub(crate) fn playback_menu() -> Vec<MenuEntry<MenuCmd>> {
    [
        ("Previous", Action::Previous, "Z"),
        ("Play", Action::Play, "X"),
        ("Pause", Action::Pause, "C"),
        ("Stop", Action::Stop, "V"),
        ("Next", Action::Next, "B"),
    ]
    .into_iter()
    .map(|(label, action, key)| MenuEntry::item(label, MenuCmd::Playback(action)).shortcut(key))
    .chain([
        MenuEntry::Separator,
        MenuEntry::item("Back 5 seconds", MenuCmd::Playback(Action::SeekBy(-5))).shortcut("Left"),
        MenuEntry::item("Forward 5 seconds", MenuCmd::Playback(Action::SeekBy(5)))
            .shortcut("Right"),
    ])
    .collect()
}

pub(crate) fn main_menu(s: &MenuState) -> Vec<MenuEntry<MenuCmd>> {
    vec![
        MenuEntry::item("About OpenAMP...", MenuCmd::About),
        MenuEntry::item("Preferences...", MenuCmd::Preferences).shortcut("Ctrl+P"),
        MenuEntry::Separator,
        MenuEntry::item("Play files...", MenuCmd::OpenFiles).shortcut("L"),
        MenuEntry::item("Add files...", MenuCmd::AddFiles),
        MenuEntry::item("File info...", MenuCmd::FileInfo)
            .enabled(s.has_track)
            .shortcut("Alt+3"),
        MenuEntry::Separator,
        MenuEntry::item("Playlist editor", MenuCmd::TogglePlaylist)
            .checked(s.playlist)
            .shortcut("Alt+E"),
        MenuEntry::item("Equalizer", MenuCmd::ToggleEqualizer)
            .checked(s.equalizer)
            .shortcut("Alt+G"),
        MenuEntry::item("Media Library", MenuCmd::ToggleLibrary)
            .checked(s.library)
            .shortcut("Alt+L"),
        MenuEntry::Separator,
        MenuEntry::submenu("Skins", skins_menu(s)),
        MenuEntry::submenu("Options", options_menu(s)),
        MenuEntry::submenu("Playback", playback_menu()),
        MenuEntry::submenu("Visualization", vis_menu(s)),
        MenuEntry::Separator,
        MenuEntry::item("Exit", MenuCmd::Exit),
    ]
}

/// Playlist MISC > SORT.
pub(crate) fn playlist_sort_menu() -> Vec<MenuEntry<MenuCmd>> {
    vec![
        MenuEntry::item("Sort by title", MenuCmd::SortByTitle),
        MenuEntry::item("Sort by file name", MenuCmd::SortByFileName),
        MenuEntry::item("Sort by path and file name", MenuCmd::SortByPath),
        MenuEntry::Separator,
        MenuEntry::item("Reverse list", MenuCmd::ReverseList),
        MenuEntry::item("Randomize list", MenuCmd::RandomizeList),
    ]
}

/// Playlist REM > MISC.
pub(crate) fn playlist_remove_menu() -> Vec<MenuEntry<MenuCmd>> {
    vec![
        MenuEntry::item("Remove missing files", MenuCmd::RemoveMissing),
        MenuEntry::item("Remove duplicate entries", MenuCmd::RemoveDuplicates),
    ]
}

/// Playlist MISC > OPTS.
pub(crate) fn playlist_misc_menu(s: &MenuState) -> Vec<MenuEntry<MenuCmd>> {
    vec![
        MenuEntry::item("Reread titles from files", MenuCmd::RefreshTitles),
        MenuEntry::item("File info...", MenuCmd::FileInfo).enabled(s.has_track),
    ]
}

/// The media library's Library button.
pub(crate) fn library_menu() -> Vec<MenuEntry<MenuCmd>> {
    vec![
        MenuEntry::item("Add folder to library...", MenuCmd::LibraryAddFolder),
        MenuEntry::item("Rescan library", MenuCmd::LibraryRescan),
    ]
}

/// Which menu to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuKind {
    Main,
    Visualization,
    PlaylistSort,
    PlaylistRemove,
    PlaylistMisc,
    Library,
    EqPresets,
}

/// An open menu and how it was opened.
pub(crate) struct OpenMenu {
    stack: MenuStack<MenuCmd>,
    /// Where it opened, and whether a button release there should be ignored (the release
    /// of the right-click that opened it).
    at: Point,
    armed: bool,
    /// Render hash of what the popups show.
    hash: Option<u64>,
}

/// A shortcut key on the main window, as a menu command.
pub(crate) fn shortcut(keysym: u32, mods: Modifiers) -> Option<MenuCmd> {
    let lower = keysym | 0x20;
    let letter = (0x61..=0x7a)
        .contains(&lower)
        .then_some(lower as u8 as char);
    Some(match (letter, mods.ctrl, mods.alt) {
        (Some('a'), true, false) => MenuCmd::AlwaysOnTop,
        (Some('p'), true, false) => MenuCmd::Preferences,
        (Some('d'), true, false) => MenuCmd::DoubleSize,
        (Some('t'), true, false) => MenuCmd::ToggleRemaining,
        (Some('e'), false, true) => MenuCmd::TogglePlaylist,
        (Some('g'), false, true) => MenuCmd::ToggleEqualizer,
        (Some('l'), false, false) => MenuCmd::OpenFiles,
        (Some('r'), false, false) => MenuCmd::Repeat,
        (Some('s'), false, false) => MenuCmd::Shuffle,
        // Alt+3, as in the classic player.
        (None, false, true) if keysym == 0x33 => MenuCmd::FileInfo,
        _ => return None,
    })
}

impl App {
    fn menu_state(&self) -> MenuState {
        let visible = |panel| self.wins.iter().any(|w| w.panel == panel && w.visible);
        MenuState {
            playlist: visible(PL),
            equalizer: visible(EQ),
            library: visible(LIB),
            remaining: self.main.show_remaining(),
            on_top: self.main.always_on_top(),
            double: self.scale > 1,
            repeat: self.main.repeat(),
            shuffle: self.main.shuffle(),
            vis: self.main.vis.mode,
            theme: self.theme_index(),
            has_track: self.info_track().is_some(),
        }
    }

    /// Opens a menu at `at` (desktop pixels). `armed` is false when a button is still held
    /// (a right-click press), so its release does not pick the item under it.
    pub(crate) fn open_menu(&mut self, kind: MenuKind, at: Point, armed: bool) -> Result<()> {
        self.close_menu()?;
        let s = self.menu_state();
        let entries = match kind {
            MenuKind::Main => main_menu(&s),
            MenuKind::Visualization => vis_menu(&s),
            MenuKind::PlaylistSort => playlist_sort_menu(),
            MenuKind::PlaylistRemove => playlist_remove_menu(),
            MenuKind::PlaylistMisc => playlist_misc_menu(&s),
            MenuKind::Library => library_menu(),
            MenuKind::EqPresets => self.eq_presets_menu(),
        };
        let screens = self.platform.screens()?;
        let screen = screens
            .iter()
            .copied()
            .find(|r| r.contains(at))
            .or(screens.first().copied())
            .unwrap_or(Rect::new(0, 0, 1920, 1080));
        let stack = MenuStack::open(
            entries,
            at,
            screen,
            &self.skin,
            &mut self.font,
            self.scale as u32,
        );
        self.menu = Some(OpenMenu {
            stack,
            at,
            armed,
            hash: None,
        });
        self.sync_menu()?;
        if let Some(&first) = self.popups.first()
            && !self.platform.grab_input(first)?
        {
            log::warn!("could not grab the pointer; the menu closes on Escape or a choice");
        }
        Ok(())
    }

    pub(crate) fn close_menu(&mut self) -> Result<()> {
        if self.menu.take().is_some() {
            self.platform.release_input()?;
            for &id in &self.popups {
                self.platform.set_visible(id, false)?;
            }
            self.popups_shown = 0;
        }
        Ok(())
    }

    /// Shows each open menu level in its popup window.
    fn sync_menu(&mut self) -> Result<()> {
        let Some(menu) = &self.menu else {
            return Ok(());
        };
        let hash = menu.stack.render_hash();
        if menu.hash == Some(hash) {
            return Ok(());
        }
        let depth = menu.stack.depth();
        let owner = self.wins.iter().find(|w| w.panel == MAIN).map(|w| w.id);
        while self.popups.len() < depth {
            let id = self.platform.create_window(&WindowSpec {
                title: "Menu".into(),
                rect: Rect::new(0, 0, 1, 1),
                kind: WindowKind::Popup,
                owner,
            })?;
            self.popups.push(id);
        }
        for i in 0..self.popups.len() {
            let id = self.popups[i];
            let Some(menu) = &self.menu else { break };
            match menu.stack.rect(i) {
                Some(r) => {
                    let frame = menu.stack.render(i);
                    self.platform.move_window(id, Point::new(r.x, r.y))?;
                    self.platform.resize_window(id, Size::new(r.w, r.h))?;
                    if i >= self.popups_shown {
                        self.platform.set_visible(id, true)?;
                    }
                    self.platform
                        .present(id, Size::new(r.w, r.h), frame.pixels())?;
                }
                None if i < self.popups_shown => self.platform.set_visible(id, false)?,
                None => {}
            }
        }
        self.popups_shown = depth;
        if let Some(menu) = &mut self.menu {
            menu.hash = Some(hash);
        }
        Ok(())
    }

    /// Sends input to the open menu. Returns false when no menu is open or the event is not
    /// input (it is then handled as usual).
    pub(crate) fn menu_event(&mut self, event: &Event) -> Result<bool> {
        let Some(menu) = &mut self.menu else {
            return Ok(false);
        };
        let result = match *event {
            Event::Motion { root, .. } => {
                if (root.x - menu.at.x).abs() > 3 || (root.y - menu.at.y).abs() > 3 {
                    menu.armed = true;
                }
                menu.stack.pointer_move(root, &self.skin, &mut self.font);
                MenuEvent::None
            }
            Event::ButtonDown { root, .. } => {
                menu.armed = true;
                menu.stack.press(root)
            }
            Event::ButtonUp { root, .. } if menu.armed => menu.stack.release(root),
            Event::ButtonUp { .. } => {
                menu.armed = true;
                MenuEvent::None
            }
            Event::Key {
                keysym,
                pressed: true,
                ..
            } => menu.stack.key(keysym, &self.skin, &mut self.font),
            Event::Key { .. } => MenuEvent::None,
            _ => return Ok(false),
        };
        match result {
            MenuEvent::None => self.sync_menu()?,
            MenuEvent::Close => self.close_menu()?,
            MenuEvent::Chosen(cmd) => {
                self.close_menu()?;
                self.run_menu(cmd)?;
            }
        }
        Ok(true)
    }

    pub(crate) fn run_menu(&mut self, cmd: MenuCmd) -> Result<()> {
        match cmd {
            MenuCmd::About => {
                amp_app::dialog::show_message("About OpenAMP", amp_app::info::about);
            }
            MenuCmd::Preferences => self.show_prefs()?,
            MenuCmd::OpenFiles => self.open_dialog(PickMode::Replace),
            MenuCmd::AddFiles => self.open_dialog(PickMode::Append),
            MenuCmd::FileInfo => self.show_file_info(),
            MenuCmd::TogglePlaylist => {
                let on = !self.menu_state().playlist;
                self.set_panel_visible(PL, on)?;
            }
            MenuCmd::ToggleEqualizer => {
                let on = !self.menu_state().equalizer;
                self.set_panel_visible(EQ, on)?;
            }
            MenuCmd::ToggleLibrary => {
                let on = !self.menu_state().library;
                self.show_library(on)?;
            }
            MenuCmd::Theme(usize::MAX) => self.next_theme()?,
            MenuCmd::Theme(i) => self.set_theme(i)?,
            MenuCmd::ShowRemaining(on) => self.main.set_show_remaining(on),
            MenuCmd::ToggleRemaining => {
                let on = !self.main.show_remaining();
                self.main.set_show_remaining(on);
            }
            MenuCmd::AlwaysOnTop => {
                let on = !self.main.always_on_top();
                self.set_always_on_top(on)?;
            }
            MenuCmd::DoubleSize => self.act(Action::ToggleDoubleSize)?,
            MenuCmd::Repeat => {
                let on = !self.main.repeat();
                self.main.set_repeat(on);
                self.act(Action::SetRepeat(on))?;
            }
            MenuCmd::Shuffle => {
                let on = !self.main.shuffle();
                self.main.set_shuffle(on);
                self.act(Action::SetShuffle(on))?;
            }
            MenuCmd::Vis(mode) => self.main.vis.mode = mode,
            MenuCmd::Playback(action) => self.act(action)?,
            MenuCmd::Exit => self.quit = true,
            MenuCmd::SortByTitle | MenuCmd::SortByFileName | MenuCmd::SortByPath => {
                let mut keyed: Vec<(String, TrackId)> = self
                    .queue_cache
                    .iter()
                    .map(|(id, path)| {
                        let key = match cmd {
                            MenuCmd::SortByTitle => self.title_for(*id, path),
                            MenuCmd::SortByFileName => path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                            _ => path.to_string_lossy().into_owned(),
                        };
                        (key.to_lowercase(), *id)
                    })
                    .collect();
                keyed.sort();
                let order: Vec<TrackId> = keyed.into_iter().map(|(_, id)| id).collect();
                self.player.with_queue(|q| q.reorder(&order));
            }
            MenuCmd::ReverseList => {
                let order: Vec<TrackId> =
                    self.queue_cache.iter().rev().map(|(id, _)| *id).collect();
                self.player.with_queue(|q| q.reorder(&order));
            }
            MenuCmd::RandomizeList => self.player.with_queue(|q| q.randomize()),
            MenuCmd::RemoveMissing => {
                let gone: Vec<TrackId> = self
                    .queue_cache
                    .iter()
                    .filter(|(_, p)| !p.exists())
                    .map(|(id, _)| *id)
                    .collect();
                self.player.with_queue(|q| {
                    gone.iter().for_each(|id| {
                        q.remove(*id);
                    })
                });
            }
            MenuCmd::RemoveDuplicates => {
                let mut seen = HashSet::new();
                let dups: Vec<TrackId> = self
                    .queue_cache
                    .iter()
                    .filter(|(_, p)| !seen.insert(p.clone()))
                    .map(|(id, _)| *id)
                    .collect();
                self.player.with_queue(|q| {
                    dups.iter().for_each(|id| {
                        q.remove(*id);
                    })
                });
            }
            MenuCmd::LibraryAddFolder => {
                let start = self.dirs.music.clone();
                amp_app::dialog::pick_folder(start, self.folder_picks.0.clone());
            }
            MenuCmd::LibraryRescan => self.start_scan(),
            MenuCmd::EqLoadBuiltin(_)
            | MenuCmd::EqLoadUser(_)
            | MenuCmd::EqLoadFile
            | MenuCmd::EqSaveAs
            | MenuCmd::EqSaveAuto
            | MenuCmd::EqSaveFile
            | MenuCmd::EqDeleteUser(_)
            | MenuCmd::EqDeleteAuto
            | MenuCmd::EqReset => self.run_eq_menu(cmd)?,
            MenuCmd::RefreshTitles => {
                // Forget probed tags; the next update probes every entry again.
                self.meta.clear();
                self.requested.clear();
                self.queue_version = u64::MAX;
                self.entries_dirty = true;
            }
        }
        Ok(())
    }

    /// Keeps every window above other programs (or not) and lights the A button.
    pub(crate) fn set_always_on_top(&mut self, on: bool) -> Result<()> {
        self.main.set_always_on_top(on);
        let ids: Vec<WindowId> = self.wins.iter().map(|w| w.id).collect();
        for id in ids {
            self.platform.set_always_on_top(id, on)?;
        }
        Ok(())
    }

    /// The track the file info box describes: the current one, else the first.
    fn info_track(&self) -> Option<PathBuf> {
        let id = self
            .current
            .or_else(|| self.queue_cache.first().map(|(id, _)| *id))?;
        self.queue_cache
            .iter()
            .find(|(q, _)| *q == id)
            .map(|(_, p)| p.clone())
    }

    pub(crate) fn show_file_info(&mut self) {
        if let Some(path) = self.info_track() {
            self.show_file_info_for(path);
        }
    }

    pub(crate) fn show_file_info_for(&mut self, path: PathBuf) {
        amp_app::dialog::show_message("File info", move || {
            match amp_core::Decoder::open(&path, 44_100) {
                Ok(dec) => amp_app::info::describe(&path, dec.info()),
                Err(e) => format!("{}\n\nCannot read this file: {e}", path.display()),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amp_ui::Mark;

    fn labels(entries: &[MenuEntry<MenuCmd>]) -> Vec<String> {
        entries
            .iter()
            .map(|e| match e {
                MenuEntry::Item(i) => i.label.clone(),
                MenuEntry::Separator => "-".into(),
            })
            .collect()
    }

    fn item<'a>(
        entries: &'a [MenuEntry<MenuCmd>],
        label: &str,
    ) -> &'a amp_ui::menu::MenuItem<MenuCmd> {
        entries
            .iter()
            .find_map(|e| match e {
                MenuEntry::Item(i) if i.label == label => Some(i),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no {label}"))
    }

    #[test]
    fn options_menu_reflects_state() {
        let s = MenuState {
            remaining: true,
            on_top: true,
            shuffle: true,
            ..MenuState::default()
        };
        let m = options_menu(&s);
        assert_eq!(item(&m, "Time remaining").mark, Mark::Radio(true));
        assert_eq!(item(&m, "Time elapsed").mark, Mark::Radio(false));
        assert_eq!(item(&m, "Always on top").mark, Mark::Check(true));
        assert_eq!(item(&m, "Double size").mark, Mark::Check(false));
        assert_eq!(item(&m, "Shuffle").mark, Mark::Check(true));
        assert_eq!(item(&m, "Repeat").mark, Mark::Check(false));
    }

    #[test]
    fn skins_menu_marks_the_current_theme() {
        let s = MenuState {
            theme: Some(1),
            ..MenuState::default()
        };
        let m = skins_menu(&s);
        assert_eq!(m.len(), amp_skin::theme::THEMES.len() + 2);
        let marks: Vec<_> = m
            .iter()
            .filter_map(|e| match e {
                MenuEntry::Item(i) => Some(i.mark),
                MenuEntry::Separator => None,
            })
            .take(amp_skin::theme::THEMES.len())
            .collect();
        assert_eq!(marks[1], Mark::Radio(true));
        assert_eq!(marks.iter().filter(|m| **m == Mark::Radio(true)).count(), 1);
    }

    #[test]
    fn main_menu_layout() {
        let m = main_menu(&MenuState::default());
        assert_eq!(
            labels(&m),
            [
                "About OpenAMP...",
                "Preferences...",
                "-",
                "Play files...",
                "Add files...",
                "File info...",
                "-",
                "Playlist editor",
                "Equalizer",
                "Media Library",
                "-",
                "Skins",
                "Options",
                "Playback",
                "Visualization",
                "-",
                "Exit",
            ]
        );
        assert!(!item(&m, "File info...").enabled, "nothing to describe");
        let with_track = main_menu(&MenuState {
            has_track: true,
            ..MenuState::default()
        });
        assert!(item(&with_track, "File info...").enabled);
    }

    #[test]
    fn playlist_menus() {
        assert_eq!(
            labels(&playlist_sort_menu()),
            [
                "Sort by title",
                "Sort by file name",
                "Sort by path and file name",
                "-",
                "Reverse list",
                "Randomize list"
            ]
        );
        assert_eq!(playlist_remove_menu().len(), 2);
        let misc = playlist_misc_menu(&MenuState::default());
        assert!(!item(&misc, "File info...").enabled);
    }

    #[test]
    fn vis_menu_marks_the_mode() {
        let m = vis_menu(&MenuState {
            vis: VisMode::Oscilloscope,
            ..MenuState::default()
        });
        assert_eq!(item(&m, "Oscilloscope").mark, Mark::Radio(true));
        assert_eq!(item(&m, "Off").mark, Mark::Radio(false));
    }

    #[test]
    fn shortcuts_map_to_commands() {
        let none = Modifiers::default();
        let ctrl = Modifiers { ctrl: true, ..none };
        let alt = Modifiers { alt: true, ..none };
        assert_eq!(shortcut(0x61, ctrl), Some(MenuCmd::AlwaysOnTop));
        assert_eq!(shortcut(0x41, ctrl), Some(MenuCmd::AlwaysOnTop));
        assert_eq!(shortcut(0x64, ctrl), Some(MenuCmd::DoubleSize));
        assert_eq!(shortcut(0x65, alt), Some(MenuCmd::TogglePlaylist));
        assert_eq!(shortcut(0x67, alt), Some(MenuCmd::ToggleEqualizer));
        assert_eq!(shortcut(0x72, none), Some(MenuCmd::Repeat));
        assert_eq!(shortcut(0x33, alt), Some(MenuCmd::FileInfo));
        assert_eq!(shortcut(0x61, none), None);
        assert_eq!(
            shortcut(0x78, none),
            None,
            "x is Play, handled by the window"
        );
    }
}
