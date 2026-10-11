//! The player: main, equalizer and playlist windows wired to the playback engine.
//!
//! `openamp [--skin SKIN] [--scale 1|2|3] [--device NAME] [--audio device|null] [FILES...]`

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use amp_app::config::{Config, WindowState};
use amp_app::metadata::{Prober, TrackMeta};
use amp_app::null_sink::NullSink;
use amp_app::system_font::SystemFont;
use amp_audio_out::{AudioOutput, Controls, OutputDevice};
use amp_core::{PlaybackState, Player, PlayerConfig, Repeat, Status, TrackId};
use amp_dock::{Layout, PanelId, Rect};
use amp_dsp::{AnalyzerReader, EqControls, Equalizer, FFT_SIZE, Spectrum};
use amp_library::{Filter, Library};
use amp_platform::AppDirs;
use amp_platform::x11::X11Platform;
use amp_platform::{
    Event, Modifiers, MouseButton, Platform, Point, Size, WindowId, WindowKind, WindowSpec, keys,
};
use amp_skin::{Bitmap, Skin};
use amp_ui::main_window::{HEIGHT, WIDTH};
use amp_ui::{
    Action, DockDrag, EqAction, EqDown, EqWindow, LibAction, LibDown, LibraryRow, LibraryWindow,
    MainWindow, MoveTracker, NowPlaying, PlDown, PlayState, PlaylistAction, PlaylistEntry,
    PlaylistWindow, PointerDown, SelectMods, Selection, VisMode,
};
use anyhow::{Context, Result, bail};
use clap::Parser;

const MAIN: PanelId = PanelId(1);
const EQ: PanelId = PanelId(2);
const PL: PanelId = PanelId(3);
const LIB: PanelId = PanelId(4);

#[derive(Parser)]
#[command(version, about = "Classic skinnable audio player")]
struct Args {
    /// Skin file (.wsz) or folder. Defaults to the last one used, then skins/base-2.91.wsz.
    #[arg(long)]
    skin: Option<PathBuf>,
    /// Integer UI scale. Defaults to the last one used.
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=3))]
    scale: Option<u8>,
    /// Audio output device (see `openamp-cli devices`).
    #[arg(long)]
    device: Option<String>,
    /// `null` discards audio at real-time pace (headless runs, no sound hardware).
    #[arg(long, value_enum, default_value_t = AudioArg::Device)]
    audio: AudioArg,
    /// Starting volume, 0 to 100. Defaults to the last one used.
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
    volume: Option<u8>,
    /// Open the media library window at start (toggle with Alt+L).
    #[arg(long)]
    library: bool,
    /// Queue every library track matching this text (title, artist, album or path), after
    /// any files given. Scan first with `openamp-cli library scan`.
    #[arg(long)]
    search: Option<String>,
    /// Files or folders to play.
    files: Vec<PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum AudioArg {
    Device,
    Null,
}

