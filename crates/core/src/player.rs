//! Playback engine.
//!
//! A decode thread fills a lock-free ring buffer with interleaved stereo `f32`. The audio
//! callback drains it through [`PcmSource`], which never allocates, locks or does I/O.
//!
//! Every frame written to the ring has a stream index (frames pushed since the player started).
//! The decode thread records on a timeline which track starts at which stream index, so the
//! position shown to the user follows what is actually being heard, not what is being decoded.
//! The next track is decoded straight after the previous one into the same stream, which makes
//! playback gapless.
//!
//! Seek and stop do not touch the consumer. The decode thread raises `discard_until` to the
//! current write index and the consumer drops everything before it on its next callback.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use rtrb::{Consumer, Producer, RingBuffer};

use crate::decoder::{self, Decoder, OUTPUT_CHANNELS, TrackInfo};
use crate::error::{CoreError, Result};
use crate::queue::{Queue, TrackId};

/// Engine settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerConfig {
    /// Output sample rate in Hz. Every track is resampled to this.
    pub sample_rate: u32,
    /// How much decoded audio to keep ahead of the output.
    pub buffer: Duration,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44_100,
            buffer: Duration::from_millis(500),
        }
    }
}

/// What the user hears right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

/// Snapshot of the player, as heard at the output.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub state: PlaybackState,
    pub track: Option<TrackId>,
    pub position: Duration,
    pub info: Option<Arc<TrackInfo>>,
}

#[derive(Debug)]
enum Command {
    /// Start a track from the top. `None` means the last played track or the first in the queue.
    Play(Option<TrackId>),
    Stop,
    Seek(Duration),
    Next,
    Prev,
    /// Make `id` the track Play starts, without playing.
    Select(TrackId),
    Shutdown,
}

/// A run of the output stream belonging to one track, or a stop marker.
#[derive(Debug, Clone)]
struct Segment {
    /// Stream index of the first frame.
    start: u64,
    track: Option<(TrackId, Arc<TrackInfo>)>,
    /// Track position, in output frames, of the first frame.
    offset: u64,
}

#[derive(Debug, Default)]
struct Timeline {
    segments: VecDeque<Segment>,
}

impl Timeline {
    fn push(&mut self, seg: Segment) {
        // A new segment replaces any that would start at or after it (they were discarded).
        while self.segments.back().is_some_and(|s| s.start >= seg.start) {
            self.segments.pop_back();
        }
        self.segments.push_back(seg);
    }

    /// Drops segments that end before `frame`.
    fn prune(&mut self, frame: u64) {
        while self.segments.len() > 1 && self.segments[1].start <= frame {
            self.segments.pop_front();
        }
    }

    fn at(&self, frame: u64) -> Option<&Segment> {
        self.segments.iter().rev().find(|s| s.start <= frame)
    }
}

/// State shared by the control side, the decode thread and the audio callback.
#[derive(Debug)]
struct Shared {
    sample_rate: u32,
    /// Stream frames taken out of the ring by the consumer, including discarded ones.
    consumed: AtomicU64,
    /// Frames before this stream index are stale (after seek or stop) and must be dropped.
    discard_until: AtomicU64,
    paused: AtomicBool,
    timeline: Mutex<Timeline>,
    queue: Mutex<Queue>,
}

