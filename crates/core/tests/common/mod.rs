//! Shared helpers for integration tests. Fixtures are generated, never committed.
#![allow(dead_code)]

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use ampcraft_core::wav::{WavFormat, write_wav};
use ampcraft_core::{Decoder, PcmSource, PlaybackState, Player};

/// Stereo test signal: different frequencies per channel so swaps and drops are visible.
pub fn stereo_tone(rate: u32, frames: usize, start_frame: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(frames * 2);
    for i in start_frame..start_frame + frames {
        let t = i as f64 / f64::from(rate);
        out.push((0.5 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as f32);
        out.push((0.3 * (2.0 * std::f64::consts::PI * 660.0 * t).sin()) as f32);
    }
    out
}

/// Stereo linear chirp. Not periodic, so alignment against it is unambiguous.
pub fn stereo_chirp(rate: u32, frames: usize, start_frame: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(frames * 2);
    for i in start_frame..start_frame + frames {
        let t = i as f64 / f64::from(rate);
        // Instantaneous frequency 200 Hz rising 300 Hz per second.
        let phase = 2.0 * std::f64::consts::PI * (200.0 * t + 150.0 * t * t);
        out.push((0.5 * phase.sin()) as f32);
        out.push((0.3 * (phase * 1.5).sin()) as f32);
    }
    out
}

pub fn mono_tone(rate: u32, frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|i| {
            let t = i as f64 / f64::from(rate);
            (0.5 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as f32
        })
        .collect()
}

pub fn write_fixture(
    dir: &Path,
    name: &str,
    format: WavFormat,
    rate: u32,
    channels: u16,
    samples: &[f32],
) -> PathBuf {
    let path = dir.join(name);
    let file = BufWriter::new(File::create(&path).unwrap());
    write_wav(file, format, rate, channels, samples).unwrap();
    path
}

/// Encodes `src` with the ffmpeg CLI. Returns None (test should skip) if ffmpeg is missing.
pub fn ffmpeg_encode(src: &Path, dst: &Path, args: &[&str]) -> Option<PathBuf> {
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(src)
        .args(args)
        .arg(dst)
        .status();
    match status {
        Ok(s) if s.success() => Some(dst.to_path_buf()),
        Ok(s) => panic!("ffmpeg failed with {s} for {}", dst.display()),
        Err(_) => {
            eprintln!("ffmpeg CLI not found, skipping");
            None
        }
    }
}

pub fn decode_all(path: &Path, rate: u32) -> Vec<f32> {
    let mut dec = Decoder::open(path, rate).unwrap();
    let mut out = Vec::new();
    while dec.read(&mut out).unwrap() {}
    out
}

pub fn assert_close(actual: &[f32], expected: &[f32], tol: f32) {
    assert_eq!(actual.len() / 2, expected.len() / 2, "frame count differs");
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (a - e).abs() <= tol,
            "sample {i} (frame {}): got {a}, expected {e}",
            i / 2
        );
    }
}

pub fn rms(samples: &[f32]) -> f64 {
    let sum: f64 = samples.iter().map(|s| f64::from(*s).powi(2)).sum();
    (sum / samples.len().max(1) as f64).sqrt()
}

/// Polls until `cond` holds or panics after `timeout`.
pub fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Pulls from `source` until the player reports Stopped. Returns everything heard.
pub fn drain(player: &Player, source: &mut PcmSource) -> Vec<f32> {
    wait_until(Duration::from_secs(10), || {
        player.status().state != PlaybackState::Stopped
    });
    drain_rest(player, source)
}

/// Pulls from `source` until the player reports Stopped, without waiting for it to start.
pub fn drain_rest(player: &Player, source: &mut PcmSource) -> Vec<f32> {
    let mut heard = Vec::new();
    let mut buf = vec![0.0f32; 1024];
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let n = source.fill(&mut buf);
        heard.extend_from_slice(&buf[..n * 2]);
        if n == 0 {
            if player.status().state == PlaybackState::Stopped {
                return heard;
            }
            assert!(Instant::now() < deadline, "drain timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// Pulls exactly `frames` frames of real audio.
pub fn pull(source: &mut PcmSource, frames: usize) -> Vec<f32> {
    let mut heard = Vec::with_capacity(frames * 2);
    let mut buf = vec![0.0f32; 1024];
    let deadline = Instant::now() + Duration::from_secs(10);
    while heard.len() < frames * 2 {
        let want = (frames * 2 - heard.len()).min(buf.len());
        let n = source.fill(&mut buf[..want]);
        heard.extend_from_slice(&buf[..n * 2]);
        if n == 0 {
            assert!(Instant::now() < deadline, "pull timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    heard
}