/// Keeps the audio output alive.
enum Output {
    Device(#[allow(dead_code)] AudioOutput),
    Null(#[allow(dead_code)] NullSink),
}

/// Progress from the background library scan.
enum ScanMsg {
    Progress(usize, usize),
    Done(Result<amp_library::ScanReport, String>),
}

/// How files chosen in the dialog are used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickMode {
    /// Eject: replace the playlist and play.
    Replace,
    /// Playlist ADD: append.
    Append,
}

struct Win {
    id: WindowId,
    panel: PanelId,
    /// Rectangle last sent to the platform.
    applied: Rect,
    visible: bool,
    frame: Option<Bitmap>,
}

struct App {
    platform: X11Platform,
    skin: Skin,
    font: SystemFont,
    wins: Vec<Win>,
    main: MainWindow,
    eq: EqWindow,
    pl: PlaylistWindow,
    lib: LibraryWindow,
    library: Option<Library>,
    db_path: PathBuf,
    config: Config,
    skin_path: Option<PathBuf>,
    playlist_file: PathBuf,
    /// What was last written, to skip unchanged saves.
    saved_config: String,
    saved_playlist: String,
    last_save: Instant,
    last_title_press: Option<(PanelId, Instant)>,
    #[cfg(target_os = "linux")]
    mpris: Option<amp_app::mpris::Mpris>,
    /// Last position reported over MPRIS and when, to detect jumps.
    mpris_position: (Option<TrackId>, Duration, Instant),
    /// Paths of the rows shown in the library window, by library id.
    lib_paths: HashMap<i64, PathBuf>,
    /// Messages from a running library scan.
    scan: Option<Receiver<ScanMsg>>,
    folder_picks: (Sender<Vec<PathBuf>>, Receiver<Vec<PathBuf>>),
    layout: Layout,
    drag: DockDrag,
    tracker: MoveTracker<WindowId>,
    scale: i32,
    player: Player,
    controls: Arc<Controls>,
    eq_controls: Arc<EqControls>,
    _output: Output,
    current: Option<TrackId>,
    meta: HashMap<TrackId, TrackMeta>,
    requested: HashSet<TrackId>,
    prober: Prober,
    picks: (Sender<Vec<PathBuf>>, Receiver<Vec<PathBuf>>),
    pick_mode: Option<PickMode>,
    last_dir: Option<PathBuf>,
    dirs: AppDirs,
    last_status: Status,
    analyzer: AnalyzerReader,
    spectrum: Spectrum,
    vis_samples: Vec<f32>,
    last_vis: Instant,
    started: Instant,
    quit: bool,
}

fn file_stem(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn select_mods(m: Modifiers) -> SelectMods {
    SelectMods {
        shift: m.shift,
        ctrl: m.ctrl,
    }
}

impl App {
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn scaled(&self, p: Point) -> Point {
        Point::new(p.x / self.scale, p.y / self.scale)
    }

    fn panel_of(&self, w: WindowId) -> Option<PanelId> {
        self.wins.iter().find(|x| x.id == w).map(|x| x.panel)
    }

    fn win(&mut self, panel: PanelId) -> Option<&mut Win> {
        self.wins.iter_mut().find(|w| w.panel == panel)
    }

    /// Moves and resizes windows whose layout rectangle changed.
    fn sync_windows(&mut self) -> Result<()> {
        for w in &mut self.wins {
            let Some(r) = self.layout.rect(w.panel) else {
                continue;
            };
            if r == w.applied {
                continue;
            }
            if (r.w, r.h) != (w.applied.w, w.applied.h) {
                self.platform.resize_window(w.id, Size::new(r.w, r.h))?;
                w.frame = None;
            }
            if (r.x, r.y) != (w.applied.x, w.applied.y) {
                self.tracker.requested(w.id, Point::new(r.x, r.y));
                self.platform.move_window(w.id, Point::new(r.x, r.y))?;
            }
            w.applied = r;
        }
        Ok(())
    }

    fn set_panel_visible(&mut self, panel: PanelId, visible: bool) -> Result<()> {
        self.layout.set_visible(panel, visible);
        if visible {
            self.layout.ensure_reachable();
        }
        self.sync_windows()?;
        if let Some(w) = self.win(panel) {
            w.visible = visible;
            w.frame = None;
            let id = w.id;
            self.platform.set_visible(id, visible)?;
        }
        match panel {
            EQ => self.main.set_equalizer_visible(visible),
            PL => self.main.set_playlist_visible(visible),
            _ => {}
        }
        Ok(())
    }

    /// Title for a queued track: tags from playback or probing, else the file name.
    fn title_for(&self, id: TrackId, path: &std::path::Path) -> String {
        self.meta
            .get(&id)
            .and_then(|m| m.title.clone())
            .unwrap_or_else(|| file_stem(path))
    }

    fn update(&mut self) {
        for m in self.prober.drain() {
            self.meta.insert(m.id, m);
        }
        let status = self.player.status();
        if status.track.is_some() {
            self.current = status.track;
        }
        // Prefer what the decoder reports for the playing track.
        if let (Some(id), Some(info)) = (status.track, status.info.as_deref()) {
            let title = match (&info.artist, &info.title) {
                (Some(a), Some(t)) => Some(format!("{a} - {t}")),
                (None, Some(t)) => Some(t.clone()),
                _ => None,
            };
            let entry = self.meta.entry(id).or_insert(TrackMeta {
                id,
                title: None,
                duration: None,
            });
            entry.title = entry.title.take().or(title);
            entry.duration = entry.duration.or(info.duration);
        }

        // Playlist entries from the queue; probe anything new.
        let queued: Vec<(TrackId, PathBuf)> = self
            .player
            .with_queue(|q| q.entries().iter().map(|e| (e.id, e.path.clone())).collect());
        for (id, path) in &queued {
            if self.requested.insert(*id) {
                self.prober.request(*id, path.clone());
            }
        }
        let entries: Vec<PlaylistEntry> = queued
            .iter()
            .map(|(id, path)| PlaylistEntry {
                id: *id,
                title: self.title_for(*id, path),
                duration: self.meta.get(id).and_then(|m| m.duration),
            })
            .collect();
        if entries != self.pl.entries() {
            self.pl.set_entries(entries);
        }
        self.pl.set_current(self.current);

        let shown = self.current.or_else(|| queued.first().map(|(id, _)| *id));
        let position = shown.and_then(|id| queued.iter().position(|(q, _)| *q == id));
        let info = status.info.as_deref();
        self.main.now = NowPlaying {
            state: match status.state {
                PlaybackState::Playing => PlayState::Playing,
                PlaybackState::Paused => PlayState::Paused,
                PlaybackState::Stopped => PlayState::Stopped,
            },
            position: status.position,
            duration: info.and_then(|i| i.duration),
            title: match (shown, position) {
                (Some(id), Some(p)) => self.title_for(id, &queued[p].1),
                _ => String::new(),
            },
            track_number: position.map(|p| p + 1),
            kbps: info.and_then(|i| i.bit_rate).map(|b| (b / 1000) as u32),
            khz: info.map(|i| (i.sample_rate + 500) / 1000),
            channels: info.map_or(0, |i| i.channels),
        };
        self.pl.now = self.main.now.clone();
        self.update_vis(status.state);
        self.last_status = status;
    }

    fn update_vis(&mut self, state: PlaybackState) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_vis).as_secs_f32() * 1000.0;
        self.last_vis = now;
        self.analyzer.update();
        if state == PlaybackState::Playing {
            self.analyzer.latest(&mut self.vis_samples);
            let n = self.vis_samples.len();
            let levels = self.spectrum.bars(&self.vis_samples[n - FFT_SIZE..]);
            self.main
                .vis
                .update(&levels, &self.vis_samples[n - 576..], dt);
        } else {
            if state == PlaybackState::Stopped {
                self.analyzer.clear();
            }
            self.main.vis.update(&[0.0; amp_dsp::BARS], &[], dt);
        }
    }

    fn redraw(&mut self, force: bool) -> Result<()> {
        let now_ms = self.now_ms();
        for i in 0..self.wins.len() {
            if !self.wins[i].visible {
                continue;
            }
            let scale = self.scale as u32;
            let frame = match self.wins[i].panel {
                MAIN => self.main.render(&self.skin, &mut self.font, now_ms, scale),
                EQ => self.eq.render(&self.skin, scale),
                LIB => self.lib.render(&self.skin, &mut self.font, scale),
                _ => self.pl.render(&self.skin, &mut self.font, now_ms, scale),
            };
            let w = &mut self.wins[i];
            if force || w.frame.as_ref() != Some(&frame) {
                let size = Size::new(frame.width() as i32, frame.height() as i32);
                self.platform.present(w.id, size, frame.pixels())?;
                w.frame = Some(frame);
            }
        }
        Ok(())
    }

    fn open_dialog(&mut self, mode: PickMode) {
        if self.pick_mode.is_some() {
            return;
        }
        let start = self.last_dir.clone().or_else(|| self.dirs.music.clone());
        if amp_app::dialog::pick_files(start, self.picks.0.clone()) {
            self.pick_mode = Some(mode);
        }
    }

    fn picked(&mut self, paths: Vec<PathBuf>) -> Result<()> {
        let mode = self.pick_mode.take();
        if paths.is_empty() {
            return Ok(());
        }
        self.last_dir = paths[0].parent().map(PathBuf::from);
        let files = amp_app::files::expand(&paths);
        match mode {
            Some(PickMode::Replace) => {
                self.player.stop()?;
                self.player.with_queue(|q| q.clear());
                let ids = self.player.enqueue(files);
                if let Some(first) = ids.first() {
                    self.player.play_track(*first)?;
                }
            }
            _ => {
                self.player.enqueue(files);
            }
        }
        Ok(())
    }