impl Shared {
    fn timeline(&self) -> MutexGuard<'_, Timeline> {
        self.timeline.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn queue(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The segment being heard and the stream index of the frame being heard.
    fn heard(&self) -> (Option<Segment>, u64) {
        let consumed = self.consumed.load(Ordering::Acquire);
        let frame = consumed.max(self.discard_until.load(Ordering::Acquire));
        let mut tl = self.timeline();
        tl.prune(frame);
        (tl.at(frame).cloned(), frame)
    }

    fn status(&self) -> Status {
        let (seg, frame) = self.heard();
        match seg.and_then(|s| s.track.map(|t| (t, s.start, s.offset))) {
            Some(((id, info), start, offset)) => {
                let pos_frames = offset + (frame - start);
                Status {
                    state: if self.paused.load(Ordering::Acquire) {
                        PlaybackState::Paused
                    } else {
                        PlaybackState::Playing
                    },
                    track: Some(id),
                    position: frames_to_duration(pos_frames, self.sample_rate),
                    info: Some(info),
                }
            }
            None => Status {
                state: PlaybackState::Stopped,
                track: None,
                position: Duration::ZERO,
                info: None,
            },
        }
    }
}

fn frames_to_duration(frames: u64, rate: u32) -> Duration {
    let rate = u64::from(rate.max(1));
    Duration::from_secs(frames / rate)
        + Duration::from_nanos((frames % rate) * 1_000_000_000 / rate)
}

fn duration_to_frames(d: Duration, rate: u32) -> u64 {
    (d.as_secs_f64() * f64::from(rate)).round() as u64
}

/// Control handle for the engine. Dropping it stops the decode thread.
#[derive(Debug)]
pub struct Player {
    tx: Sender<Command>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Player {
    /// Starts the engine. Hand the returned [`PcmSource`] to the audio output.
    pub fn new(config: PlayerConfig) -> Result<(Player, PcmSource)> {
        decoder::init()?;
        let rate = config.sample_rate.max(1);
        let buffer_frames = duration_to_frames(config.buffer, rate).max(1024) as usize;
        let (producer, consumer) = RingBuffer::new(buffer_frames * OUTPUT_CHANNELS);
        let shared = Arc::new(Shared {
            sample_rate: rate,
            consumed: AtomicU64::new(0),
            discard_until: AtomicU64::new(0),
            paused: AtomicBool::new(false),
            timeline: Mutex::new(Timeline::default()),
            queue: Mutex::new(Queue::new()),
        });
        let (tx, rx) = mpsc::channel();
        let worker_shared = Arc::clone(&shared);
        // The worker is built on its own thread: FFmpeg handles inside it are not Send.
        let thread = std::thread::Builder::new()
            .name("amp-decode".into())
            .spawn(move || Worker::new(worker_shared, producer, rx).run())?;
        let source = PcmSource {
            consumer,
            shared: Arc::clone(&shared),
            consumed: 0,
        };
        Ok((
            Player {
                tx,
                shared,
                thread: Some(thread),
            },
            source,
        ))
    }

    pub fn sample_rate(&self) -> u32 {
        self.shared.sample_rate
    }

    /// Runs `f` with the queue locked. Keep it short: the decode thread waits on this lock at
    /// track changes.
    pub fn with_queue<R>(&self, f: impl FnOnce(&mut Queue) -> R) -> R {
        f(&mut self.shared.queue())
    }

    /// Appends files to the queue and returns their ids.
    pub fn enqueue<I: IntoIterator<Item = PathBuf>>(&self, paths: I) -> Vec<TrackId> {
        self.with_queue(|q| q.extend(paths))
    }

    /// Resumes if paused, otherwise starts the last played track (or the first) from the top.
    pub fn play(&self) -> Result<()> {
        if self.shared.paused.swap(false, Ordering::AcqRel)
            && self.status().state != PlaybackState::Stopped
        {
            return Ok(());
        }
        self.send(Command::Play(None))
    }

    pub fn play_track(&self, id: TrackId) -> Result<()> {
        self.shared.paused.store(false, Ordering::Release);
        self.send(Command::Play(Some(id)))
    }

    /// Pauses output immediately. Decoding stays buffered.
    pub fn pause(&self) {
        if self.status().state == PlaybackState::Playing {
            self.shared.paused.store(true, Ordering::Release);
        }
    }

    pub fn toggle_pause(&self) {
        match self.status().state {
            PlaybackState::Playing => self.shared.paused.store(true, Ordering::Release),
            PlaybackState::Paused => self.shared.paused.store(false, Ordering::Release),
            PlaybackState::Stopped => {}
        }
    }

    pub fn stop(&self) -> Result<()> {
        self.shared.paused.store(false, Ordering::Release);
        self.send(Command::Stop)
    }

    /// Seeks within the track being heard.
    pub fn seek(&self, pos: Duration) -> Result<()> {
        self.send(Command::Seek(pos))
    }

    /// Makes `id` the track a later [`Player::play`] starts, without starting playback (used
    /// when restoring a saved playlist).
    pub fn select(&self, id: TrackId) -> Result<()> {
        self.send(Command::Select(id))
    }

    pub fn next(&self) -> Result<()> {
        self.send(Command::Next)
    }

    pub fn prev(&self) -> Result<()> {
        self.send(Command::Prev)
    }

    pub fn status(&self) -> Status {
        self.shared.status()
    }

    fn send(&self, cmd: Command) -> Result<()> {
        self.tx.send(cmd).map_err(|_| CoreError::EngineGone)
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(t) = self.thread.take()
            && t.join().is_err()
        {
            log::error!("decode thread panicked");
        }
    }
}

/// The audio-callback side of the engine. Real-time safe.
#[derive(Debug)]
pub struct PcmSource {
    consumer: Consumer<f32>,
    shared: Arc<Shared>,
    /// Local copy of `Shared::consumed`; this side is its only writer.
    consumed: u64,
}

impl PcmSource {
    pub fn sample_rate(&self) -> u32 {
        self.shared.sample_rate
    }

