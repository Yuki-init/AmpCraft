//! Playback engine: demux, decode, resample, queue, seek, gapless.
//!
//! [`Decoder`] turns one file into interleaved stereo `f32`. [`Player`] runs a decode thread
//! over a [`Queue`] and feeds an audio output through [`PcmSource`].

pub mod decoder;
pub mod error;
pub mod player;
pub mod queue;
pub mod wav;

pub use decoder::{Decoder, OUTPUT_CHANNELS, TrackInfo};
pub use error::{CoreError, Result};
pub use player::{PcmSource, PlaybackState, Player, PlayerConfig, Status};
pub use queue::{Queue, QueueEntry, Repeat, TrackId};