    fn act(&mut self, action: Action) -> Result<()> {
        let status = &self.last_status;
        match action {
            Action::Play => self.player.play()?,
            Action::Pause => self.player.toggle_pause(),
            Action::Stop => self.player.stop()?,
            Action::Next => self.player.next()?,
            Action::Previous => self.player.prev()?,
            Action::Eject => self.open_dialog(PickMode::Replace),
            Action::Seek(f) => {
                if let Some(d) = status.info.as_ref().and_then(|i| i.duration) {
                    self.player.seek(d.mul_f32(f.clamp(0.0, 1.0)))?;
                }
            }
            Action::SeekBy(secs) => {
                if status.state != PlaybackState::Stopped {
                    let delta = Duration::from_secs(u64::from(secs.unsigned_abs()));
                    let pos = if secs < 0 {
                        status.position.saturating_sub(delta)
                    } else {
                        status.position + delta
                    };
                    self.player.seek(pos)?;
                }
            }
            Action::SetVolume(v) => self.controls.set_volume(v),
            Action::SetBalance(b) => self.controls.set_balance(b),
            Action::SetShuffle(on) => self.player.with_queue(|q| q.set_shuffle(on)),
            Action::SetRepeat(on) => self
                .player
                .with_queue(|q| q.set_repeat(if on { Repeat::All } else { Repeat::Off })),
            Action::SetEqualizerVisible(on) => self.set_panel_visible(EQ, on)?,
            Action::SetPlaylistVisible(on) => self.set_panel_visible(PL, on)?,
            Action::ToggleDoubleSize => self.set_scale(if self.scale == 1 { 2 } else { 1 })?,
            Action::ToggleShade => self.toggle_shade(MAIN)?,
            Action::Minimize => {
                if let Some(id) = self.wins.iter().find(|w| w.panel == MAIN).map(|w| w.id) {
                    self.platform.minimize(id)?;
                }
            }
            Action::Close => self.quit = true,
            other => log::info!("{other:?}: not implemented yet"),
        }
        Ok(())
    }

    fn act_playlist(&mut self, action: PlaylistAction) -> Result<()> {
        match action {
            PlaylistAction::PlayEntry(id) => self.player.play_track(id)?,
            PlaylistAction::Remove(ids) => self.player.with_queue(|q| {
                for id in ids {
                    q.remove(id);
                }
            }),
            PlaylistAction::AddFiles => self.open_dialog(PickMode::Append),
            PlaylistAction::Close => self.set_panel_visible(PL, false)?,
            PlaylistAction::ToggleShade => self.toggle_shade(PL)?,
            PlaylistAction::Resize(w, h) => {
                self.layout
                    .resize(PL, amp_dock::Size::new(w * self.scale, h * self.scale));
                self.sync_windows()?;
            }
            PlaylistAction::Transport(a) => self.act(a)?,
        }
        Ok(())
    }

    fn act_eq(&mut self, action: EqAction) -> Result<()> {
        match action {
            EqAction::SetEnabled(on) => self.eq_controls.set_enabled(on),
            EqAction::SetPreamp(db) => self.eq_controls.set_preamp_db(db),
            EqAction::SetBand(i, db) => self.eq_controls.set_band_db(i, db),
            EqAction::Close => self.set_panel_visible(EQ, false)?,
            EqAction::ToggleShade => self.toggle_shade(EQ)?,
            other => log::info!("{other:?}: not implemented yet"),
        }
        Ok(())
    }

    /// Applies the skin's region.txt shapes (main and equalizer) at the current scale.
    fn apply_shapes(&mut self) -> Result<()> {
        let regions = self.skin.regions().clone();
        let shapes = [
            (
                MAIN,
                self.main.shaded(),
                &regions.normal,
                &regions.window_shade,
            ),
            (
                EQ,
                self.eq.shaded,
                &regions.equalizer,
                &regions.equalizer_shade,
            ),
        ];
        for (panel, shaded, normal, shade) in shapes {
            let Some(id) = self.wins.iter().find(|w| w.panel == panel).map(|w| w.id) else {
                continue;
            };
            let (polys, height) = if shaded {
                (shade, 14)
            } else {
                (normal, HEIGHT)
            };
            let rects = amp_skin::region::shape_rects(polys, WIDTH, height, self.scale).map(|rs| {
                rs.into_iter()
                    .map(|r| Rect::new(r.x, r.y, r.w, r.h))
                    .collect::<Vec<_>>()
            });
            self.platform.set_shape(id, rects.as_deref())?;
        }
        Ok(())
    }

    /// Collapses or expands a window to its window-shade bar. Docked windows below follow.
    fn toggle_shade(&mut self, panel: PanelId) -> Result<()> {
        let (w, h) = match panel {
            MAIN => {
                self.main.set_shaded(!self.main.shaded());
                (WIDTH, self.main.height())
            }
            EQ => {
                self.eq.shaded = !self.eq.shaded;
                (WIDTH, self.eq.height())
            }
            PL => {
                self.pl.shaded = !self.pl.shaded;
                (self.pl.size().0, self.pl.height())
            }
            _ => return Ok(()),
        };
        self.layout
            .resize(panel, amp_dock::Size::new(w * self.scale, h * self.scale));
        self.sync_windows()?;
        self.apply_shapes()
    }

    fn shaded(&self, panel: PanelId) -> bool {
        match panel {
            MAIN => self.main.shaded(),
            EQ => self.eq.shaded,
            PL => self.pl.shaded,
            _ => false,
        }
    }

    fn set_scale(&mut self, scale: i32) -> Result<()> {
        self.layout.rescale(self.scale, scale);
        self.scale = scale;
        self.main.set_double_size(scale > 1);
        self.sync_windows()?;
        self.apply_shapes()?;
        self.redraw(true)
    }

    /// Brings every visible window of ours to the front, `focused` last so it ends on top,
    /// as the classic player does when any of its windows is activated.
    fn raise_all(&mut self, focused: WindowId) -> Result<()> {
        let ids: Vec<WindowId> = self
            .wins
            .iter()
            .filter(|w| w.visible && w.id != focused)
            .map(|w| w.id)
            .collect();
        for id in ids {
            self.platform.raise(id)?;
        }
        self.platform.raise(focused)?;
        Ok(())
    }

