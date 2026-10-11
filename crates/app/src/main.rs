//! The player: main window wired to the playback engine.
//!
//! `openamp [--skin SKIN] [--scale 1|2|3] [--device NAME] [FILES OR FOLDERS...]`

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use amp_app::null_sink::NullSink;
use amp_app::system_font::SystemFont;
use amp_audio_out::{AudioOutput, Controls, OutputDevice};
use amp_core::{PlaybackState, Player, PlayerConfig, Repeat, Status, TrackId};
use amp_dock::{Layout, PanelId, Rect};
use amp_platform::x11::X11Platform;
use amp_platform::{
    Event, MouseButton, Platform, Point, Size, WindowId, WindowKind, WindowSpec, keys,
};
use amp_skin::{Bitmap, Skin};
use amp_ui::main_window::{HEIGHT, WIDTH};
use amp_ui::{
    Action, DockDrag, MainWindow, MoveTracker, NowPlaying, PlayState, PointerDown, scale_nearest,
};
use anyhow::{Context, Result, bail};
use clap::Parser;

const MAIN: PanelId = PanelId(1);

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

struct App {
    platform: X11Platform,
    skin: Skin,
    font: SystemFont,
    window: WindowId,
    main: MainWindow,
    layout: Layout,
    drag: DockDrag,
    tracker: MoveTracker<WindowId>,
    scale: i32,
    player: Player,
    controls: Arc<Controls>,
    _output: Output,
    /// Last track heard, so a stopped player still shows what Play will start.
    current: Option<TrackId>,
    /// Titles from tags of tracks played so far, shown again when stopped.
    titles: std::collections::HashMap<TrackId, String>,
    last_status: Status,
    frame: Option<Bitmap>,
    started: Instant,
    quit: bool,
}

fn track_title(info: &amp_core::TrackInfo) -> String {
    match (&info.artist, &info.title) {
        (Some(a), Some(t)) => format!("{a} - {t}"),
        (None, Some(t)) => t.clone(),
        _ => file_stem(&info.path),
    }
}

