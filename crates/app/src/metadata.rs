//! Background probing of queued files for playlist titles and durations, so adding a long
//! list never blocks the UI (the music drive may be slow or asleep).

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use amp_core::{Decoder, TrackId};

/// What the playlist shows for a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackMeta {
    pub id: TrackId,
    /// "Artist - Title" from tags, if present.
    pub title: Option<String>,
    pub duration: Option<Duration>,
}

/// Probes files on a worker thread, in request order.
#[derive(Debug)]
pub struct Prober {
    requests: Sender<(TrackId, PathBuf)>,
    results: Receiver<TrackMeta>,
}

impl Default for Prober {
    fn default() -> Self {
        Self::new()
    }
}

impl Prober {
    pub fn new() -> Self {
        let (requests, rx) = mpsc::channel::<(TrackId, PathBuf)>();
        let (tx, results) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("metadata".into())
            .spawn(move || {
                for (id, path) in rx {
                    let meta = match Decoder::open(&path, 44_100) {
                        Ok(dec) => {
                            let info = dec.info();
                            let title = match (&info.artist, &info.title) {
                                (Some(a), Some(t)) => Some(format!("{a} - {t}")),
                                (None, Some(t)) => Some(t.clone()),
                                _ => None,
                            };
                            TrackMeta {
                                id,
                                title,
                                duration: info.duration,
                            }
                        }
                        Err(e) => {
                            log::debug!("probe failed: {e}");
                            TrackMeta {
                                id,
                                title: None,
                                duration: None,
                            }
                        }
                    };
                    if tx.send(meta).is_err() {
                        return;
                    }
                }
            });
        if let Err(e) = spawned {
            log::warn!("no metadata thread: {e}");
        }
        Self { requests, results }
    }

    pub fn request(&self, id: TrackId, path: PathBuf) {
        let _ = self.requests.send((id, path));
    }

    /// Results that arrived since the last call.
    pub fn drain(&self) -> Vec<TrackMeta> {
        self.results.try_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_files_and_reports_failures() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("t.wav");
        let samples = vec![0.0f32; 44_100 * 2];
        amp_core::wav::write_wav(
            std::fs::File::create(&wav).unwrap(),
            amp_core::wav::WavFormat::Pcm16,
            44_100,
            2,
            &samples,
        )
        .unwrap();
        let p = Prober::new();
        p.request(1, wav);
        p.request(2, dir.path().join("missing.flac"));
        let mut got = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while got.len() < 2 && std::time::Instant::now() < deadline {
            got.extend(p.drain());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].id, 1);
        let d = got[0].duration.unwrap().as_secs_f64();
        assert!((d - 1.0).abs() < 0.01, "{d}");
        assert_eq!(
            got[1],
            TrackMeta {
                id: 2,
                title: None,
                duration: None
            }
        );
    }
}
