//! Engine behavior on generated WAV fixtures: exact decode, resampling, seek, gapless, transport.

// Integration tests are test code; panicking on setup failure is the point.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use amp_core::wav::WavFormat;
use amp_core::{Decoder, PlaybackState, Player, PlayerConfig, Repeat};
use common::*;

const RATE: u32 = 44_100;

fn assert_secs(actual: Duration, expected: f64) {
    let diff = (actual.as_secs_f64() - expected).abs();
    assert!(diff < 1e-6, "position {actual:?}, expected {expected}s");
}

#[test]
fn float_wav_decodes_bit_exact() {
    let dir = tempfile::tempdir().unwrap();
    let src = stereo_chirp(RATE, 10_000, 0);
    let p = write_fixture(dir.path(), "f.wav", WavFormat::Float32, RATE, 2, &src);
    assert_close(&decode_all(&p, RATE), &src, 1e-7);
}

#[test]
fn pcm16_wav_decodes_within_quantization() {
    let dir = tempfile::tempdir().unwrap();
    let src = stereo_chirp(RATE, 10_000, 0);
    let p = write_fixture(dir.path(), "i.wav", WavFormat::Pcm16, RATE, 2, &src);
    assert_close(&decode_all(&p, RATE), &src, 1.0 / 32_000.0);
}

#[test]
fn mono_is_copied_to_both_channels_at_full_level() {
    let dir = tempfile::tempdir().unwrap();
    let src = mono_tone(RATE, 5_000);
    let p = write_fixture(dir.path(), "m.wav", WavFormat::Float32, RATE, 1, &src);
    let out = decode_all(&p, RATE);
    assert_eq!(out.len(), src.len() * 2);
    for (i, s) in src.iter().enumerate() {
        assert!((out[i * 2] - s).abs() < 1e-6);
        assert_eq!(out[i * 2], out[i * 2 + 1]);
    }
}

#[test]
fn resampling_preserves_length_and_signal() {
    let dir = tempfile::tempdir().unwrap();
    let src = stereo_chirp(RATE, 44_100, 0);
    let p = write_fixture(dir.path(), "r.wav", WavFormat::Float32, RATE, 2, &src);
    let out = decode_all(&p, 48_000);
    let frames = out.len() / 2;
    assert!(frames.abs_diff(48_000) <= 2, "got {frames} frames");
    // Compare against the same chirp rendered directly at 48 kHz, away from the edges.
    let expected = stereo_chirp(48_000, 48_000, 0);
    let mid = &out[2_000..out.len() - 2_000];
    let exp = &expected[2_000..expected.len() - 2_000];
    let max = mid
        .iter()
        .zip(exp)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(max < 2e-3, "max error {max}");
}

#[test]
fn decoder_seek_is_sample_accurate() {
    let dir = tempfile::tempdir().unwrap();
    let src = stereo_chirp(RATE, 88_200, 0);
    let p = write_fixture(dir.path(), "s.wav", WavFormat::Float32, RATE, 2, &src);
    let mut dec = Decoder::open(&p, RATE).unwrap();
    let mut out = Vec::new();
    dec.read(&mut out).unwrap();
    dec.seek(Duration::from_millis(1_250)).unwrap();
    out.clear();
    while out.len() < 4_000 && dec.read(&mut out).unwrap() {}
    let start = 55_125 * 2; // 1.25 s
    assert_close(&out[..4_000], &src[start..start + 4_000], 1e-7);
}

#[test]
fn queue_plays_gaplessly_and_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let lengths = [12_345usize, 777, 30_000];
    let total: usize = lengths.iter().sum();
    let expected = stereo_chirp(RATE, total, 0);
    let mut start = 0;
    let mut paths = Vec::new();
    for (i, len) in lengths.iter().enumerate() {
        let part = &expected[start * 2..(start + len) * 2];
        paths.push(write_fixture(
            dir.path(),
            &format!("{i}.wav"),
            WavFormat::Float32,
            RATE,
            2,
            part,
        ));
        start += len;
    }
    let (player, mut source) = Player::new(PlayerConfig::default()).unwrap();
    player.enqueue(paths);
    player.play().unwrap();
    let heard = drain(&player, &mut source);
    assert_close(&heard, &expected, 1e-7);
}

#[test]
fn mixed_sample_rates_play_back_to_back() {
    let dir = tempfile::tempdir().unwrap();
    let a = write_fixture(
        dir.path(),
        "a.wav",
        WavFormat::Float32,
        48_000,
        2,
        &stereo_chirp(48_000, 24_000, 0),
    );
    let b = write_fixture(
        dir.path(),
        "b.wav",
        WavFormat::Float32,
        22_050,
        1,
        &mono_tone(22_050, 11_025),
    );
    let (player, mut source) = Player::new(PlayerConfig::default()).unwrap();
    player.enqueue([a, b]);
    player.play().unwrap();
    let heard = drain(&player, &mut source);
    let frames = heard.len() / 2;
    assert!(frames.abs_diff(22_050 + 22_050) <= 4, "got {frames}");
}

