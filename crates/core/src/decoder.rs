//! Single-file decoder: FFmpeg demux and decode, resampled to interleaved stereo `f32`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use ff::format::Sample;
use ff::format::sample::Type as SampleType;
use ff::software::resampling;
use ff::{ChannelLayout, Packet, codec, frame, media};
use ffmpeg_next as ff;

use crate::error::{CoreError, Result};

/// The engine always outputs interleaved stereo.
pub const OUTPUT_CHANNELS: usize = 2;

/// Give up on a file after this many decode or demux errors in a row.
const MAX_CONSECUTIVE_ERRORS: u32 = 64;

/// Initializes FFmpeg once per process. Safe to call repeatedly.
pub fn init() -> Result<()> {
    static INIT: OnceLock<std::result::Result<(), ff::Error>> = OnceLock::new();
    let result = INIT.get_or_init(|| {
        ff::init()?;
        // We report problems ourselves; FFmpeg's own log would spam stderr on damaged files.
        ff::util::log::set_level(ff::util::log::Level::Fatal);
        Ok(())
    });
    result.map_err(CoreError::Init)
}

/// Facts about an opened file.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackInfo {
    pub path: PathBuf,
    pub duration: Option<Duration>,
    /// Source sample rate before resampling.
    pub sample_rate: u32,
    /// Source channel count before downmixing.
    pub channels: u16,
    pub codec: String,
    pub container: String,
    /// Bits per second, if known.
    pub bit_rate: Option<u64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
}

/// Decodes one file into interleaved stereo `f32` at a fixed output rate.
pub struct Decoder {
    input: ff::format::context::Input,
    stream_index: usize,
    /// Seconds per stream timestamp tick.
    time_base: f64,
    /// Stream start timestamp, subtracted so positions start at zero.
    start_ts: i64,
    decoder: ff::decoder::Audio,
    frame: frame::Audio,
    resampler: Option<Resampler>,
    out_rate: u32,
    info: TrackInfo,
    input_eof: bool,
    finished: bool,
    consecutive_errors: u32,
    /// Seek position in seconds, applied to the first frame after a seek.
    seek_target: Option<f64>,
    /// Output frames still to drop to land exactly on the seek target.
    skip_frames: u64,
}

impl std::fmt::Debug for Decoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decoder")
            .field("info", &self.info)
            .field("out_rate", &self.out_rate)
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

