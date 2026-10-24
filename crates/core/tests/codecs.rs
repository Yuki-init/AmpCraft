//! Codec and container coverage using fixtures encoded on the fly by the ffmpeg CLI.

// Integration tests are test code; panicking on setup failure is the point.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

mod common;

use std::path::Path;

use ampcraft_core::wav::WavFormat;
use ampcraft_core::{Player, PlayerConfig};
use common::*;

const RATE: u32 = 44_100;

/// (file name, encoder args, exact length expected, max RMS error against the source).
const CODECS: &[(&str, &[&str], bool, f64)] = &[
    ("t.flac", &["-c:a", "flac"], true, 1e-4),
    ("t.alac.m4a", &["-c:a", "alac"], true, 1e-4),
    ("t.mp3", &["-c:a", "libmp3lame", "-b:a", "192k"], true, 0.03),
    ("t.ogg", &["-c:a", "libvorbis", "-q:a", "5"], true, 0.03),
    ("t.opus", &["-c:a", "libopus", "-b:a", "128k"], true, 0.03),
    // FFmpeg's AAC encoder leaves end padding untrimmed, so length is not exact.
    ("t.aac.m4a", &["-c:a", "aac", "-b:a", "192k"], false, 0.03),
    // WMA has a fixed decoder delay the container does not record, so only check it decodes.
    (
        "t.wma",
        &["-c:a", "wmav2", "-b:a", "192k"],
        false,
        f64::INFINITY,
    ),
];

fn rms_error(a: &[f32], b: &[f32]) -> f64 {
    let n = a.len().min(b.len());
    let sum: f64 = a[..n]
        .iter()
        .zip(&b[..n])
        .map(|(x, y)| f64::from(x - y).powi(2))
        .sum();
    (sum / n.max(1) as f64).sqrt()
}

fn source_wav(
    dir: &Path,
    frames: usize,
    start: usize,
    name: &str,
) -> (Vec<f32>, std::path::PathBuf) {
    let src = stereo_chirp(RATE, frames, start);
    let wav = write_fixture(dir, name, WavFormat::Pcm16, RATE, 2, &src);
    (src, wav)
}

#[test]
fn every_codec_decodes_with_expected_length_and_fidelity() {
    let dir = tempfile::tempdir().unwrap();
    let (src, wav) = source_wav(dir.path(), 88_200, 0, "src.wav");
    for (name, args, exact, max_err) in CODECS {
        let Some(path) = ffmpeg_encode(&wav, &dir.path().join(name), args) else {
            return;
        };
        let out = decode_all(&path, RATE);
        let frames = out.len() / 2;
        if *exact {
            assert_eq!(frames, 88_200, "{name}: length");
        } else {
            assert!(frames.abs_diff(88_200) < 4096, "{name}: length {frames}");
        }
        assert!(
            out.iter().all(|s| s.is_finite()),
            "{name}: non-finite sample"
        );
        let err = rms_error(&out, &src);
        assert!(err <= *max_err, "{name}: rms error {err}");
    }
}

#[test]
fn info_reports_codec_duration_and_tags() {
    let dir = tempfile::tempdir().unwrap();
    let (_, wav) = source_wav(dir.path(), 44_100, 0, "src.wav");
    let Some(path) = ffmpeg_encode(
        &wav,
        &dir.path().join("tagged.flac"),
        &[
            "-c:a",
            "flac",
            "-metadata",
            "title=Tëst Ťitle",
            "-metadata",
            "artist=Ärtist",
            "-metadata",
            "album=Album 日本",
            "-metadata",
            "track=3/12",
            "-metadata",
            "date=2007-05-01",
            "-metadata",
            "genre=Trance",
            "-metadata",
            "album_artist=Various",
        ],
    ) else {
        return;
    };
    let dec = ampcraft_core::Decoder::open(&path, RATE).unwrap();
    let info = dec.info();
    assert_eq!(info.codec, "flac");
    assert_eq!(info.sample_rate, RATE);
    assert_eq!(info.channels, 2);
    assert_eq!(info.title.as_deref(), Some("Tëst Ťitle"));
    assert_eq!(info.artist.as_deref(), Some("Ärtist"));
    assert_eq!(info.album.as_deref(), Some("Album 日本"));
    assert_eq!(info.track, Some(3));
    assert_eq!(info.year, Some(2007));
    assert_eq!(info.genre.as_deref(), Some("Trance"));
    assert_eq!(info.album_artist.as_deref(), Some("Various"));
    let d = info.duration.unwrap().as_secs_f64();
    assert!((d - 1.0).abs() < 0.01, "duration {d}");
}