#[test]
fn position_follows_what_is_heard() {
    let dir = tempfile::tempdir().unwrap();
    let src = stereo_chirp(RATE, 88_200, 0);
    let p = write_fixture(dir.path(), "p.wav", WavFormat::Float32, RATE, 2, &src);
    let (player, mut source) = Player::new(PlayerConfig::default()).unwrap();
    let ids = player.enqueue([p]);
    player.play().unwrap();
    wait_until(Duration::from_secs(5), || {
        player.status().state == PlaybackState::Playing
    });
    let heard = pull(&mut source, 22_050);
    assert_close(&heard, &src[..44_100], 1e-7);
    let status = player.status();
    assert_eq!(status.track, Some(ids[0]));
    assert_eq!(status.position, Duration::from_millis(500));
    assert_eq!(status.info.unwrap().sample_rate, RATE);
}

#[test]
fn track_boundary_is_reported_when_heard_not_when_decoded() {
    let dir = tempfile::tempdir().unwrap();
    let a = write_fixture(
        dir.path(),
        "a.wav",
        WavFormat::Float32,
        RATE,
        2,
        &stereo_chirp(RATE, 4_410, 0),
    );
    let b = write_fixture(
        dir.path(),
        "b.wav",
        WavFormat::Float32,
        RATE,
        2,
        &stereo_chirp(RATE, 44_100, 0),
    );
    let (player, mut source) = Player::new(PlayerConfig::default()).unwrap();
    let ids = player.enqueue([a, b]);
    player.play().unwrap();
    wait_until(Duration::from_secs(5), || {
        player.status().state == PlaybackState::Playing
    });
    // Give the decoder time to run well into track b.
    std::thread::sleep(Duration::from_millis(100));
    pull(&mut source, 4_400);
    assert_eq!(player.status().track, Some(ids[0]));
    pull(&mut source, 20);
    let status = player.status();
    assert_eq!(status.track, Some(ids[1]));
    assert_secs(status.position, 10.0 / 44_100.0);
}

#[test]
fn seek_jumps_to_exact_sample_and_drops_stale_audio() {
    let dir = tempfile::tempdir().unwrap();
    let src = stereo_chirp(RATE, 132_300, 0);
    let p = write_fixture(dir.path(), "s.wav", WavFormat::Float32, RATE, 2, &src);
    let (player, mut source) = Player::new(PlayerConfig::default()).unwrap();
    player.enqueue([p]);
    player.play().unwrap();
    wait_until(Duration::from_secs(5), || {
        player.status().state == PlaybackState::Playing
    });
    pull(&mut source, 1_000);
    player.seek(Duration::from_secs(2)).unwrap();
    wait_until(Duration::from_secs(5), || {
        player.status().position == Duration::from_secs(2)
    });
    let heard = pull(&mut source, 2_000);
    assert_close(&heard, &src[88_200 * 2..90_200 * 2], 1e-7);
}

#[test]
fn seek_past_end_finishes_the_track() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_fixture(
        dir.path(),
        "s.wav",
        WavFormat::Float32,
        RATE,
        2,
        &stereo_chirp(RATE, 44_100, 0),
    );
    let (player, mut source) = Player::new(PlayerConfig::default()).unwrap();
    player.enqueue([p]);
    player.play().unwrap();
    wait_until(Duration::from_secs(5), || {
        player.status().state == PlaybackState::Playing
    });
    player.seek(Duration::from_secs(60)).unwrap();
    // Seeking is asynchronous; wait until it has taken effect before listening.
    wait_until(Duration::from_secs(5), || {
        let st = player.status();
        st.state == PlaybackState::Stopped || st.position >= Duration::from_secs(1)
    });
    let rest = drain_rest(&player, &mut source);
    assert!(
        rest.len() / 2 < 4_410,
        "played {} frames after seeking past end",
        rest.len() / 2
    );
}