    /// Writes settings, window layout and the playlist, skipping files that did not change.
    /// `quitting` also records where Play should resume.
    fn save_state(&mut self, quitting: bool) {
        self.last_save = Instant::now();
        let window = |panel: PanelId, size: Option<(i32, i32)>| {
            let w = self.wins.iter().find(|w| w.panel == panel)?;
            let r = self.layout.rect(panel)?;
            Some(WindowState {
                x: r.x,
                y: r.y,
                visible: w.visible,
                width: size.map(|s| s.0),
                height: size.map(|s| s.1),
                shaded: self.shaded(panel),
            })
        };
        let mut c = self.config.clone();
        c.windows.main = window(MAIN, None);
        c.windows.equalizer = window(EQ, None);
        c.windows.playlist = window(PL, Some(self.pl.size()));
        c.windows.library = window(LIB, Some(self.lib.size()));
        c.player.volume = self.controls.volume();
        c.player.balance = self.controls.balance();
        c.player.shuffle = self.main.shuffle();
        c.player.repeat = self.main.repeat();
        c.player.scale = self.scale as u8;
        c.player.skin = self.skin_path.clone();
        c.player.show_remaining = self.main.show_remaining();
        c.player.visualizer = match self.main.vis.mode {
            VisMode::Spectrum => "spectrum",
            VisMode::Oscilloscope => "oscilloscope",
            VisMode::Off => "off",
        }
        .into();
        let eq = self.eq_controls.settings();
        c.equalizer.enabled = eq.enabled;
        c.equalizer.preamp_db = eq.preamp_db;
        c.equalizer.bands_db = eq.bands_db;
        c.equalizer.auto = self.eq.auto;
        let queued: Vec<(TrackId, PathBuf)> = self
            .player
            .with_queue(|q| q.entries().iter().map(|e| (e.id, e.path.clone())).collect());
        if quitting || self.current.is_some() {
            c.player.current_track = self
                .current
                .and_then(|id| queued.iter().position(|(q, _)| *q == id));
        }
        self.config = c;

        if let Ok(text) = toml::to_string_pretty(&self.config)
            && text != self.saved_config
        {
            match self.config.save(&self.dirs.config_file()) {
                Ok(()) => self.saved_config = text,
                Err(e) => log::warn!("{e}"),
            }
        }
        let items: Vec<amp_library::playlist::PlaylistItem> = queued
            .iter()
            .map(|(id, path)| amp_library::playlist::PlaylistItem {
                path: path.clone(),
                title: self.meta.get(id).and_then(|m| m.title.clone()),
                duration: self.meta.get(id).and_then(|m| m.duration),
            })
            .collect();
        let text = amp_library::playlist::write_m3u(&items);
        if text != self.saved_playlist {
            match amp_library::playlist::save_m3u(&self.playlist_file, &items) {
                Ok(()) => self.saved_playlist = text,
                Err(e) => log::warn!("cannot save playlist: {e}"),
            }
        }
    }

    /// Carries out MPRIS commands and publishes the current state.
    #[cfg(target_os = "linux")]
    fn sync_mpris(&mut self) -> Result<()> {
        use amp_app::mpris::{MprisCommand as C, MprisState, Status};
        let Some(mpris) = &self.mpris else {
            return Ok(());
        };
        let commands = mpris.commands();
        for c in commands {
            let status = self.last_status.clone();
            match c {
                C::Play => self.player.play()?,
                C::Pause => {
                    if status.state == PlaybackState::Playing {
                        self.player.pause();
                    }
                }
                C::PlayPause => {
                    if status.state == PlaybackState::Stopped {
                        self.player.play()?;
                    } else {
                        self.player.toggle_pause();
                    }
                }
                C::Stop => self.player.stop()?,
                C::Next => self.player.next()?,
                C::Previous => self.player.prev()?,
                C::Seek(us) => {
                    if status.state != PlaybackState::Stopped {
                        let target = status.position.as_micros() as i64 + us;
                        let length = status.info.as_ref().and_then(|i| i.duration);
                        if length.is_some_and(|l| target >= l.as_micros() as i64) {
                            self.player.next()?;
                        } else {
                            self.player
                                .seek(Duration::from_micros(target.max(0) as u64))?;
                        }
                    }
                }
                C::SetPosition(id, us) => {
                    if status.track == Some(id) && us >= 0 {
                        self.player.seek(Duration::from_micros(us as u64))?;
                    }
                }
                C::SetVolume(v) => {
                    self.controls.set_volume(v as f32);
                    self.main.set_volume(v as f32);
                }
                C::SetShuffle(on) => {
                    self.main.set_shuffle(on);
                    self.act(Action::SetShuffle(on))?;
                }
                C::SetLoop(on) => {
                    self.main.set_repeat(on);
                    self.act(Action::SetRepeat(on))?;
                }
                C::Raise => {
                    if let Some(id) = self.wins.iter().find(|w| w.panel == MAIN).map(|w| w.id) {
                        self.raise_all(id)?;
                    }
                }
                C::Quit => self.quit = true,
            }
        }

        let status = &self.last_status;
        let info = status.info.as_deref();
        let now = Instant::now();
        let (last_track, last_pos, last_at) = self.mpris_position;
        let expected = if status.state == PlaybackState::Playing {
            last_pos + now.duration_since(last_at)
        } else {
            last_pos
        };
        let drift = status.position.abs_diff(expected);
        let seeked = status.track == last_track
            && status.state != PlaybackState::Stopped
            && drift > Duration::from_millis(1500);
        self.mpris_position = (status.track, status.position, now);
        let meta = status.track.and_then(|id| self.meta.get(&id));
        let state = MprisState {
            status: match status.state {
                PlaybackState::Playing => Status::Playing,
                PlaybackState::Paused => Status::Paused,
                PlaybackState::Stopped => Status::Stopped,
            },
            track_id: status.track,
            title: info
                .and_then(|i| i.title.clone())
                .or_else(|| meta.and_then(|m| m.title.clone()))
                .or_else(|| info.map(|i| file_stem(&i.path))),
            artist: info.and_then(|i| i.artist.clone()),
            album: info.and_then(|i| i.album.clone()),
            length_us: info.and_then(|i| i.duration).map(|d| d.as_micros() as i64),
            position_us: status.position.as_micros() as i64,
            volume: f64::from(self.controls.volume()),
            shuffle: self.main.shuffle(),
            repeat: self.main.repeat(),
        };
        if let Some(mpris) = &self.mpris {
            mpris.update(state, seeked);
        }
        Ok(())
    }

