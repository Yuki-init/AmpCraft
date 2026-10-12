//! Headless player for exercising the engine without the GUI.
//!
//! ```text
//! openamp-cli probe FILE...
//! openamp-cli decode FILE -o OUT.wav [--rate HZ] [--seek SECS] [--length SECS]
//! openamp-cli play FILE... [--sink device|null|fast] [--seek SECS] [--shuffle] [--repeat MODE]
//! ```
//!
//! While playing, type a command and press Enter: `p` pause/resume, `n` next, `b` previous,
//! `s SECS` seek, `v 0-100` volume, `q` quit.

use std::fs::File;
use std::io::{BufRead, BufWriter, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use amp_app::null_sink::NullSink;
use amp_audio_out::OutputDevice;
use amp_core::wav::{WavFormat, write_wav};
use amp_core::{Decoder, OUTPUT_CHANNELS, PlaybackState, Player, PlayerConfig, Repeat};
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(version, about = "Headless player for testing the playback engine")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print stream information and tags.
    Probe { files: Vec<PathBuf> },
    /// List audio output devices.
    Devices,
    /// Show where settings, data and music are looked for on this system.
    Paths,
    /// Manage and search the media library.
    #[command(subcommand)]
    Library(LibraryCmd),
    /// Built-in themes.
    #[command(subcommand)]
    Skin(SkinCmd),
    /// Decode a file to a 32-bit float WAV.
    Decode {
        file: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Output sample rate.
        #[arg(long, default_value_t = 44_100)]
        rate: u32,
        /// Start position in seconds.
        #[arg(long)]
        seek: Option<f64>,
        /// Maximum length to decode, in seconds.
        #[arg(long)]
        length: Option<f64>,
    },
    /// Play files in order.
    Play {
        files: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = Sink::Device)]
        sink: Sink,
        /// Output device id or part of its name (see `devices`). Default: system default.
        #[arg(long)]
        device: Option<String>,
        /// Start position in the first track, in seconds.
        #[arg(long)]
        seek: Option<f64>,
        #[arg(long)]
        shuffle: bool,
        #[arg(long, value_enum, default_value_t = RepeatArg::Off)]
        repeat: RepeatArg,
        /// Volume, 0 to 100.
        #[arg(long, default_value_t = 100)]
        volume: u8,
        /// Balance, -100 (left) to 100 (right).
        #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
        balance: i8,
    },
}

#[derive(Subcommand)]
enum SkinCmd {
    /// List the built-in themes.
    List,
    /// Write a built-in theme as a .wsz skin file.
    Export { theme: String, output: PathBuf },
}