impl Decoder {
    /// Opens `path` and prepares to decode its best audio stream at `out_rate` Hz.
    pub fn open(path: &Path, out_rate: u32) -> Result<Self> {
        init()?;
        let url = file_url(path)?;
        let input = ff::format::input(&url).map_err(|source| CoreError::Open {
            path: path.to_path_buf(),
            source,
        })?;

        let stream = input
            .streams()
            .best(media::Type::Audio)
            .ok_or_else(|| CoreError::NoAudioStream(path.to_path_buf()))?;
        let stream_index = stream.index();
        let tb = stream.time_base();
        let time_base = if tb.denominator() != 0 {
            f64::from(tb.numerator()) / f64::from(tb.denominator())
        } else {
            0.0
        };
        let start_ts = match stream.start_time() {
            ff::ffi::AV_NOPTS_VALUE => 0,
            t => t,
        };
        let stream_duration = match stream.duration() {
            d if d > 0 && time_base > 0.0 => Some(Duration::from_secs_f64(d as f64 * time_base)),
            _ => None,
        };

        let ctx = codec::context::Context::from_parameters(stream.parameters())
            .map_err(CoreError::Codec)?;
        let mut dec = ctx.decoder();
        dec.set_packet_time_base(tb);
        let decoder = dec.audio().map_err(CoreError::Codec)?;

        let format_duration = match input.duration() {
            d if d > 0 => Some(Duration::from_micros(d.unsigned_abs())),
            _ => None,
        };
        let format_meta = input.metadata();
        let stream_meta = stream.metadata();
        let tag = |key: &str| {
            format_meta
                .get(key)
                .or_else(|| stream_meta.get(key))
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        let bit_rate = match decoder.bit_rate() {
            0 => u64::try_from(input.bit_rate()).ok().filter(|b| *b > 0),
            b => Some(b as u64),
        };
        let info = TrackInfo {
            path: path.to_path_buf(),
            duration: stream_duration.or(format_duration),
            sample_rate: decoder.rate(),
            channels: decoder.channels(),
            codec: decoder
                .codec()
                .map(|c| c.name().to_owned())
                .unwrap_or_default(),
            container: input.format().name().to_owned(),
            bit_rate,
            title: tag("title"),
            artist: tag("artist"),
            album: tag("album"),
        };

        Ok(Self {
            input,
            stream_index,
            time_base,
            start_ts,
            decoder,
            frame: frame::Audio::empty(),
            resampler: None,
            out_rate,
            info,
            input_eof: false,
            finished: false,
            consecutive_errors: 0,
            seek_target: None,
            skip_frames: 0,
        })
    }

    pub fn info(&self) -> &TrackInfo {
        &self.info
    }

    pub fn out_rate(&self) -> u32 {
        self.out_rate
    }

    /// Appends the next block of interleaved stereo samples to `out`.
    ///
    /// Returns `Ok(false)` once the track is fully decoded and nothing was appended.
    pub fn read(&mut self, out: &mut Vec<f32>) -> Result<bool> {
        loop {
            if self.finished {
                return Ok(false);
            }
            match self.decoder.receive_frame(&mut self.frame) {
                Ok(()) => {
                    self.consecutive_errors = 0;
                    let before = out.len();
                    self.convert_frame(out)?;
                    if out.len() > before {
                        return Ok(true);
                    }
                }
                Err(ff::Error::Other { errno }) if errno == ff::util::error::EAGAIN => {
                    self.feed()?;
                }
                Err(ff::Error::Eof) => {
                    let before = out.len();
                    if let Some(r) = self.resampler.as_mut() {
                        r.flush(out, &mut self.skip_frames)?;
                    }
                    self.finished = true;
                    return Ok(out.len() > before);
                }
                Err(e) => self.note_error(e)?,
            }
        }
    }

    /// Seeks so that the next sample returned is at `pos` (sample accurate).
    pub fn seek(&mut self, pos: Duration) -> Result<()> {
        let ts = i64::try_from(pos.as_micros()).unwrap_or(i64::MAX);
        // RangeTo maps to max_ts, which FFmpeg treats as inclusive.
        self.input.seek(ts, ..ts).map_err(CoreError::Seek)?;
        self.decoder.flush();
        // Samples buffered in the old resampler belong to the old position.
        self.resampler = None;
        self.input_eof = false;
        self.finished = false;
        self.consecutive_errors = 0;
        self.seek_target = Some(pos.as_secs_f64());
        self.skip_frames = 0;
        Ok(())
    }

    /// Feeds the next packet of our stream to the decoder, or signals end of input.
    fn feed(&mut self) -> Result<()> {
        if self.input_eof {
            // The decoder asked for more after EOF was sent. Treat as finished.
            self.finished = true;
            return Ok(());
        }
        loop {
            let mut packet = Packet::empty();
            match packet.read(&mut self.input) {
                Ok(()) => {
                    if packet.stream() != self.stream_index {
                        continue;
                    }
                    match self.decoder.send_packet(&packet) {
                        Ok(()) => return Ok(()),
                        Err(e) => self.note_error(e)?,
                    }
                }
                Err(ff::Error::Eof) => {
                    self.input_eof = true;
                    if let Err(e) = self.decoder.send_eof() {
                        log::debug!("send_eof: {e}");
                        self.finished = true;
                    }
                    return Ok(());
                }
                Err(e) => self.note_error(e)?,
            }
        }
    }

    fn note_error(&mut self, e: ff::Error) -> Result<()> {
        self.consecutive_errors += 1;
        log::debug!("decode error {} in a row: {e}", self.consecutive_errors);
        if self.consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
            return Err(CoreError::TooManyErrors(self.consecutive_errors));
        }
        Ok(())
    }

    fn convert_frame(&mut self, out: &mut Vec<f32>) -> Result<()> {
        if let Some(target) = self.seek_target.take()
            && let Some(ts) = self.frame.timestamp()
        {
            let t = (ts - self.start_ts) as f64 * self.time_base;
            if target > t {
                self.skip_frames = ((target - t) * f64::from(self.out_rate)).round() as u64;
            }
        }

        let channels = self.frame.channels();
        let layout = normalize_layout(self.frame.channel_layout(), channels);
        self.frame.set_channel_layout(layout);
        let key = InputKey {
            format: self.frame.format(),
            layout,
            rate: self.frame.rate(),
        };

        let needs_new = self.resampler.as_ref().is_none_or(|r| r.key != key);
        if needs_new {
            // A mid-stream format change: drain the old resampler first.
            if let Some(mut old) = self.resampler.take() {
                old.flush(out, &mut self.skip_frames)?;
            }
            self.resampler = Some(Resampler::new(key, self.out_rate)?);
        }
        match self.resampler.as_mut() {
            Some(r) => r.run(&self.frame, out, &mut self.skip_frames),
            None => Ok(()),
        }
    }
}

/// Builds an FFmpeg URL that is always treated as a local file, even if the name contains `:`.
fn file_url(path: &Path) -> Result<String> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let s = abs
        .to_str()
        .ok_or_else(|| CoreError::NonUtf8Path(path.to_path_buf()))?;
    Ok(format!("file:{s}"))
}