    fn show_library(&mut self, visible: bool) -> Result<()> {
        if visible {
            self.refresh_library();
            let q = self.lib.query();
            self.act_lib(q)?;
        }
        self.set_panel_visible(LIB, visible)
    }

    /// Reloads the tree (counts, artists, albums, genres) from the database.
    fn refresh_library(&mut self) {
        let Some(lib) = &self.library else {
            self.lib.set_status("Library unavailable");
            return;
        };
        let stats = lib.stats().unwrap_or_default();
        self.lib.set_tree(
            stats.tracks,
            lib.artists().unwrap_or_default(),
            lib.albums().unwrap_or_default(),
            lib.genres().unwrap_or_default(),
        );
        if self.scan.is_none() {
            self.lib.set_status(if stats.tracks == 0 {
                "Empty: Add folder or Rescan".to_owned()
            } else {
                format!("{} tracks", stats.tracks)
            });
        }
    }

    /// Scans the library folders on a worker thread, reporting progress.
    fn start_scan(&mut self) {
        if self.scan.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let roots = self.config.library.roots(self.dirs.music.as_deref());
        let db = self.db_path.clone();
        let spawned = std::thread::Builder::new()
            .name("library-scan".into())
            .spawn(move || {
                let progress = tx.clone();
                let result = Library::open(&db)
                    .and_then(|mut lib| {
                        lib.scan(&roots, &amp_library::FfmpegTags, &mut |done, total| {
                            if done % 10 == 0 || done == total {
                                let _ = progress.send(ScanMsg::Progress(done, total));
                            }
                        })
                    })
                    .map_err(|e| e.to_string());
                let _ = tx.send(ScanMsg::Done(result));
            });
        match spawned {
            Ok(_) => {
                self.lib.set_status("Scanning...");
                self.scan = Some(rx);
            }
            Err(e) => log::warn!("cannot start scan: {e}"),
        }
    }

    fn poll_scan(&mut self) -> Result<()> {
        let Some(rx) = &self.scan else {
            return Ok(());
        };
        let mut finished = None;
        let mut status = None;
        for msg in rx.try_iter() {
            match msg {
                ScanMsg::Progress(done, total) => status = Some(format!("Scanning {done}/{total}")),
                ScanMsg::Done(r) => finished = Some(r),
            }
        }
        if let Some(s) = status {
            self.lib.set_status(s);
        }
        let Some(result) = finished else {
            return Ok(());
        };
        self.scan = None;
        self.refresh_library();
        match result {
            Ok(r) => {
                let total = self
                    .library
                    .as_ref()
                    .and_then(|l| l.stats().ok())
                    .map_or(0, |s| s.tracks);
                self.lib.set_status(format!(
                    "{total} tracks ({} new, {} removed)",
                    r.added, r.removed
                ));
            }
            Err(e) => self.lib.set_status(format!("Scan failed: {e}")),
        }
        let q = self.lib.query();
        self.act_lib(q)
    }

    fn lib_paths_for(&self, ids: &[i64]) -> Vec<PathBuf> {
        ids.iter()
            .filter_map(|id| self.lib_paths.get(id).cloned())
            .collect()
    }

    fn act_lib(&mut self, action: LibAction) -> Result<()> {
        match action {
            LibAction::Query { selection, text } => {
                let Some(lib) = &self.library else {
                    return Ok(());
                };
                let mut filter = Filter {
                    text,
                    ..Filter::default()
                };
                match selection {
                    Selection::All => {}
                    Selection::Artist(a) => filter.artist = Some(a),
                    Selection::Album(a) => filter.album = Some(a),
                    Selection::Genre(g) => filter.genre = Some(g),
                }
                let tracks = lib.tracks(&filter, 50_000)?;
                self.lib_paths = tracks.iter().map(|t| (t.id, t.path.clone())).collect();
                let rows = tracks
                    .into_iter()
                    .map(|t| LibraryRow {
                        id: t.id,
                        artist: t.tags.artist.clone().unwrap_or_default(),
                        title: t.tags.title.clone().unwrap_or_else(|| file_stem(&t.path)),
                        album: t.tags.album.clone().unwrap_or_default(),
                        duration: t.tags.duration,
                    })
                    .collect();
                self.lib.set_tracks(rows);
            }
            LibAction::Play { ids, start } => {
                let paths = self.lib_paths_for(&ids);
                if paths.is_empty() {
                    return Ok(());
                }
                self.player.stop()?;
                self.player.with_queue(|q| q.clear());
                let queued = self.player.enqueue(paths);
                if let Some(id) = queued.get(start.min(queued.len().saturating_sub(1))) {
                    self.player.play_track(*id)?;
                }
            }
            LibAction::Enqueue(ids) => {
                let paths = self.lib_paths_for(&ids);
                self.player.enqueue(paths);
            }
            LibAction::AddFolder => {
                let start = self.dirs.music.clone();
                amp_app::dialog::pick_folder(start, self.folder_picks.0.clone());
            }
            LibAction::Rescan => self.start_scan(),
            LibAction::Close => self.set_panel_visible(LIB, false)?,
            LibAction::Resize(w, h) => {
                self.layout
                    .resize(LIB, amp_dock::Size::new(w * self.scale, h * self.scale));
                self.sync_windows()?;
            }
        }
        Ok(())
    }

    fn folder_picked(&mut self, folders: Vec<PathBuf>) -> Result<()> {
        let Some(folder) = folders.into_iter().next() else {
            return Ok(());
        };
        if self.config.library.add(folder)
            && let Err(e) = self.config.save(&self.dirs.config_file())
        {
            log::warn!("{e}");
            self.lib.set_status("Could not save settings");
        }
        self.start_scan();
        Ok(())
    }