/// How strictly a codec is held to gapless playback.
struct Gapless {
    ext: &'static str,
    args: &'static [&'static str],
    /// Allowed length error per file, in frames.
    len_tol: usize,
    /// Per-sample tolerance against the source, or 0 to skip the exact comparison.
    exact_tol: f32,
    /// Allowed RMS error in the window around each join, after local alignment.
    join_err: f64,
}

/// Splits one continuous signal into consecutive files and checks the joins are seamless.
fn gapless_case(c: Gapless) {
    let dir = tempfile::tempdir().unwrap();
    // Odd split points so joins never land on codec frame boundaries.
    let lengths = [30_011usize, 44_101, 20_000];
    let total: usize = lengths.iter().sum();
    let expected = stereo_chirp(RATE, total, 0);

    let mut paths = Vec::new();
    let mut start = 0;
    for (i, len) in lengths.iter().enumerate() {
        let (_, wav) = source_wav(dir.path(), *len, start, &format!("part{i}.wav"));
        let Some(p) = ffmpeg_encode(&wav, &dir.path().join(format!("part{i}.{}", c.ext)), c.args)
        else {
            return;
        };
        paths.push(p);
        start += len;
    }

    let (player, mut source) = Player::new(PlayerConfig::default()).unwrap();
    player.enqueue(paths);
    player.play().unwrap();
    let heard = drain(&player, &mut source);
    let frames = heard.len() / 2;
    assert!(
        frames.abs_diff(total) <= c.len_tol * lengths.len(),
        "{}: {frames} frames, expected {total}",
        c.ext
    );
    if c.exact_tol > 0.0 {
        assert_close(&heard, &expected, c.exact_tol);
    }

    // Around each join, the heard audio must line up with the source at some small offset:
    // no inserted silence, no dropped block, no click.
    let mut drift = 0i64;
    let mut join = 0usize;
    for len in &lengths[..2] {
        join += len;
        let (off, err) = best_alignment(&heard, &expected, join, drift, c.len_tol as i64 + 2);
        assert!(
            err <= c.join_err,
            "{}: join at {join} rms error {err} (offset {off})",
            c.ext
        );
        drift = off;
    }
}

/// Finds the offset near `drift` that best aligns heard audio to the source around `join`.
fn best_alignment(
    heard: &[f32],
    expected: &[f32],
    join: usize,
    drift: i64,
    range: i64,
) -> (i64, f64) {
    let half = 96i64;
    let mut best = (0, f64::INFINITY);
    for off in drift - range..=drift + range {
        let mut sum = 0.0;
        for f in join as i64 - half..join as i64 + half {
            let h = f + off;
            if h < 0 || (h as usize + 1) * 2 > heard.len() {
                sum = f64::INFINITY;
                break;
            }
            for ch in 0..2 {
                let d = heard[h as usize * 2 + ch] - expected[f as usize * 2 + ch];
                sum += f64::from(d).powi(2);
            }
        }
        let err = (sum / (half * 4) as f64).sqrt();
        if err < best.1 {
            best = (off, err);
        }
    }
    best
}

#[test]
fn gapless_flac() {
    gapless_case(Gapless {
        ext: "flac",
        args: &["-c:a", "flac"],
        len_tol: 0,
        exact_tol: 1.0 / 16_000.0,
        join_err: 1e-4,
    });
}

#[test]
fn gapless_mp3() {
    gapless_case(Gapless {
        ext: "mp3",
        args: &["-c:a", "libmp3lame", "-b:a", "192k"],
        len_tol: 0,
        exact_tol: 0.0,
        join_err: 0.03,
    });
}

#[test]
fn gapless_opus() {
    // Opus is always 48 kHz inside; the round trip through 44.1 kHz can add one frame per file.
    gapless_case(Gapless {
        ext: "opus",
        args: &["-c:a", "libopus", "-b:a", "128k"],
        len_tol: 1,
        exact_tol: 0.0,
        join_err: 0.03,
    });
}

#[test]
fn vorbis_length_is_close() {
    // libavcodec trims 128 frames too many from a final partial Vorbis block, so the length
    // can come up short by that much.
    let dir = tempfile::tempdir().unwrap();
    let (_, wav) = source_wav(dir.path(), 30_011, 0, "v.wav");
    let Some(p) = ffmpeg_encode(&wav, &dir.path().join("v.ogg"), &["-c:a", "libvorbis"]) else {
        return;
    };
    let frames = decode_all(&p, RATE).len() / 2;
    assert!(frames.abs_diff(30_011) <= 128, "{frames}");
}

#[test]
#[ignore = "known FFmpeg limitation: Vorbis end trim drops 128 frames at each join"]
fn gapless_vorbis_strict() {
    gapless_case(Gapless {
        ext: "ogg",
        args: &["-c:a", "libvorbis", "-q:a", "5"],
        len_tol: 0,
        exact_tol: 0.0,
        join_err: 0.03,
    });
}
