//! Opt-in: scans part of the owner's music library into a temporary database. Runs only when
//! `AMPCRAFT_TEST_MUSIC_DIR` exists. The library is only read; output is aggregate counts only.

// Integration tests are test code; panicking on setup failure is the point.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use std::path::PathBuf;
use std::time::Instant;

use ampcraft_library::{FfmpegTags, Library};

/// Album folders scanned per run, so the test stays bounded on a large or slow drive.
const FOLDERS: usize = 3;

#[test]
fn scan_sample_of_real_library() {
    let Some(root) = std::env::var_os("AMPCRAFT_TEST_MUSIC_DIR").map(PathBuf::from) else {
        eprintln!("AMPCRAFT_TEST_MUSIC_DIR not set, skipping");
        return;
    };
    if !root.is_dir() {
        eprintln!("music dir not mounted, skipping");
        return;
    }
    // Find folders that contain audio, two levels down at most, in name order.
    let mut candidates: Vec<PathBuf> = Vec::new();
    let mut level = vec![root.clone()];
    for _ in 0..2 {
        let mut next = Vec::new();
        for dir in &level {
            let Ok(rd) = std::fs::read_dir(dir) else {
                continue;
            };
            let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            entries.sort();
            for p in entries {
                if p.is_dir()
                    && !p
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
                {
                    next.push(p);
                }
            }
        }
        candidates.extend(
            next.iter()
                .filter(|d| {
                    std::fs::read_dir(d)
                        .map(|rd| rd.flatten().any(|e| ampcraft_library::is_audio(&e.path())))
                        .unwrap_or(false)
                })
                .cloned(),
        );
        level = next;
    }
    if candidates.is_empty() {
        eprintln!("no album folders found, skipping");
        return;
    }
    let step = (candidates.len() / FOLDERS).max(1);
    let roots: Vec<PathBuf> = candidates
        .iter()
        .step_by(step)
        .take(FOLDERS)
        .cloned()
        .collect();

    let db_dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(&db_dir.path().join("library.sqlite3")).unwrap();
    let started = Instant::now();
    let first = lib.scan(&roots, &FfmpegTags, &mut |_, _| {}).unwrap();
    let first_time = started.elapsed();
    let again = Instant::now();
    let second = lib.scan(&roots, &FfmpegTags, &mut |_, _| {}).unwrap();
    let second_time = again.elapsed();
    let stats = lib.stats().unwrap();
    let all = lib.search("", 100_000).unwrap();
    let tagged = all
        .iter()
        .filter(|t| t.tags.title.is_some() && t.tags.artist.is_some())
        .count();
    let with_album = all.iter().filter(|t| t.tags.album.is_some()).count();
    let with_track = all.iter().filter(|t| t.tags.track.is_some()).count();
    let with_year = all.iter().filter(|t| t.tags.year.is_some()).count();

    eprintln!("== library scan sample (aggregate only)");
    eprintln!(
        "folders {}: {} files, {} added, {} without readable tags in {:.1}s; rescan {} unchanged in {:.2}s",
        first.roots_scanned,
        first.files_seen,
        first.added,
        first.unreadable,
        first_time.as_secs_f64(),
        second.unchanged,
        second_time.as_secs_f64()
    );
    eprintln!(
        "tracks {}, artists {}, albums {}; tagged title+artist {}, album {}, track no. {}, year {}",
        stats.tracks, stats.artists, stats.albums, tagged, with_album, with_track, with_year
    );

    assert!(first.files_seen > 0);
    assert_eq!(first.added, first.files_seen);
    assert_eq!(second.unchanged, first.files_seen, "rescan re-read nothing");
    assert_eq!(second.added + second.updated + second.removed, 0);
    assert_eq!(stats.tracks as usize, first.files_seen);
}