fn file_stem(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl App {
    fn scaled(&self, p: Point) -> Point {
        Point::new(p.x / self.scale, p.y / self.scale)
    }

    fn update_now_playing(&mut self) {
        let status = self.player.status();
        if status.track.is_some() {
            self.current = status.track;
        }
        if let (Some(id), Some(info)) = (status.track, status.info.as_deref()) {
            self.titles.entry(id).or_insert_with(|| track_title(info));
        }
        let (track_number, queued_name) = self.player.with_queue(|q| {
            let id = self.current.or_else(|| q.first());
            let pos = id.and_then(|id| q.position(id));
            let name = id.and_then(|id| {
                self.titles
                    .get(&id)
                    .cloned()
                    .or_else(|| q.get(id).map(|e| file_stem(&e.path)))
            });
            (pos.map(|p| p + 1), name)
        });
        let info = status.info.as_deref();
        self.main.now = NowPlaying {
            state: match status.state {
                PlaybackState::Playing => PlayState::Playing,
                PlaybackState::Paused => PlayState::Paused,
                PlaybackState::Stopped => PlayState::Stopped,
            },
            position: status.position,
            duration: info.and_then(|i| i.duration),
            title: info.map(track_title).or(queued_name).unwrap_or_default(),
            track_number,
            kbps: info.and_then(|i| i.bit_rate).map(|b| (b / 1000) as u32),
            khz: info.map(|i| (i.sample_rate + 500) / 1000),
            channels: info.map_or(0, |i| i.channels),
        };
        self.last_status = status;
    }

    fn redraw(&mut self, force: bool) -> Result<()> {
        let now_ms = self.started.elapsed().as_millis() as u64;
        let frame = self.main.render(&self.skin, &mut self.font, now_ms);
        let frame = scale_nearest(&frame, self.scale as u32);
        if force || self.frame.as_ref() != Some(&frame) {
            let size = Size::new(frame.width() as i32, frame.height() as i32);
            self.platform.present(self.window, size, frame.pixels())?;
            self.frame = Some(frame);
        }
        Ok(())
    }

    fn apply_moves(&mut self, moves: Vec<(PanelId, Rect)>) -> Result<()> {
        for (id, r) in moves {
            if id == MAIN {
                self.tracker.requested(self.window, Point::new(r.x, r.y));
                self.platform
                    .move_window(self.window, Point::new(r.x, r.y))?;
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
            Action::Seek(f) => {
                if let Some(d) = status.info.as_ref().and_then(|i| i.duration) {
                    self.player.seek(d.mul_f32(f.clamp(0.0, 1.0)))?;
                }
            }
            Action::SeekBy(secs) => {
                if status.state != PlaybackState::Stopped {
                    let pos = if secs < 0 {
                        status
                            .position
                            .saturating_sub(Duration::from_secs(secs.unsigned_abs().into()))
                    } else {
                        status.position + Duration::from_secs(secs as u64)
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
            Action::ToggleDoubleSize => self.set_scale(if self.scale == 1 { 2 } else { 1 })?,
            Action::Minimize => self.platform.minimize(self.window)?,
            Action::Close => self.quit = true,
            other => log::info!("{other:?}: not implemented yet"),
        }
        Ok(())
    }

    fn set_scale(&mut self, scale: i32) -> Result<()> {
        self.layout.rescale(self.scale, scale);
        self.scale = scale;
        self.main.set_double_size(scale > 1);
        if let Some(r) = self.layout.rect(MAIN) {
            self.platform
                .resize_window(self.window, Size::new(r.w, r.h))?;
            self.tracker.requested(self.window, Point::new(r.x, r.y));
            self.platform
                .move_window(self.window, Point::new(r.x, r.y))?;
        }
        self.redraw(true)
    }

    fn handle(&mut self, event: Event) -> Result<()> {
        let mut actions = Vec::new();
        let mut moves = Vec::new();
        match event {
            Event::Expose { .. } => return self.redraw(true),
            Event::ButtonDown {
                button: MouseButton::Left,
                local,
                root,
                ..
            } => {
                let (down, a) = self.main.pointer_down(self.scaled(local));
                actions = a;
                if down == PointerDown::DragWindow {
                    self.drag.press(&self.layout, MAIN, root);
                }
            }
            Event::ButtonDown {
                button: MouseButton::WheelUp,
                ..
            } => actions = self.main.wheel(true),
            Event::ButtonDown {
                button: MouseButton::WheelDown,
                ..
            } => actions = self.main.wheel(false),
            Event::Motion { local, .. } => {
                if self.drag.is_dragging() {
                    moves = self.drag.motion(&mut self.layout, self.platform.pointer()?);
                } else if self.main.is_interacting() {
                    actions = self.main.pointer_move(self.scaled(local));
                }
            }
            Event::ButtonUp {
                button: MouseButton::Left,
                local,
                root,
                ..
            } => {
                if self.drag.is_dragging() {
                    moves = self.drag.release(&mut self.layout, root);
                } else {
                    actions = self.main.pointer_up(self.scaled(local));
                }
            }
            Event::Key {
                keysym: keys::ESCAPE,
                pressed: true,
                ..
            } if self.drag.is_dragging() => {
                moves = self.drag.cancel(&mut self.layout);
            }
            Event::Key {
                keysym,
                pressed: true,
                ..
            } => actions = self.main.key(keysym),
            Event::Focus { focused, .. } => self.main.focused = focused,
            Event::CloseRequested { .. } => self.quit = true,
            Event::Configured { window, rect } => {
                self.tracker.observed(window, Point::new(rect.x, rect.y));
            }
            _ => {}
        }
        if !self.drag.is_dragging() {
            for (_, origin) in self.tracker.take_overrides() {
                moves.extend(self.layout.moved_externally(MAIN, origin));
            }
        }
        self.apply_moves(moves)?;
        for a in actions {
            self.act(a)?;
        }
        Ok(())
    }

    /// How long to wait for input before the next frame.
    fn frame_timeout(&self) -> Duration {
        let animating = self.main.now.state != PlayState::Stopped
            || self.main.is_interacting()
            || self.drag.is_dragging();
        if animating {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(250)
        }
    }

    fn run(&mut self) -> Result<()> {
        while !self.quit {
            if let Some(event) = self.platform.next_event(Some(self.frame_timeout()))? {
                self.handle(event)?;
            }
            self.update_now_playing();
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

    let device = match args.audio {
        AudioArg::Device => {
            Some(OutputDevice::open(args.device.as_deref()).context("opening audio device")?)
        }
        AudioArg::Null => None,
    };
    let (player, mut source) = Player::new(PlayerConfig {
        sample_rate: device.as_ref().map_or(44_100, OutputDevice::sample_rate),
        ..PlayerConfig::default()
    })?;
    let (output, controls) = match device {
        Some(d) => {
            let out = d.start(move |buf| {
                source.fill(buf);
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
    player.enqueue(files);

    let mut platform = X11Platform::connect().context("connecting to the display")?;
    let screens = platform.screens()?;
    let origin = screens
        .first()
        .map_or(Point::new(100, 100), |s| Point::new(s.x + 100, s.y + 100));
    let rect = Rect::new(origin.x, origin.y, WIDTH * scale, HEIGHT * scale);
    let window = platform.create_window(&WindowSpec {
        title: "OpenAMP".into(),
        rect,
        kind: WindowKind::Main,
    })?;
    let mut layout = Layout::new();
    layout.set_snap_distance(10 * scale);
    layout.set_min_visible(amp_dock::Size::new(28 * scale, 14 * scale));
    layout.insert(MAIN, rect);
    layout.set_leader(Some(MAIN));
    layout.set_screens(screens);

    let mut main = MainWindow::new();
    main.set_volume(controls.volume());
    main.set_double_size(scale > 1);

    let mut app = App {
        platform,
        skin,
        font: SystemFont::load(),
        window,
        main,
        layout,
        drag: DockDrag::new(),
        tracker: MoveTracker::new(),
        scale,
        player,
        controls,
        _output: output,
        current: None,
        titles: std::collections::HashMap::new(),
        last_status: Status {
            state: PlaybackState::Stopped,
            track: None,
            position: Duration::ZERO,
            info: None,
        },
        frame: None,
        started: Instant::now(),
        quit: false,
    };
    if let Some(r) = app.layout.rect(MAIN) {
        app.tracker.requested(window, Point::new(r.x, r.y));
        app.platform.move_window(window, Point::new(r.x, r.y))?;
    }
    app.platform.set_visible(window, true)?;
    app.platform.flush()?;
    if have_files {
        app.player.play()?;
    }
    app.run()
}