    /// Fills `out` (interleaved stereo) with the next audio, padding with silence when paused,
    /// stopped or starved. Returns the number of frames of real audio written.
    pub fn fill(&mut self, out: &mut [f32]) -> usize {
        let discard = self.shared.discard_until.load(Ordering::Acquire);
        if self.consumed < discard {
            let available = self.consumer.slots() / OUTPUT_CHANNELS;
            let n = (discard - self.consumed).min(available as u64) as usize;
            if let Ok(chunk) = self.consumer.read_chunk(n * OUTPUT_CHANNELS) {
                chunk.commit_all();
                self.consumed += n as u64;
            }
        }

        let mut written = 0;
        if self.consumed >= discard && !self.shared.paused.load(Ordering::Acquire) {
            let want = out.len() / OUTPUT_CHANNELS;
            let available = self.consumer.slots() / OUTPUT_CHANNELS;
            let n = want.min(available);
            let (filled, _) = self
                .consumer
                .pop_partial_slice(&mut out[..n * OUTPUT_CHANNELS]);
            written = filled.len() / OUTPUT_CHANNELS;
            self.consumed += written as u64;
        }
        out[written * OUTPUT_CHANNELS..].fill(0.0);
        self.shared.consumed.store(self.consumed, Ordering::Release);
        written
    }
}

struct Active {
    id: TrackId,
    decoder: Decoder,
}

/// The decode thread.
struct Worker {
    shared: Arc<Shared>,
    producer: Producer<f32>,
    rx: Receiver<Command>,
    current: Option<Active>,
    last_track: Option<TrackId>,
    /// Decoded samples not yet in the ring.
    pending: Vec<f32>,
    pending_pos: usize,
    /// Stream frames written to the ring so far.
    pushed: u64,
}

impl Worker {
    fn new(shared: Arc<Shared>, producer: Producer<f32>, rx: Receiver<Command>) -> Self {
        Self {
            shared,
            producer,
            rx,
            current: None,
            last_track: None,
            pending: Vec::new(),
            pending_pos: 0,
            pushed: 0,
        }
    }

