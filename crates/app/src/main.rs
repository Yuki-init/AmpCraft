//! The player: main, equalizer and playlist windows wired to the playback engine.
//!
//! `openamp [--skin SKIN] [--scale 1|2|3] [--device NAME] [--audio device|null] [FILES...]`

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use amp_app::metadata::{Prober, TrackMeta};
use amp_app::null_sink::NullSink;
use amp_app::system_font::SystemFont;
use amp_audio_out::{AudioOutput, Controls, OutputDevice};
use amp_core::{PlaybackState, Player, PlayerConfig, Repeat, Status, TrackId};
use amp_dock::{Layout, PanelId, Rect};
use amp_dsp::{EqControls, Equalizer};
use amp_platform::x11::X11Platform;
use amp_platform::{
    Event, Modifiers, MouseButton, Platform, Point, Size, WindowId, WindowKind, WindowSpec, keys,
};
use amp_skin::{Bitmap, Skin};
use amp_ui::main_window::{HEIGHT, WIDTH};
use amp_ui::{
    Action, DockDrag, EqAction, EqDown, EqWindow, MainWindow, MoveTracker, NowPlaying, PlDown,
    PlayState, PlaylistAction, PlaylistEntry, PlaylistWindow, PointerDown, SelectMods,
    scale_nearest,
};
use anyhow::{Context, Result, bail};
use clap::Parser;

const MAIN: PanelId = PanelId(1);
const EQ: PanelId = PanelId(2);
const PL: PanelId = PanelId(3);

#[derive(Parser)]
#[command(version, about = "Classic skinnable audio player")]
struct Args {
    /// Skin file (.wsz) or folder. Defaults to skins/base-2.91.wsz when present.
    #[arg(long)]
    skin: Option<PathBuf>,
    /// Integer UI scale.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=3))]
    scale: u8,
    /// Audio output device (see `openamp-cli devices`).
    #[arg(long)]
    device: Option<String>,
    /// `null` discards audio at real-time pace (headless runs, no sound hardware).
    #[arg(long, value_enum, default_value_t = AudioArg::Device)]
    audio: AudioArg,
    /// Starting volume, 0 to 100.
    #[arg(long, default_value_t = 50)]
    volume: u8,
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
    last_status: Status,
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
        self.last_status = status;
    }

    fn redraw(&mut self, force: bool) -> Result<()> {
        let now_ms = self.now_ms();
        for i in 0..self.wins.len() {
            if !self.wins[i].visible {
                continue;
            }
            let frame = match self.wins[i].panel {
                MAIN => self.main.render(&self.skin, &mut self.font, now_ms),
                EQ => self.eq.render(&self.skin),
                _ => self.pl.render(&self.skin, &mut self.font, now_ms),
            };
            let frame = scale_nearest(&frame, self.scale as u32);
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
        if amp_app::dialog::pick_files(self.last_dir.clone(), self.picks.0.clone()) {
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
            other => log::info!("{other:?}: not implemented yet"),
        }
        Ok(())
    }

    fn set_scale(&mut self, scale: i32) -> Result<()> {
        self.layout.rescale(self.scale, scale);
        self.scale = scale;
        self.main.set_double_size(scale > 1);
        self.sync_windows()?;
        self.redraw(true)
    }

    fn interacting(&self, panel: PanelId) -> bool {
        match panel {
            MAIN => self.main.is_interacting(),
            EQ => self.eq.is_interacting(),
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
                            _ => {
                                let (down, actions) =
                                    self.pl.pointer_down(p, select_mods(mods), now_ms);
                                for a in actions {
                                    self.act_playlist(a)?;
                                }
                                down == PlDown::DragWindow
                            }
                        };
                        if drag_window {
                            self.drag.press(&self.layout, panel, root);
                        }
                    }
                    MouseButton::WheelUp | MouseButton::WheelDown => {
                        let up = button == MouseButton::WheelUp;
                        if panel == PL {
                            self.pl.wheel(up);
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
            Event::Focus { window, focused } => match self.panel_of(window) {
                Some(MAIN) => self.main.focused = focused,
                Some(EQ) => self.eq.focused = focused,
                Some(_) => self.pl.focused = focused,
                None => {}
            },
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
            || self.drag.is_dragging()
            || [MAIN, EQ, PL].iter().any(|p| self.interacting(*p))
            || self.pick_mode.is_some();
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
    let skin = find_skin(args.skin)?;
    let scale = i32::from(args.scale);

    let eq_controls = EqControls::new();
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
            })?;
            let controls = Arc::clone(out.controls());
            (Output::Device(out), controls)
        }
        None => {
            let sink = NullSink::start(source, true)?;
            let controls = Arc::clone(sink.controls());
            (Output::Null(sink), controls)
        }
    };
    controls.set_volume(f32::from(args.volume.min(100)) / 100.0);

    let files = amp_app::files::expand(&args.files);
    let have_files = !files.is_empty();
    let last_dir = files.first().and_then(|f| f.parent()).map(PathBuf::from);
    player.enqueue(files);

    let mut platform = X11Platform::connect().context("connecting to the display")?;
    let screens = platform.screens()?;
    let origin = screens
        .first()
        .map_or(Point::new(100, 100), |s| Point::new(s.x + 100, s.y + 100));

    let mut pl = PlaylistWindow::new();
    let (pl_w, pl_h) = pl.size();
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
    ];
    let mut layout = Layout::new();
    layout.set_snap_distance(10 * scale);
    layout.set_min_visible(amp_dock::Size::new(28 * scale, 14 * scale));
    let mut wins = Vec::new();
    for (panel, title, kind, r) in rects {
        let rect = Rect::new(
            origin.x + r.x * scale,
            origin.y + r.y * scale,
            r.w * scale,
            r.h * scale,
        );
        let id = platform.create_window(&WindowSpec {
            title: title.into(),
            rect,
            kind,
        })?;
        layout.insert(panel, rect);
        wins.push(Win {
            id,
            panel,
            applied: rect,
            visible: true,
            frame: None,
        });
    }
    layout.set_leader(Some(MAIN));
    layout.set_screens(screens);
    pl.focused = false;

    let mut main = MainWindow::new();
    main.set_volume(controls.volume());
    main.set_double_size(scale > 1);
    main.set_equalizer_visible(true);
    main.set_playlist_visible(true);

    let mut app = App {
        platform,
        skin,
        font: SystemFont::load(),
        wins,
        main,
        eq: EqWindow::new(),
        pl,
        layout,
        drag: DockDrag::new(),
        tracker: MoveTracker::new(),
        scale,
        player,
        controls,
        eq_controls,
        _output: output,
        current: None,
        meta: HashMap::new(),
        requested: HashSet::new(),
        prober: Prober::new(),
        picks: mpsc::channel(),
        pick_mode: None,
        last_dir,
        last_status: Status {
            state: PlaybackState::Stopped,
            track: None,
            position: Duration::ZERO,
            info: None,
        },
        started: Instant::now(),
        quit: false,
    };
    for w in &mut app.wins {
        // Force the first sync to place every window exactly.
        w.applied = Rect::new(i32::MIN, i32::MIN, w.applied.w, w.applied.h);
    }
    app.sync_windows()?;
    for w in &app.wins {
        app.platform.set_visible(w.id, true)?;
    }
    app.platform.flush()?;
    if have_files {
        app.player.play()?;
    }
    app.run()
}