#[test]
fn pause_freezes_output_and_position() {
    let dir = tempfile::tempdir().unwrap();
    let src = stereo_chirp(RATE, 44_100, 0);
    let p = write_fixture(dir.path(), "p.wav", WavFormat::Float32, RATE, 2, &src);
    let (player, mut source) = Player::new(PlayerConfig::default()).unwrap();
    player.enqueue([p]);
    player.play().unwrap();
    wait_until(Duration::from_secs(5), || {
        player.status().state == PlaybackState::Playing
    });
    pull(&mut source, 1_000);
    player.pause();
    assert_eq!(player.status().state, PlaybackState::Paused);
    let mut buf = vec![1.0; 512];
    assert_eq!(source.fill(&mut buf), 0);
    assert!(buf.iter().all(|s| *s == 0.0));
    assert_secs(player.status().position, 1_000.0 / 44_100.0);
    player.play().unwrap();
    assert_eq!(player.status().state, PlaybackState::Playing);
    let heard = pull(&mut source, 100);
    assert_close(&heard, &src[2_000..2_200], 1e-7);
}

#[test]
fn next_and_prev_follow_the_heard_track() {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (0..3)
        .map(|i| {
            write_fixture(
                dir.path(),
                &format!("{i}.wav"),
                WavFormat::Float32,
                RATE,
                2,
                &stereo_chirp(RATE, 22_050, 0),
            )
        })
        .collect();
    let (player, mut source) = Player::new(PlayerConfig::default()).unwrap();
    let ids = player.enqueue(paths);
    player.play().unwrap();
    wait_until(Duration::from_secs(5), || {
        player.status().track == Some(ids[0])
    });
    pull(&mut source, 100);
    player.next().unwrap();
    wait_until(Duration::from_secs(5), || {
        player.status().track == Some(ids[1])
    });
    assert_eq!(player.status().position, Duration::ZERO);
    player.prev().unwrap();
    wait_until(Duration::from_secs(5), || {
        player.status().track == Some(ids[0])
    });
    player.play_track(ids[2]).unwrap();
    wait_until(Duration::from_secs(5), || {
        player.status().track == Some(ids[2])
    });
    player.stop().unwrap();
    wait_until(Duration::from_secs(5), || {
        player.status().state == PlaybackState::Stopped
    });
    let mut buf = vec![1.0; 256];
    assert_eq!(source.fill(&mut buf), 0);
}

#[test]
fn unplayable_tracks_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let a_src = stereo_chirp(RATE, 5_000, 0);
    let b_src = stereo_chirp(RATE, 7_000, 5_000);
    let a = write_fixture(dir.path(), "a.wav", WavFormat::Float32, RATE, 2, &a_src);
    let junk = dir.path().join("junk.mp3");
    std::fs::write(&junk, b"this is not audio at all, just text pretending").unwrap();
    let missing = dir.path().join("missing.flac");
    let b = write_fixture(dir.path(), "b.wav", WavFormat::Float32, RATE, 2, &b_src);
    let (player, mut source) = Player::new(PlayerConfig::default()).unwrap();
    player.enqueue([a, junk, missing, b]);
    player.play().unwrap();
    let heard = drain(&player, &mut source);
    let expected: Vec<f32> = a_src.into_iter().chain(b_src).collect();
    assert_close(&heard, &expected, 1e-7);
}

#[test]
fn awkward_file_names_play() {
    let dir = tempfile::tempdir().unwrap();
    let src = stereo_chirp(RATE, 2_000, 0);
    let names = [
        "colon: in name.wav",
        "ünïcödé 日本語 🎵.wav",
        "spaces and #hash%20.wav",
        "-leading-dash.wav",
    ];
    let paths: Vec<_> = names
        .iter()
        .map(|n| write_fixture(dir.path(), n, WavFormat::Float32, RATE, 2, &src))
        .collect();
    for p in &paths {
        assert_close(&decode_all(p, RATE), &src, 1e-7);
    }
}

#[test]
fn repeat_one_loops_the_track() {
    let dir = tempfile::tempdir().unwrap();
    let src = stereo_chirp(RATE, 3_000, 0);
    let p = write_fixture(dir.path(), "r.wav", WavFormat::Float32, RATE, 2, &src);
    let (player, mut source) = Player::new(PlayerConfig::default()).unwrap();
    player.enqueue([p]);
    player.with_queue(|q| q.set_repeat(Repeat::One));
    player.play().unwrap();
    wait_until(Duration::from_secs(5), || {
        player.status().state == PlaybackState::Playing
    });
    let heard = pull(&mut source, 7_500);
    let expected: Vec<f32> = src
        .iter()
        .chain(&src)
        .chain(&src[..3_000])
        .copied()
        .collect();
    assert_close(&heard, &expected, 1e-7);
}

#[test]
fn empty_queue_play_stays_stopped() {
    let (player, mut source) = Player::new(PlayerConfig::default()).unwrap();
    player.play().unwrap();
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(player.status().state, PlaybackState::Stopped);
    let mut buf = vec![0.0; 64];
    assert_eq!(source.fill(&mut buf), 0);
}