    fn run(mut self) {
        loop {
            let idle = self.current.is_none() && self.pending_frames() == 0;
            let cmd = if idle {
                match self.rx.recv() {
                    Ok(c) => Some(c),
                    Err(_) => return,
                }
            } else {
                match self.rx.try_recv() {
                    Ok(c) => Some(c),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => return,
                }
            };
            if let Some(cmd) = cmd {
                if !self.handle(cmd) {
                    return;
                }
                continue;
            }

            if !self.push_pending() {
                // Ring is full. Wait for room, but stay responsive to commands.
                match self.rx.recv_timeout(Duration::from_millis(5)) {
                    Ok(cmd) => {
                        if !self.handle(cmd) {
                            return;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
                continue;
            }
            self.decode_step();
        }
    }

    /// Returns false on shutdown.
    fn handle(&mut self, cmd: Command) -> bool {
        match cmd {
            Command::Shutdown => return false,
            Command::Play(Some(id)) => self.start(id),
            Command::Play(None) => {
                let queue = self.shared.queue();
                let id = self
                    .last_track
                    .filter(|id| queue.get(*id).is_some())
                    .or_else(|| queue.first());
                drop(queue);
                match id {
                    Some(id) => self.start(id),
                    None => self.stop(),
                }
            }
            Command::Stop => self.stop(),
            Command::Select(id) => {
                if self.current.is_none() {
                    self.last_track = Some(id);
                }
            }
            Command::Seek(pos) => self.seek(pos),
            Command::Next => {
                let Some(id) = self.heard_track() else {
                    return true;
                };
                let next = self.shared.queue().next_after(id);
                if let Some(next) = next {
                    self.start(next);
                }
            }
            Command::Prev => {
                let Some(id) = self.heard_track() else {
                    return true;
                };
                let prev = self.shared.queue().prev_before(id);
                if let Some(prev) = prev {
                    self.start(prev);
                }
            }
        }
        true
    }

    /// The track being heard, or the last one started if nothing is audible.
    fn heard_track(&self) -> Option<TrackId> {
        let (seg, _) = self.shared.heard();
        seg.and_then(|s| s.track.map(|(id, _)| id))
            .or(self.last_track)
    }

    fn pending_frames(&self) -> u64 {
        ((self.pending.len() - self.pending_pos) / OUTPUT_CHANNELS) as u64
    }

    /// Stream index where the next decoded frame will land.
    fn write_pos(&self) -> u64 {
        self.pushed + self.pending_frames()
    }

    /// Moves pending samples into the ring. Returns true when nothing is left pending.
    fn push_pending(&mut self) -> bool {
        let room = self.producer.slots() / OUTPUT_CHANNELS * OUTPUT_CHANNELS;
        let left = self.pending.len() - self.pending_pos;
        let n = room.min(left);
        if n > 0 {
            let slice = &self.pending[self.pending_pos..self.pending_pos + n];
            let (written, _) = self.producer.push_partial_slice(slice);
            let written = written.len();
            self.pending_pos += written;
            self.pushed += (written / OUTPUT_CHANNELS) as u64;
        }
        self.pending_pos == self.pending.len()
    }

    /// Makes everything already in the ring stale.
    fn flush_output(&mut self) {
        self.pending.clear();
        self.pending_pos = 0;
        self.shared
            .discard_until
            .store(self.pushed, Ordering::Release);
    }

    fn mark(&self, track: Option<(TrackId, Arc<TrackInfo>)>, offset: u64) {
        let start = self.write_pos();
        let mut tl = self.shared.timeline();
        tl.push(Segment {
            start,
            track,
            offset,
        });
        tl.prune(self.shared.consumed.load(Ordering::Acquire));
    }

    fn decode_step(&mut self) {
        let Some(active) = self.current.as_mut() else {
            return;
        };
        self.pending.clear();
        self.pending_pos = 0;
        let finished_id = active.id;
        match active.decoder.read(&mut self.pending) {
            Ok(true) => {}
            Ok(false) => self.advance(finished_id),
            Err(e) => {
                log::warn!("decoding stopped early: {e}");
                self.advance(finished_id);
            }
        }
    }

    /// Continues gaplessly after `finished` ended on its own.
    fn advance(&mut self, finished: TrackId) {
        let candidate = self.shared.queue().after_finished(finished);
        self.open_from(candidate);
    }

    /// Opens `candidate`, skipping unplayable tracks. Stops if nothing can be played.
    fn open_from(&mut self, mut candidate: Option<TrackId>) {
        let limit = self.shared.queue().len().max(1);
        for _ in 0..limit {
            let Some(id) = candidate else {
                break;
            };
            match self.open(id) {
                Ok(()) => return,
                Err(e) => {
                    log::warn!("skipping unplayable track: {e}");
                    candidate = self.shared.queue().next_after(id);
                }
            }
        }
        self.current = None;
        self.mark(None, 0);
    }

    fn open(&mut self, id: TrackId) -> Result<()> {
        let path = self
            .shared
            .queue()
            .get(id)
            .map(|e| e.path.clone())
            .ok_or(CoreError::NotQueued(id))?;
        let decoder = Decoder::open(&path, self.shared.sample_rate)?;
        let info = Arc::new(decoder.info().clone());
        self.mark(Some((id, info)), 0);
        self.current = Some(Active { id, decoder });
        self.last_track = Some(id);
        Ok(())
    }

    fn start(&mut self, id: TrackId) {
        self.flush_output();
        self.current = None;
        self.open_from(Some(id));
    }

    fn stop(&mut self) {
        self.flush_output();
        self.current = None;
        self.mark(None, 0);
    }

    fn seek(&mut self, pos: Duration) {
        let Some(id) = self.heard_track() else {
            return;
        };
        self.flush_output();
        // The decoder may already be on the next track (gapless preroll). Reopen if so.
        if self.current.as_ref().is_none_or(|a| a.id != id) {
            self.current = None;
            if let Err(e) = self.open(id) {
                log::warn!("cannot reopen track to seek: {e}");
                let next = self.shared.queue().next_after(id);
                self.open_from(next);
                return;
            }
        }
        let Some(active) = self.current.as_mut() else {
            return;
        };
        let duration = active.decoder.info().duration;
        let pos = duration.map_or(pos, |d| pos.min(d));
        match active.decoder.seek(pos) {
            Ok(()) => {
                let info = Arc::new(active.decoder.info().clone());
                let offset = duration_to_frames(pos, self.shared.sample_rate);
                self.mark(Some((id, info)), offset);
            }
            Err(e) => {
                log::warn!("seek failed: {e}");
                self.advance(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: u64, id: Option<TrackId>) -> Segment {
        let info = Arc::new(TrackInfo {
            path: PathBuf::new(),
            duration: None,
            sample_rate: 0,
            channels: 0,
            codec: String::new(),
            container: String::new(),
            bit_rate: None,
            title: None,
            artist: None,
            album: None,
            album_artist: None,
            genre: None,
            track: None,
            disc: None,
            year: None,
        });
        Segment {
            start,
            track: id.map(|i| (i, info)),
            offset: 0,
        }
    }

    #[test]
    fn timeline_lookup_and_prune() {
        let mut tl = Timeline::default();
        tl.push(seg(0, Some(1)));
        tl.push(seg(100, Some(2)));
        tl.push(seg(250, None));
        assert_eq!(
            tl.at(99).and_then(|s| s.track.as_ref()).map(|t| t.0),
            Some(1)
        );
        assert_eq!(
            tl.at(100).and_then(|s| s.track.as_ref()).map(|t| t.0),
            Some(2)
        );
        assert!(tl.at(300).is_some_and(|s| s.track.is_none()));
        tl.prune(150);
        assert_eq!(tl.segments.len(), 2);
        assert_eq!(
            tl.at(150).and_then(|s| s.track.as_ref()).map(|t| t.0),
            Some(2)
        );
    }

    #[test]
    fn timeline_push_replaces_discarded_segments() {
        let mut tl = Timeline::default();
        tl.push(seg(0, Some(1)));
        tl.push(seg(500, Some(2)));
        tl.push(seg(500, Some(3)));
        assert_eq!(tl.segments.len(), 2);
        assert_eq!(
            tl.at(600).and_then(|s| s.track.as_ref()).map(|t| t.0),
            Some(3)
        );
    }

    #[test]
    fn frame_duration_round_trip() {
        assert_eq!(frames_to_duration(44_100, 44_100), Duration::from_secs(1));
        assert_eq!(
            frames_to_duration(22_050, 44_100),
            Duration::from_millis(500)
        );
        assert_eq!(
            duration_to_frames(Duration::from_millis(250), 48_000),
            12_000
        );
    }

    #[test]
    fn idle_source_outputs_silence() {
        let (player, mut source) = Player::new(PlayerConfig::default()).unwrap();
        let mut buf = vec![1.0; 512];
        assert_eq!(source.fill(&mut buf), 0);
        assert!(buf.iter().all(|s| *s == 0.0));
        assert_eq!(player.status().state, PlaybackState::Stopped);
    }
}