/// Replaces unspecified or inconsistent layouts with FFmpeg's default for the channel count.
fn normalize_layout(layout: ChannelLayout, channels: u16) -> ChannelLayout {
    let unspec = layout.0.order == ff::ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC;
    if unspec || layout.channels() != i32::from(channels) {
        ChannelLayout::default(i32::from(channels.max(1)))
    } else {
        layout
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct InputKey {
    format: Sample,
    layout: ChannelLayout,
    rate: u32,
}

/// swresample wrapper producing packed `f32` with one or two channels.
struct Resampler {
    key: InputKey,
    ctx: resampling::Context,
    out_rate: u32,
    /// 1 for mono sources (duplicated to both sides afterwards), else 2.
    out_channels: usize,
    out_frame: frame::Audio,
    capacity: usize,
}

impl Resampler {
    fn new(key: InputKey, out_rate: u32) -> Result<Self> {
        // Mono is resampled as mono and copied to both channels at full level, as the classic
        // player does. Letting swresample upmix would place it at -3 dB.
        let out_channels = if key.layout.channels() == 1 { 1 } else { 2 };
        let out_layout = ChannelLayout::default(out_channels as i32);
        let out_format = Sample::F32(SampleType::Packed);
        let ctx = resampling::Context::get(
            key.format, key.layout, key.rate, out_format, out_layout, out_rate,
        )
        .map_err(CoreError::Resample)?;
        let capacity = 8192;
        Ok(Self {
            key,
            ctx,
            out_rate,
            out_channels,
            out_frame: frame::Audio::new(out_format, capacity, out_layout),
            capacity,
        })
    }

    fn ensure_capacity(&mut self, in_samples: usize) {
        let rate_in = self.key.rate.max(1) as usize;
        let needed = in_samples * self.out_rate as usize / rate_in + 256;
        if needed > self.capacity {
            self.capacity = needed.next_power_of_two();
            self.out_frame = frame::Audio::new(
                Sample::F32(SampleType::Packed),
                self.capacity,
                ChannelLayout::default(self.out_channels as i32),
            );
        }
        // With nb_samples zero, swr_convert_frame uses the whole allocated buffer as capacity.
        self.out_frame.set_samples(0);
    }

    fn run(&mut self, input: &frame::Audio, out: &mut Vec<f32>, skip: &mut u64) -> Result<()> {
        self.ensure_capacity(input.samples());
        self.ctx
            .run(input, &mut self.out_frame)
            .map_err(CoreError::Resample)?;
        self.append(out, skip);
        Ok(())
    }

    fn flush(&mut self, out: &mut Vec<f32>, skip: &mut u64) -> Result<()> {
        loop {
            self.ensure_capacity(0);
            self.ctx
                .flush(&mut self.out_frame)
                .map_err(CoreError::Resample)?;
            if self.out_frame.samples() == 0 {
                return Ok(());
            }
            self.append(out, skip);
        }
    }

    fn append(&self, out: &mut Vec<f32>, skip: &mut u64) {
        let frames = self.out_frame.samples();
        let bytes = frames * self.out_channels * 4;
        let data = &self.out_frame.data(0)[..bytes];
        let dropped = (*skip).min(frames as u64) as usize;
        *skip -= dropped as u64;
        let data = &data[dropped * self.out_channels * 4..];
        let samples = data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_ne_bytes(*b));
        if self.out_channels == 1 {
            out.reserve((frames - dropped) * 2);
            for s in samples {
                out.push(s);
                out.push(s);
            }
        } else {
            out.extend(samples);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_url_is_absolute_and_prefixed() {
        let url = file_url(Path::new("/music/a:b.flac")).unwrap();
        assert_eq!(url, "file:/music/a:b.flac");
        let rel = file_url(Path::new("x.mp3")).unwrap();
        assert!(rel.starts_with("file:/"));
        assert!(rel.ends_with("/x.mp3"));
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_path_is_an_error_not_a_panic() {
        use std::os::unix::ffi::OsStrExt;
        let p = Path::new(std::ffi::OsStr::from_bytes(b"/tmp/\xff\xfe.mp3"));
        assert!(matches!(file_url(p), Err(CoreError::NonUtf8Path(_))));
        assert!(matches!(
            Decoder::open(p, 44_100),
            Err(CoreError::NonUtf8Path(_))
        ));
    }

    #[test]
    fn missing_file_is_an_open_error() {
        let err = Decoder::open(Path::new("/nonexistent/nothing.flac"), 44_100).unwrap_err();
        assert!(matches!(err, CoreError::Open { .. }), "{err}");
    }

    #[test]
    fn normalize_layout_fixes_unspecified() {
        let l = normalize_layout(ChannelLayout::default(2), 2);
        assert_eq!(l, ChannelLayout::default(2));
        let fixed = normalize_layout(ChannelLayout::default(2), 1);
        assert_eq!(fixed.channels(), 1);
    }
}
