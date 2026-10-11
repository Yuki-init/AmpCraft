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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use amp_audio_out::{Controls, OutputDevice};
use amp_core::wav::{WavFormat, write_wav};
use amp_core::{Decoder, OUTPUT_CHANNELS, PcmSource, PlaybackState, Player, PlayerConfig, Repeat};
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
    StderrLogger::install();
    match Cli::parse().command {
        Cmd::Probe { files } => probe(&files),
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
            seek,
            shuffle,
            repeat,
            volume,
            balance,
        } => play(files, sink, seek, shuffle, repeat, volume, balance),
    }
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
    Thread(Arc<AtomicBool>, Option<std::thread::JoinHandle<()>>),
}

impl Drop for SinkHandle {
    fn drop(&mut self) {
        if let SinkHandle::Thread(stop, handle) = self {
            stop.store(true, Ordering::Relaxed);
            if let Some(h) = handle.take() {
                let _ = h.join();
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn play(
    files: Vec<PathBuf>,
    sink: Sink,
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
        Sink::Device => Some(OutputDevice::default_output().context("opening audio device")?),
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
                "output: {} Hz, {} ch, {:?}",
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
            controls = Arc::new(Controls::default());
            let stop = Arc::new(AtomicBool::new(false));
            let realtime = matches!(sink, Sink::Null);
            let handle = spawn_null_sink(source, Arc::clone(&stop), realtime)?;
            SinkHandle::Thread(stop, Some(handle))
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
fn spawn_null_sink(
    mut source: PcmSource,
    stop: Arc<AtomicBool>,
    realtime: bool,
) -> Result<std::thread::JoinHandle<()>> {
    let rate = source.sample_rate();
    let handle = std::thread::Builder::new()
        .name("null-sink".into())
        .spawn(move || {
            let period = Duration::from_millis(10);
            let frames = (rate as usize / 100).max(1);
            let mut buf = vec![0.0f32; frames * OUTPUT_CHANNELS];
            let mut next = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                let n = source.fill(&mut buf);
                if realtime {
                    next += period;
                    if let Some(wait) = next.checked_duration_since(Instant::now()) {
                        std::thread::sleep(wait);
                    }
                } else if n == 0 {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        })?;
    Ok(handle)
}

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

/// Minimal logger: level from `OPENAMP_LOG` (error, warn, info, debug, trace), default warn.
struct StderrLogger;

impl StderrLogger {
    fn install() {
        let level = std::env::var("OPENAMP_LOG")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(log::LevelFilter::Warn);
        if log::set_logger(&StderrLogger).is_ok() {
            log::set_max_level(level);
        }
    }
}

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
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