#[derive(Subcommand)]
enum LibraryCmd {
    /// List the folders the library scans.
    Folders,
    /// Add a folder to scan (it is only read, never written).
    Add { folder: PathBuf },
    /// Stop scanning a folder (its tracks are removed at the next scan only if it still exists
    /// elsewhere; otherwise run `scan` after removing).
    Remove { folder: PathBuf },
    /// Scan the folders and update the database.
    Scan,
    /// Find tracks by title, artist, album or path.
    Search {
        text: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Show track, artist and album counts.
    Stats,
}

#[derive(Clone, Copy, ValueEnum)]
enum Sink {
    /// The default audio device.
    Device,
    /// Discard audio at real-time pace.
    Null,
    /// Discard audio as fast as it decodes.
    Fast,
}

#[derive(Clone, Copy, ValueEnum)]
enum RepeatArg {
    Off,
    All,
    One,
}

fn main() -> Result<()> {
    amp_app::logging::install();
    match Cli::parse().command {
        Cmd::Probe { files } => probe(&files),
        Cmd::Library(cmd) => library(cmd),
        Cmd::Skin(SkinCmd::List) => {
            for t in amp_skin::theme::THEMES {
                println!("{:<12} {}", t.id, t.name);
            }
            Ok(())
        }
        Cmd::Skin(SkinCmd::Export { theme, output }) => {
            let Some(t) = amp_skin::theme::find(&theme) else {
                bail!("unknown theme {theme:?}; see `openamp-cli skin list`");
            };
            std::fs::write(&output, t.export_wsz()?)
                .with_context(|| output.display().to_string())?;
            println!("wrote {} ({})", output.display(), t.name);
            Ok(())
        }
        Cmd::Paths => {
            let d = amp_platform::AppDirs::system();
            println!("config  {}", d.config.display());
            println!("        {}", d.config_file().display());
            println!("data    {}", d.data.display());
            println!("cache   {}", d.cache.display());
            match &d.music {
                Some(m) => println!("music   {}", m.display()),
                None => println!("music   (not found)"),
            }
            Ok(())
        }
        Cmd::Devices => {
            for (id, name) in amp_audio_out::list_outputs()? {
                println!("{id:<40} {name}");
            }
            Ok(())
        }
        Cmd::Decode {
            file,
            output,
            rate,
            seek,
            length,
        } => decode(&file, &output, rate, seek, length),
        Cmd::Play {
            files,
            sink,
            device,
            seek,
            shuffle,
            repeat,
            volume,
            balance,
        } => play(files, sink, device, seek, shuffle, repeat, volume, balance),
    }
}

fn library(cmd: LibraryCmd) -> Result<()> {
    use amp_app::config::Config;
    let dirs = amp_platform::AppDirs::system();
    let config_path = dirs.config_file();
    let mut config = Config::load(&config_path)?;
    let db_path = dirs.data.join("library.sqlite3");
    match cmd {
        LibraryCmd::Folders => {
            for root in config.library.roots(dirs.music.as_deref()) {
                let note =
                    if Some(&root) == dirs.music.as_ref() && config.library.include_music_folder {
                        " (Music folder)"
                    } else {
                        ""
                    };
                let state = if root.is_dir() {
                    ""
                } else {
                    "  [not available]"
                };
                println!("{}{note}{state}", root.display());
            }
        }
        LibraryCmd::Add { folder } => {
            let folder = amp_library::canonicalize(&folder)
                .with_context(|| format!("{} is not an existing folder", folder.display()))?;
            if !folder.is_dir() {
                bail!("{} is not a folder", folder.display());
            }
            if config.library.add(folder.clone()) {
                config.save(&config_path)?;
                println!(
                    "added {}; run `openamp-cli library scan` to index it",
                    folder.display()
                );
            } else {
                println!("{} is already in the library", folder.display());
            }
        }
        LibraryCmd::Remove { folder } => {
            let canonical = amp_library::canonicalize(&folder).unwrap_or(folder.clone());
            if config.library.remove(&canonical) || config.library.remove(&folder) {
                config.save(&config_path)?;
                println!("removed {}", folder.display());
            } else {
                bail!("{} is not one of the added folders", folder.display());
            }
        }
        LibraryCmd::Scan => {
            let roots = config.library.roots(dirs.music.as_deref());
            let mut lib = amp_library::Library::open(&db_path)?;
            let tty = std::io::stderr().is_terminal();
            let started = Instant::now();
            let report = lib.scan(&roots, &amp_library::FfmpegTags, &mut |done, total| {
                if tty && (done % 25 == 0 || done == total) {
                    eprint!("\rscanning {done}/{total}");
                }
            })?;
            if tty {
                eprintln!();
            }
            println!(
                "{} folders scanned, {} not available; {} files: {} added, {} updated, {} unchanged, {} removed, {} without readable tags ({:.1}s)",
                report.roots_scanned,
                report.roots_missing,
                report.files_seen,
                report.added,
                report.updated,
                report.unchanged,
                report.removed,
                report.unreadable,
                started.elapsed().as_secs_f64()
            );
        }
        LibraryCmd::Search { text, limit } => {
            let lib = amp_library::Library::open(&db_path)?;
            for t in lib.search(&text, limit)? {
                let duration = t.tags.duration.map(clock).unwrap_or_default();
                let album = t.tags.album.as_deref().unwrap_or("");
                println!("{:<50} {:>6}  {album}", t.display_title(), duration);
            }
        }
        LibraryCmd::Stats => {
            let lib = amp_library::Library::open(&db_path)?;
            let s = lib.stats()?;
            println!(
                "{} tracks, {} artists, {} albums, {} total",
                s.tracks,
                s.artists,
                s.albums,
                clock(s.total_duration)
            );
        }
    }
    Ok(())
}

fn probe(files: &[PathBuf]) -> Result<()> {
    let mut failed = 0;
    for path in files {
        match Decoder::open(path, 44_100) {
            Ok(dec) => {
                let i = dec.info();
                println!("{}", path.display());
                println!("  container  {}", i.container);
                println!("  codec      {}", i.codec);
                println!("  format     {} Hz, {} ch", i.sample_rate, i.channels);
                if let Some(b) = i.bit_rate {
                    println!("  bit rate   {} kbps", b / 1000);
                }
                if let Some(d) = i.duration {
                    println!("  duration   {}", clock(d));
                }
                for (k, v) in [
                    ("title", &i.title),
                    ("artist", &i.artist),
                    ("album", &i.album),
                ] {
                    if let Some(v) = v {
                        println!("  {k:<10} {v}");
                    }
                }
            }
            Err(e) => {
                failed += 1;
                eprintln!("{}: {e}", path.display());
            }
        }
    }
    if failed > 0 {
        bail!("{failed} of {} files could not be opened", files.len());
    }
    Ok(())
}

fn decode(
    file: &Path,
    output: &Path,
    rate: u32,
    seek: Option<f64>,
    length: Option<f64>,
) -> Result<()> {
    let mut dec = Decoder::open(file, rate)?;
    if let Some(s) = seek {
        dec.seek(Duration::try_from_secs_f64(s).context("bad --seek")?)?;
    }
    let limit = length.map(|l| (l.max(0.0) * f64::from(rate)) as usize * OUTPUT_CHANNELS);
    let mut samples = Vec::new();
    while limit.is_none_or(|l| samples.len() < l) && dec.read(&mut samples)? {}
    if let Some(l) = limit {
        samples.truncate(l);
    }
    let out = BufWriter::new(File::create(output).with_context(|| output.display().to_string())?);
    write_wav(
        out,
        WavFormat::Float32,
        rate,
        OUTPUT_CHANNELS as u16,
        &samples,
    )?;
    eprintln!(
        "wrote {} frames ({}) to {}",
        samples.len() / OUTPUT_CHANNELS,
        clock(Duration::from_secs_f64(
            (samples.len() / OUTPUT_CHANNELS) as f64 / f64::from(rate)
        )),
        output.display()
    );
    Ok(())
}

enum Input {
    TogglePause,
    Next,
    Prev,
    Seek(Duration),
    Volume(f32),
    Quit,
}

/// Keeps the sink alive for the duration of playback.
enum SinkHandle {
    Device(#[allow(dead_code)] amp_audio_out::AudioOutput),
    Null(#[allow(dead_code)] NullSink),
}

#[allow(clippy::too_many_arguments)]
fn play(
    files: Vec<PathBuf>,
    sink: Sink,
    device_name: Option<String>,
    seek: Option<f64>,
    shuffle: bool,
    repeat: RepeatArg,
    volume: u8,
    balance: i8,
) -> Result<()> {
    if files.is_empty() {
        bail!("nothing to play");
    }
    let controls;
    let device = match sink {
        Sink::Device => Some(
            OutputDevice::open(device_name.as_deref())
                .context("opening audio device (see `devices`)")?,
        ),
        _ => None,
    };
    let rate = device.as_ref().map_or(44_100, OutputDevice::sample_rate);
    let (player, source) = Player::new(PlayerConfig {
        sample_rate: rate,
        ..PlayerConfig::default()
    })?;

    let _sink = match device {
        Some(dev) => {
            log::info!(
                "output {}: {} Hz, {} ch, {:?}",
                dev.id().unwrap_or_default(),
                dev.sample_rate(),
                dev.channels(),
                dev.sample_format()
            );
            let mut source = source;
            let out = dev.start(move |buf| {
                source.fill(buf);
            })?;
            controls = Arc::clone(out.controls());
            SinkHandle::Device(out)
        }
        None => {
            let sink = NullSink::start(source, matches!(sink, Sink::Null))?;
            controls = Arc::clone(sink.controls());
            SinkHandle::Null(sink)
        }
    };
    controls.set_volume(f32::from(volume.min(100)) / 100.0);
    controls.set_balance(f32::from(balance.clamp(-100, 100)) / 100.0);

    player.enqueue(files);
    player.with_queue(|q| {
        q.set_repeat(match repeat {
            RepeatArg::Off => Repeat::Off,
            RepeatArg::All => Repeat::All,
            RepeatArg::One => Repeat::One,
        });
        q.set_shuffle(shuffle);
    });
    player.play()?;

    let started = Instant::now();
    while player.status().state == PlaybackState::Stopped {
        if started.elapsed() > Duration::from_secs(10) {
            bail!("no track could be played");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if let Some(s) = seek {
        player.seek(Duration::try_from_secs_f64(s).context("bad --seek")?)?;
    }

    let input = spawn_stdin_reader();
    let tty = std::io::stderr().is_terminal();
    let mut last_track = None;
    let mut last_line = String::new();
    loop {
        while let Ok(cmd) = input.try_recv() {
            match cmd {
                Input::TogglePause => player.toggle_pause(),
                Input::Next => player.next()?,
                Input::Prev => player.prev()?,
                Input::Seek(d) => player.seek(d)?,
                Input::Volume(v) => controls.set_volume(v),
                Input::Quit => {
                    player.stop()?;
                    eprintln!();
                    return Ok(());
                }
            }
        }

        let status = player.status();
        if status.state == PlaybackState::Stopped {
            break;
        }
        if status.track != last_track {
            last_track = status.track;
            if let Some(info) = &status.info {
                if tty {
                    eprintln!();
                }
                eprintln!("{}", describe(info));
            }
        }
        let line = format!(
            "{} {} / {}",
            match status.state {
                PlaybackState::Paused => "||",
                _ => "> ",
            },
            clock(status.position),
            status
                .info
                .as_ref()
                .and_then(|i| i.duration)
                .map_or_else(|| "--:--".to_owned(), clock),
        );
        if tty && line != last_line {
            eprint!("\r{line}  ");
            let _ = std::io::stderr().flush();
            last_line = line;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if tty {
        eprintln!();
    }
    eprintln!("finished");
    Ok(())
}

/// Pulls audio and throws it away, either at real-time pace or as fast as possible.
fn spawn_stdin_reader() -> mpsc::Receiver<Input> {
    let (tx, rx) = mpsc::channel();
    // Detached on purpose: a blocking stdin read cannot be interrupted, and the process exits
    // when playback ends.
    let spawned = std::thread::Builder::new()
        .name("stdin".into())
        .spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let Ok(line) = line else { return };
                let mut parts = line.split_whitespace();
                let cmd = match (parts.next(), parts.next()) {
                    (Some("p"), _) => Input::TogglePause,
                    (Some("n"), _) => Input::Next,
                    (Some("b"), _) => Input::Prev,
                    (Some("q"), _) => Input::Quit,
                    (Some("s"), Some(secs)) => match secs
                        .parse::<f64>()
                        .ok()
                        .and_then(|s| Duration::try_from_secs_f64(s).ok())
                    {
                        Some(d) => Input::Seek(d),
                        None => continue,
                    },
                    (Some("v"), Some(v)) => match v.parse::<f32>() {
                        Ok(v) => Input::Volume(v / 100.0),
                        Err(_) => continue,
                    },
                    _ => {
                        eprintln!("\ncommands: p, n, b, s SECS, v 0-100, q");
                        continue;
                    }
                };
                if tx.send(cmd).is_err() {
                    return;
                }
            }
        });
    if let Err(e) = spawned {
        log::warn!("no keyboard control: {e}");
    }
    rx
}

fn describe(info: &amp_core::TrackInfo) -> String {
    let name = match (&info.artist, &info.title) {
        (Some(a), Some(t)) => format!("{a} - {t}"),
        (None, Some(t)) => t.clone(),
        _ => info
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    format!(
        "{name}  [{} {} Hz {} ch]",
        info.codec, info.sample_rate, info.channels
    )
}

fn clock(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_formats() {
        assert_eq!(clock(Duration::from_secs(0)), "00:00");
        assert_eq!(clock(Duration::from_secs(83)), "01:23");
        assert_eq!(clock(Duration::from_secs(3_725)), "1:02:05");
    }
}
