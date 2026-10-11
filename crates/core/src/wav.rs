//! Minimal WAV writer, used by the CLI's decode command and by tests to generate fixtures.

use std::io::{self, Write};

/// Sample encoding written to the data chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavFormat {
    /// 16-bit signed integer PCM.
    Pcm16,
    /// 32-bit IEEE float.
    Float32,
}

impl WavFormat {
    fn bytes_per_sample(self) -> u16 {
        match self {
            WavFormat::Pcm16 => 2,
            WavFormat::Float32 => 4,
        }
    }

    fn format_tag(self) -> u16 {
        match self {
            WavFormat::Pcm16 => 1,
            WavFormat::Float32 => 3,
        }
    }
}

/// Writes interleaved samples as a complete WAV file.
pub fn write_wav<W: Write>(
    mut out: W,
    format: WavFormat,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
) -> io::Result<()> {
    let bps = format.bytes_per_sample();
    let data_len = samples
        .len()
        .checked_mul(usize::from(bps))
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n <= u32::MAX - 36)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "too much data for WAV"))?;
    let block_align = channels * bps;

    out.write_all(b"RIFF")?;
    out.write_all(&(36 + data_len).to_le_bytes())?;
    out.write_all(b"WAVEfmt ")?;
    out.write_all(&16u32.to_le_bytes())?;
    out.write_all(&format.format_tag().to_le_bytes())?;
    out.write_all(&channels.to_le_bytes())?;
    out.write_all(&sample_rate.to_le_bytes())?;
    out.write_all(&(sample_rate * u32::from(block_align)).to_le_bytes())?;
    out.write_all(&block_align.to_le_bytes())?;
    out.write_all(&(bps * 8).to_le_bytes())?;
    out.write_all(b"data")?;
    out.write_all(&data_len.to_le_bytes())?;

    match format {
        WavFormat::Pcm16 => {
            for s in samples {
                out.write_all(&f32_to_i16(*s).to_le_bytes())?;
            }
        }
        WavFormat::Float32 => {
            for s in samples {
                out.write_all(&s.to_le_bytes())?;
            }
        }
    }
    out.flush()
}

/// Converts a float sample to 16-bit with clipping.
pub fn f32_to_i16(s: f32) -> i16 {
    // The clamp keeps the cast in range; `as` saturates anyway but this documents intent.
    (s.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_44_bytes_and_sizes_match() {
        let mut buf = Vec::new();
        write_wav(
            &mut buf,
            WavFormat::Pcm16,
            44_100,
            2,
            &[0.0, 0.5, -0.5, 1.0],
        )
        .unwrap();
        assert_eq!(buf.len(), 44 + 8);
        assert_eq!(&buf[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(buf[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(u32::from_le_bytes(buf[40..44].try_into().unwrap()), 8);
        assert_eq!(i16::from_le_bytes([buf[46], buf[47]]), 16384);
    }

    #[test]
    fn float_format_tag() {
        let mut buf = Vec::new();
        write_wav(&mut buf, WavFormat::Float32, 48_000, 1, &[0.25]).unwrap();
        assert_eq!(u16::from_le_bytes([buf[20], buf[21]]), 3);
        assert_eq!(f32::from_le_bytes(buf[44..48].try_into().unwrap()), 0.25);
    }

    #[test]
    fn i16_conversion_clips() {
        assert_eq!(f32_to_i16(2.0), 32767);
        assert_eq!(f32_to_i16(-2.0), -32767);
        assert_eq!(f32_to_i16(0.0), 0);
    }
}