    fn interacting(&self, panel: PanelId) -> bool {
        match panel {
            MAIN => self.main.is_interacting(),
            EQ => self.eq.is_interacting(),
            LIB => self.lib.is_interacting(),
            _ => self.pl.is_interacting(),
        }
    }

    fn handle(&mut self, event: Event) -> Result<()> {
        let now_ms = self.now_ms();
        match event {
            Event::Expose { window } => {
                if let Some(w) = self.wins.iter_mut().find(|w| w.id == window) {
                    w.frame = None;
                }
            }
            Event::ButtonDown {
                window,
                button,
                local,
                root,
                mods,
            } => {
                let Some(panel) = self.panel_of(window) else {
                    return Ok(());
                };
                let p = self.scaled(local);
                match button {
                    MouseButton::Left => {
                        let drag_window = match panel {
                            MAIN => {
                                let (down, actions) = self.main.pointer_down(p);
                                for a in actions {
                                    self.act(a)?;
                                }
                                down == PointerDown::DragWindow
                            }
                            EQ => {
                                let (down, actions) = self.eq.pointer_down(p);
                                for a in actions {
                                    self.act_eq(a)?;
                                }
                                down == EqDown::DragWindow
                            }
                            LIB => {
                                let (down, actions) =
                                    self.lib.pointer_down(p, mods.ctrl, mods.shift, now_ms);
                                for a in actions {
                                    self.act_lib(a)?;
                                }
                                down == LibDown::DragWindow
                            }
                            _ => {
                                let (down, actions) =
                                    self.pl.pointer_down(p, select_mods(mods), now_ms);
                                for a in actions {
                                    self.act_playlist(a)?;
                                }
                                down == PlDown::DragWindow
                            }
                        };
                        // A second press on a title bar within 400 ms toggles window shade.
                        let double = drag_window
                            && self.last_title_press.is_some_and(|(p, at)| {
                                p == panel && at.elapsed() < Duration::from_millis(400)
                            });
                        if double && panel != LIB {
                            self.last_title_press = None;
                            self.toggle_shade(panel)?;
                        } else if drag_window {
                            self.last_title_press = Some((panel, Instant::now()));
                            self.drag.press(&self.layout, panel, root);
                        }
                    }
                    MouseButton::WheelUp | MouseButton::WheelDown => {
                        let up = button == MouseButton::WheelUp;
                        if panel == PL {
                            self.pl.wheel(up);
                        } else if panel == LIB {
                            self.lib.wheel(p, up);
                        } else {
                            for a in self.main.wheel(up) {
                                self.act(a)?;
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Motion { window, local, .. } => {
                if self.drag.is_dragging() {
                    let pointer = self.platform.pointer()?;
                    self.drag.motion(&mut self.layout, pointer);
                    self.sync_windows()?;
                } else if let Some(panel) = self.panel_of(window)
                    && self.interacting(panel)
                {
                    let p = self.scaled(local);
                    match panel {
                        MAIN => {
                            for a in self.main.pointer_move(p) {
                                self.act(a)?;
                            }
                        }
                        EQ => {
                            for a in self.eq.pointer_move(p) {
                                self.act_eq(a)?;
                            }
                        }
                        LIB => {
                            for a in self.lib.pointer_move(p) {
                                self.act_lib(a)?;
                            }
                        }
                        _ => {
                            for a in self.pl.pointer_move(p) {
                                self.act_playlist(a)?;
                            }
                        }
                    }
                }
            }
            Event::ButtonUp {
                window,
                button: MouseButton::Left,
                local,
                root,
            } => {
                if self.drag.is_dragging() {
                    self.drag.release(&mut self.layout, root);
                    self.sync_windows()?;
                } else if let Some(panel) = self.panel_of(window) {
                    let p = self.scaled(local);
                    match panel {
                        MAIN => {
                            for a in self.main.pointer_up(p) {
                                self.act(a)?;
                            }
                        }
                        EQ => {
                            for a in self.eq.pointer_up(p) {
                                self.act_eq(a)?;
                            }
                        }
                        LIB => {
                            for a in self.lib.pointer_up(p) {
                                self.act_lib(a)?;
                            }
                        }
                        _ => {
                            for a in self.pl.pointer_up(p) {
                                self.act_playlist(a)?;
                            }
                        }
                    }
                }
            }
            Event::Key {
                keysym: keys::ESCAPE,
                pressed: true,
                ..
            } if self.drag.is_dragging() => {
                self.drag.cancel(&mut self.layout);
                self.sync_windows()?;
            }
            Event::Key {
                window,
                keysym,
                pressed: true,
                mods,
            } => {
                let mut handled = false;
                if mods.alt && (keysym == 0x6c || keysym == 0x4c) {
                    let visible = self.wins.iter().any(|w| w.panel == LIB && w.visible);
                    return self.show_library(!visible);
                }
                if self.panel_of(window) == Some(LIB) {
                    let actions = self.lib.key(keysym, mods.ctrl);
                    handled = true;
                    for a in actions {
                        self.act_lib(a)?;
                    }
                }
                if self.panel_of(window) == Some(PL) {
                    let before = self.pl.selected();
                    let actions = self.pl.key(keysym, select_mods(mods));
                    handled = !actions.is_empty() || self.pl.selected() != before;
                    for a in actions {
                        self.act_playlist(a)?;
                    }
                }
                if !handled {
                    for a in self.main.key(keysym) {
                        self.act(a)?;
                    }
                }
            }
            Event::Focus { window, focused } => {
                match self.panel_of(window) {
                    Some(MAIN) => self.main.focused = focused,
                    Some(EQ) => self.eq.focused = focused,
                    Some(LIB) => self.lib.focused = focused,
                    Some(_) => self.pl.focused = focused,
                    None => {}
                }
                if focused {
                    self.raise_all(window)?;
                }
            }
            Event::CloseRequested { .. } => self.quit = true,
            Event::Configured { window, rect } => {
                self.tracker.observed(window, Point::new(rect.x, rect.y));
            }
            _ => {}
        }
        // Moves the window manager made on its own: shift the docked group to match.
        if !self.drag.is_dragging() {
            let overrides = self.tracker.take_overrides();
            for (window, origin) in overrides {
                if let Some(panel) = self.panel_of(window) {
                    self.layout.moved_externally(panel, origin);
                    if let Some(w) = self.wins.iter_mut().find(|w| w.id == window)
                        && let Some(r) = self.layout.rect(panel)
                    {
                        w.applied = r;
                    }
                }
            }
            self.sync_windows()?;
        }
        Ok(())
    }

    fn frame_timeout(&self) -> Duration {
        let animating = self.main.now.state != PlayState::Stopped
            || self.main.vis.is_active()
            || self.drag.is_dragging()
            || [MAIN, EQ, PL].iter().any(|p| self.interacting(*p))
            || self.pick_mode.is_some()
            || self.scan.is_some();
        Duration::from_millis(if animating { 33 } else { 250 })
    }

    fn run(&mut self) -> Result<()> {
        while !self.quit {
            if let Some(event) = self.platform.next_event(Some(self.frame_timeout()))? {
                self.handle(event)?;
            }
            if let Ok(paths) = self.picks.1.try_recv() {
                self.picked(paths)?;
            }
            if let Ok(folders) = self.folder_picks.1.try_recv() {
                self.folder_picked(folders)?;
            }
            self.poll_scan()?;
            #[cfg(target_os = "linux")]
            self.sync_mpris()?;
            if self.last_save.elapsed() > Duration::from_secs(30) {
                self.save_state(false);
            }
            self.update();
            self.redraw(false)?;
            self.platform.flush()?;
        }
        Ok(())
    }
}

fn find_skin(arg: Option<PathBuf>) -> Result<Skin> {
    let path = match arg {
        Some(p) => p,
        None => {
            let p = PathBuf::from("skins/base-2.91.wsz");
            if !p.is_file() {
                bail!("no skin found: pass --skin PATH (a .wsz file or skin folder)");
            }
            p
        }
    };
    let skin = Skin::load(&path).with_context(|| format!("loading skin {}", path.display()))?;
    for w in skin.warnings() {
        log::warn!("skin: {w}");
    }
    Ok(skin)
}

fn main() -> Result<()> {
    amp_app::logging::install();
    let args = Args::parse();
    let dirs = AppDirs::system();
    log::info!(
        "config {}, data {}, music {}",
        dirs.config.display(),
        dirs.data.display(),
        dirs.music
            .as_ref()
            .map_or("(none)".into(), |m| m.display().to_string())
    );
    let config = Config::load(&dirs.config_file()).unwrap_or_else(|e| {
        log::warn!("{e}; using default settings");
        Config::default()
    });
    let skin_path = args.skin.clone().or_else(|| config.player.skin.clone());
    let (skin, skin_path) = match find_skin(skin_path.clone()) {
        Ok(s) => (s, skin_path),
        Err(e) if args.skin.is_none() && skin_path.is_some() => {
            log::warn!("saved skin unavailable ({e}); trying the default");
            (find_skin(None)?, None)
        }
        Err(e) => return Err(e),
    };
    let scale = i32::from(args.scale.unwrap_or(config.player.scale).clamp(1, 3));

    let eq_controls = EqControls::new();
    let (mut tap, analyzer) = amp_dsp::analyzer();
    let device = match args.audio {
        AudioArg::Device => {
            Some(OutputDevice::open(args.device.as_deref()).context("opening audio device")?)
        }
        AudioArg::Null => None,
    };
    let rate = device.as_ref().map_or(44_100, OutputDevice::sample_rate);
    let (player, mut source) = Player::new(PlayerConfig {
        sample_rate: rate,
        ..PlayerConfig::default()
    })?;
    let (output, controls) = match device {
        Some(d) => {
            let mut eq = Equalizer::new(rate, Arc::clone(&eq_controls));
            let out = d.start(move |buf| {
                source.fill(buf);
                eq.process(buf);
                tap.push(buf);
            })?;
            let controls = Arc::clone(out.controls());
            (Output::Device(out), controls)
        }
        None => {
            let mut eq = Equalizer::new(rate, Arc::clone(&eq_controls));
            let sink = NullSink::start_with(source, true, move |buf| {
                eq.process(buf);
                tap.push(buf);
            })?;
            let controls = Arc::clone(sink.controls());
            (Output::Null(sink), controls)
        }
    };
    controls.set_volume(match args.volume {
        Some(v) => f32::from(v) / 100.0,
        None => config.player.volume,
    });
    controls.set_balance(config.player.balance);
    eq_controls.set(&amp_dsp::EqSettings {
        enabled: config.equalizer.enabled,
        preamp_db: config.equalizer.preamp_db,
        bands_db: config.equalizer.bands_db,
    });
    let db_path = dirs.data.join("library.sqlite3");
    let library = match Library::open(&db_path) {
        Ok(l) => Some(l),
        Err(e) => {
            log::warn!("library unavailable: {e}");
            None
        }
    };
    let mut files = amp_app::files::expand(&args.files);
    if let (Some(text), Some(lib)) = (&args.search, &library) {
        let found = lib.search(text, 10_000)?;
        log::info!("library search {text:?}: {} tracks", found.len());
        files.extend(found.into_iter().map(|t| t.path));
    }
    let have_files = !files.is_empty();
    let playlist_file = dirs.data.join("playlist.m3u8");
    let restored = if have_files {
        Vec::new()
    } else {
        amp_library::playlist::read_playlist(&playlist_file).unwrap_or_default()
    };
    let last_dir = files.first().and_then(|f| f.parent()).map(PathBuf::from);
    player.enqueue(files);
    let mut restored_current = None;
    if !restored.is_empty() {
        let ids = player.enqueue(restored);
        if let Some(id) = config.player.current_track.and_then(|i| ids.get(i)) {
            player.select(*id)?;
            restored_current = Some(*id);
        }
    }
    player.with_queue(|q| {
        q.set_repeat(if config.player.repeat {
            Repeat::All
        } else {
            Repeat::Off
        });
        q.set_shuffle(config.player.shuffle);
    });

    let mut platform = X11Platform::connect().context("connecting to the display")?;
    let screens = platform.screens()?;
    let origin = screens
        .first()
        .map_or(Point::new(100, 100), |s| Point::new(s.x + 100, s.y + 100));

    let saved = |panel: PanelId| match panel {
        MAIN => config.windows.main,
        EQ => config.windows.equalizer,
        PL => config.windows.playlist,
        _ => config.windows.library,
    };
    let mut pl = PlaylistWindow::new();
    if let Some(WindowState {
        width: Some(w),
        height: Some(h),
        ..
    }) = saved(PL)
    {
        pl.set_size(w, h);
    }
    let (pl_w, pl_h) = pl.size();
    let mut lib_win = LibraryWindow::new();
    if let Some(WindowState {
        width: Some(w),
        height: Some(h),
        ..
    }) = saved(LIB)
    {
        lib_win.set_size(w, h);
    }
    let (lib_w, lib_h) = lib_win.size();
    let rects = [
        (
            MAIN,
            "OpenAMP",
            WindowKind::Main,
            Rect::new(0, 0, WIDTH, HEIGHT),
        ),
        (
            EQ,
            "Equalizer",
            WindowKind::Panel,
            Rect::new(0, HEIGHT, WIDTH, HEIGHT),
        ),
        (
            PL,
            "Playlist",
            WindowKind::Panel,
            Rect::new(0, 2 * HEIGHT, pl_w, pl_h),
        ),
        (
            LIB,
            "Media Library",
            WindowKind::Panel,
            Rect::new(WIDTH, 0, lib_w, lib_h),
        ),
    ];
    let mut layout = Layout::new();
    layout.set_snap_distance(10 * scale);
    layout.set_min_visible(amp_dock::Size::new(28 * scale, 14 * scale));
    let mut wins = Vec::new();
    for (panel, title, kind, r) in rects {
        // Saved positions are desktop coordinates; otherwise the classic stack.
        let state = saved(panel);
        // Shaded windows start as their 14 px bar (the library has no shade mode).
        let h = if state.is_some_and(|s| s.shaded) && panel != LIB {
            14
        } else {
            r.h
        };
        let rect = match state {
            Some(s) => Rect::new(s.x, s.y, r.w * scale, h * scale),
            None => Rect::new(
                origin.x + r.x * scale,
                origin.y + r.y * scale,
                r.w * scale,
                r.h * scale,
            ),
        };
        let visible = state.map_or(panel != LIB, |s| s.visible || panel == MAIN);
        // Panels belong to the main window, so the window manager raises them with it.
        let owner = wins.first().map(|w: &Win| w.id);
        let id = platform.create_window(&WindowSpec {
            title: title.into(),
            rect,
            kind,
            owner,
        })?;
        layout.insert(panel, rect);
        wins.push(Win {
            id,
            panel,
            applied: rect,
            visible,
            frame: None,
        });
    }
    layout.set_leader(Some(MAIN));
    layout.set_screens(screens);
    pl.focused = false;

    let shown = |panel: PanelId| wins.iter().any(|w| w.panel == panel && w.visible);
    let mut main = MainWindow::new();
    main.set_volume(controls.volume());
    main.set_balance(controls.balance());
    main.set_double_size(scale > 1);
    main.set_equalizer_visible(shown(EQ));
    main.set_playlist_visible(shown(PL));
    main.set_shuffle(config.player.shuffle);
    main.set_repeat(config.player.repeat);
    main.set_show_remaining(config.player.show_remaining);
    main.vis.mode = match config.player.visualizer.as_str() {
        "oscilloscope" => VisMode::Oscilloscope,
        "off" => VisMode::Off,
        _ => VisMode::Spectrum,
    };
    let mut eq_win = EqWindow::new();
    eq_win.enabled = config.equalizer.enabled;
    eq_win.auto = config.equalizer.auto;
    eq_win.preamp_db = config.equalizer.preamp_db;
    eq_win.bands_db = config.equalizer.bands_db;
    let lib_shown = shown(LIB);
    let was_shaded = |panel: PanelId| saved(panel).is_some_and(|s| s.shaded);
    main.set_shaded(was_shaded(MAIN));
    eq_win.shaded = was_shaded(EQ);
    pl.shaded = was_shaded(PL);

    let mut app = App {
        platform,
        skin,
        font: SystemFont::load(),
        wins,
        main,
        eq: eq_win,
        pl,
        lib: lib_win,
        library,
        db_path,
        config,
        skin_path: skin_path.map(|p| amp_app::files::absolute(&p)),
        playlist_file,
        saved_config: String::new(),
        saved_playlist: String::new(),
        last_save: Instant::now(),
        last_title_press: None,
        #[cfg(target_os = "linux")]
        mpris: match amp_app::mpris::Mpris::start() {
            Ok(m) => Some(m),
            Err(e) => {
                log::info!("MPRIS unavailable (no session bus?): {e}");
                None
            }
        },
        mpris_position: (None, Duration::ZERO, Instant::now()),
        lib_paths: HashMap::new(),
        scan: None,
        folder_picks: mpsc::channel(),
        layout,
        drag: DockDrag::new(),
        tracker: MoveTracker::new(),
        scale,
        player,
        controls,
        eq_controls,
        _output: output,
        current: restored_current,
        meta: HashMap::new(),
        requested: HashSet::new(),
        prober: Prober::new(),
        picks: mpsc::channel(),
        pick_mode: None,
        last_dir,
        dirs,
        last_status: Status {
            state: PlaybackState::Stopped,
            track: None,
            position: Duration::ZERO,
            info: None,
        },
        analyzer,
        spectrum: Spectrum::new(rate),
        vis_samples: vec![0.0; FFT_SIZE.max(576)],
        last_vis: Instant::now(),
        started: Instant::now(),
        quit: false,
    };
    for w in &mut app.wins {
        // Force the first sync to place every window exactly.
        w.applied = Rect::new(i32::MIN, i32::MIN, w.applied.w, w.applied.h);
    }
    app.sync_windows()?;
    app.apply_shapes()?;
    for w in app.wins.iter().filter(|w| !w.visible) {
        app.layout.set_visible(w.panel, false);
    }
    app.layout.ensure_reachable();
    app.sync_windows()?;
    for w in app.wins.iter().filter(|w| w.visible) {
        app.platform.set_visible(w.id, true)?;
    }
    if args.library || lib_shown {
        app.show_library(true)?;
    }
    app.platform.flush()?;
    if have_files {
        app.player.play()?;
    }
    let result = app.run();
    app.save_state(true);
    result
}
