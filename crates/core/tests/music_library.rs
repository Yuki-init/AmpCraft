//! Opt-in checks against the owner's music library.
//!
//! Runs only when `WINAMP_RS_MUSIC_DIR` points at an existing directory, otherwise it passes
//! without doing anything. The library is read-only and private: this test never writes to
//! it and prints aggregate numbers only, never file names or paths.
//!
//! Tunables: `WINAMP_RS_MUSIC_SAMPLE` (files to test, default 60) and
//! `WINAMP_RS_MUSIC_SCAN_LIMIT` (directory entries to visit, default 50000).

// Integration tests are test code; panicking on setup failure is the point.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use amp_core::{CoreError, Decoder};

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "opus", "m4a", "mp4", "aac", "wma", "wav", "aif", "aiff", "ape",
    "wv", "mpc", "mp2", "ac3", "alac", "caf", "webm", "mka",
];
const RATE: u32 = 44_100;
const TIME_BUDGET: Duration = Duration::from_secs(300);

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Walks the tree in sorted order, visiting at most `limit` entries. Skips hidden entries.
fn collect_audio(root: &Path, limit: usize) -> (Vec<PathBuf>, usize) {
    let mut found = Vec::new();
    let mut visited = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        let mut subdirs = Vec::new();
        for e in entries {
            visited += 1;
            if visited > limit {
                return (found, visited);
            }
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(ft) = e.file_type() else { continue };
            let path = e.path();
            if ft.is_dir() {
                subdirs.push(path);
            } else if ft.is_file() && extension(&path).is_some() {
                found.push(path);
            }
        }
        // Reverse so the stack pops directories in sorted order.
        stack.extend(subdirs.into_iter().rev());
    }
    (found, visited)
}

/// Lowercased audio extension, if the file has one we care about. exFAT is case-insensitive.
fn extension(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    AUDIO_EXTENSIONS.contains(&ext.as_str()).then_some(ext)
}

fn error_kind(e: &CoreError) -> &'static str {
    match e {
        CoreError::Init(_) => "init",
        CoreError::NonUtf8Path(_) => "non-utf8 path",
        CoreError::Open { .. } => "open",
        CoreError::NoAudioStream(_) => "no audio stream",
        CoreError::Codec(_) => "codec setup",
        CoreError::Decode(_) => "decode",
        CoreError::TooManyErrors(_) => "too many decode errors",
        CoreError::Resample(_) => "resample",
        CoreError::Seek(_) => "seek",
        CoreError::Io(_) => "io",
        CoreError::NotQueued(_) | CoreError::EngineGone => "engine",
    }
}

#[derive(Default)]
struct Stats {
    by_ext: BTreeMap<String, (usize, usize)>,
    failures: BTreeMap<String, usize>,
    codecs: BTreeMap<String, usize>,
    rates: BTreeMap<u32, usize>,
    channels: BTreeMap<u16, usize>,
    no_duration: usize,
    no_title: usize,
    no_artist: usize,
    seek_short: usize,
}

/// Decodes the start, seeks to the middle, decodes again. Returns a failure category on error.
fn check_file(path: &Path, stats: &mut Stats) -> Result<(), String> {
    let mut dec = Decoder::open(path, RATE).map_err(|e| error_kind(&e).to_string())?;
    let info = dec.info().clone();
    *stats.codecs.entry(info.codec.clone()).or_default() += 1;
    *stats.rates.entry(info.sample_rate).or_default() += 1;
    *stats.channels.entry(info.channels).or_default() += 1;
    stats.no_duration += usize::from(info.duration.is_none());
    stats.no_title += usize::from(info.title.is_none());
    stats.no_artist += usize::from(info.artist.is_none());

    let mut out = Vec::new();
    while out.len() < 2 * RATE as usize * 2 {
        match dec.read(&mut out) {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => return Err(format!("{} (start)", error_kind(&e))),
        }
    }
    if out.is_empty() {
        return Err("no audio decoded".into());
    }
    if out.iter().any(|s| !s.is_finite()) {
        return Err("non-finite samples".into());
    }

    if let Some(d) = info.duration.filter(|d| *d > Duration::from_secs(4)) {
        dec.seek(d / 2)
            .map_err(|e| format!("{} (mid)", error_kind(&e)))?;
        out.clear();
        while out.len() < RATE as usize * 2 {
            match dec.read(&mut out) {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => return Err(format!("{} (after seek)", error_kind(&e))),
            }
        }
        if out.len() < RATE as usize * 2 {
            stats.seek_short += 1;
        }
    }
    Ok(())
}

#[test]
fn sample_of_library_decodes_and_seeks() {
    let Some(root) = std::env::var_os("WINAMP_RS_MUSIC_DIR").map(PathBuf::from) else {
        eprintln!("WINAMP_RS_MUSIC_DIR not set, skipping");
        return;
    };
    if !root.is_dir() {
        eprintln!("music dir not mounted, skipping");
        return;
    }
    let sample = env_usize("WINAMP_RS_MUSIC_SAMPLE", 60);
    let scan_limit = env_usize("WINAMP_RS_MUSIC_SCAN_LIMIT", 50_000);

    let started = Instant::now();
    let (all, visited) = collect_audio(&root, scan_limit);
    let scan_time = started.elapsed();
    if all.is_empty() {
        eprintln!("no audio files found, skipping");
        return;
    }
    // Evenly spaced picks across the sorted walk, so every part of the tree is represented.
    let step = (all.len() as f64 / sample.max(1) as f64).max(1.0);
    let picks: Vec<&PathBuf> = (0..sample.min(all.len()))
        .map(|i| &all[(i as f64 * step) as usize])
        .collect();

    let mut stats = Stats::default();
    let mut tested = 0;
    for path in &picks {
        if started.elapsed() > TIME_BUDGET {
            eprintln!("time budget reached");
            break;
        }
        tested += 1;
        let ext = extension(path).unwrap_or_default();
        let result = catch_unwind(AssertUnwindSafe(|| check_file(path, &mut stats)))
            .unwrap_or_else(|_| Err("panic".into()));
        let entry = stats.by_ext.entry(ext.clone()).or_default();
        match result {
            Ok(()) => entry.0 += 1,
            Err(kind) => {
                entry.1 += 1;
                *stats.failures.entry(format!("{ext}: {kind}")).or_default() += 1;
            }
        }
    }

    let failed: usize = stats.by_ext.values().map(|(_, f)| f).sum();
    eprintln!("== music library sample (aggregate only)");
    eprintln!(
        "scanned {visited} entries, {} audio files in {:.1}s; tested {tested} in {:.1}s",
        all.len(),
        scan_time.as_secs_f64(),
        (started.elapsed() - scan_time).as_secs_f64()
    );
    for (ext, (ok, bad)) in &stats.by_ext {
        eprintln!("  {ext:<6} ok {ok:>4}  failed {bad:>3}");
    }
    for (kind, n) in &stats.failures {
        eprintln!("  failure  {kind}: {n}");
    }
    eprintln!("  codecs   {:?}", stats.codecs);
    eprintln!("  rates    {:?}", stats.rates);
    eprintln!("  channels {:?}", stats.channels);
    eprintln!(
        "  missing duration {}, title {}, artist {}; short read after seek {}",
        stats.no_duration, stats.no_title, stats.no_artist, stats.seek_short
    );

    assert!(
        !stats.failures.keys().any(|k| k.ends_with("panic")),
        "decoder panicked on a library file"
    );
    // Real libraries contain some damaged files; fail only on a broad regression.
    assert!(
        failed * 10 <= tested,
        "{failed} of {tested} sampled files failed"
    );
}
