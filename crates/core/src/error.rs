use std::path::PathBuf;

/// Errors from the playback engine.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("FFmpeg failed to initialize: {0}")]
    Init(#[source] ffmpeg_next::Error),

    #[error("path is not valid UTF-8: {}", .0.display())]
    NonUtf8Path(PathBuf),

    #[error("could not open {}: {source}", path.display())]
    Open {
        path: PathBuf,
        #[source]
        source: ffmpeg_next::Error,
    },

    #[error("no audio stream in {}", .0.display())]
    NoAudioStream(PathBuf),

    #[error("could not set up the audio decoder: {0}")]
    Codec(#[source] ffmpeg_next::Error),

    #[error("decoding failed: {0}")]
    Decode(#[source] ffmpeg_next::Error),

    #[error("too many consecutive decode errors ({0})")]
    TooManyErrors(u32),

    #[error("resampler failed: {0}")]
    Resample(#[source] ffmpeg_next::Error),

    #[error("seek failed: {0}")]
    Seek(#[source] ffmpeg_next::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("track {0} is not in the queue")]
    NotQueued(crate::queue::TrackId),

    #[error("the playback thread is gone")]
    EngineGone,
}

pub type Result<T, E = CoreError> = std::result::Result<T, E>;
