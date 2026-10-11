//! Equalizer presets: the built-in list and Winamp `.eqf` preset files.
//!
//! The built-in curves follow the classic preset names. Their shapes come from the values
//! VLC publishes for the same names, scaled into our +/-12 dB range and rounded to half a
//! decibel; the preamp is lowered by half the largest boost so presets do not clip.

/// A named equalizer setting.
#[derive(Debug, Clone, PartialEq)]
pub struct EqPreset {
    pub name: String,
    pub preamp_db: f32,
    pub bands_db: [f32; 10],
}

type Builtin = (&'static str, f32, [f32; 10]);

const BUILTIN: [Builtin; 18] = [
    (
        "Classical",
        0.0,
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -5.0, -5.0, -5.0, -7.0],
    ),
    (
        "Club",
        -3.0,
        [0.0, 0.0, 5.5, 4.0, 4.0, 4.0, 2.5, 0.0, 0.0, 0.0],
    ),
    (
        "Dance",
        -3.5,
        [7.0, 5.0, 1.5, 0.0, 0.0, -4.0, -5.0, -5.0, 0.0, 0.0],
    ),
    ("Flat", 0.0, [0.0; 10]),
    (
        "Full Bass",
        -3.5,
        [-5.5, 7.0, 7.0, 4.0, 1.0, -3.0, -5.5, -7.5, -8.0, -8.0],
    ),
    (
        "Full Bass & Treble",
        -4.0,
        [5.0, 4.0, 0.0, -5.0, -3.5, 1.0, 5.5, 8.0, 8.5, 8.5],
    ),
    (
        "Full Treble",
        -6.0,
        [-7.0, -7.0, -7.0, -3.0, 1.5, 8.0, 11.5, 11.5, 11.5, 12.0],
    ),
    (
        "Headphones",
        -5.0,
        [3.5, 8.0, 4.0, -2.5, -1.5, 1.0, 3.5, 7.0, 9.0, 10.5],
    ),
    (
        "Large Hall",
        -4.0,
        [7.5, 7.5, 4.0, 4.0, 0.0, -3.5, -3.5, -3.5, 0.0, 0.0],
    ),
    (
        "Live",
        -2.0,
        [-3.5, 0.0, 3.0, 4.0, 4.0, 4.0, 3.0, 1.5, 1.5, 1.5],
    ),
    (
        "Party",
        -2.5,
        [5.0, 5.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 5.0, 5.0],
    ),
    (
        "Pop",
        -3.0,
        [-1.0, 3.5, 5.0, 5.5, 4.0, 0.0, -1.5, -1.5, -1.0, -1.0],
    ),
    (
        "Reggae",
        -2.0,
        [0.0, 0.0, 0.0, -4.0, 0.0, 4.5, 4.5, 0.0, 0.0, 0.0],
    ),
    (
        "Rock",
        -4.0,
        [5.5, 3.5, -4.0, -5.5, -2.5, 3.0, 6.5, 8.0, 8.0, 8.0],
    ),
    (
        "Ska",
        -4.0,
        [-1.5, -3.5, -3.0, 0.0, 3.0, 4.0, 6.5, 7.0, 8.0, 7.0],
    ),
    (
        "Soft",
        -4.0,
        [3.5, 1.0, 0.0, -1.5, 0.0, 3.0, 5.5, 7.0, 8.0, 8.5],
    ),
    (
        "Soft Rock",
        -3.0,
        [3.0, 3.0, 1.5, 0.0, -3.0, -4.0, -2.5, 0.0, 1.5, 6.5],
    ),
    (
        "Techno",
        -3.5,
        [5.5, 4.0, 0.0, -4.0, -3.5, 0.0, 5.5, 7.0, 7.0, 6.5],
    ),
];

/// The built-in presets, by name.
pub fn builtin() -> Vec<EqPreset> {
    BUILTIN
        .iter()
        .map(|(name, preamp_db, bands_db)| EqPreset {
            name: (*name).to_owned(),
            preamp_db: *preamp_db,
            bands_db: *bands_db,
        })
        .collect()
}

/// `.eqf` files: a header, then per preset a 257-byte zero-padded name and 11 bytes (10
/// bands, then the preamp). A byte is 0 at +12 dB, 32 at 0 dB and 63 at its lowest.
pub mod eqf {
    use super::EqPreset;

    const HEADER: &[u8] = b"Winamp EQ library file v1.1\x1a!--";
    const NAME_LEN: usize = 257;
    const ENTRY_LEN: usize = NAME_LEN + 11;
    const DB_PER_STEP: f32 = 12.0 / 32.0;

    #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
    pub enum EqfError {
        #[error("not a Winamp EQ preset file")]
        BadHeader,
        #[error("the file has no presets")]
        Empty,
    }

    pub fn db_to_byte(db: f32) -> u8 {
        (32.0 - db / DB_PER_STEP).round().clamp(0.0, 63.0) as u8
    }

    pub fn byte_to_db(b: u8) -> f32 {
        (32.0 - f32::from(b.min(63))) * DB_PER_STEP
    }

    /// Reads every preset in the file (a trailing partial entry is ignored).
    pub fn parse(bytes: &[u8]) -> Result<Vec<EqPreset>, EqfError> {
        let body = bytes.strip_prefix(HEADER).ok_or(EqfError::BadHeader)?;
        let presets: Vec<EqPreset> = body
            .as_chunks::<ENTRY_LEN>()
            .0
            .iter()
            .map(|entry| {
                let (name, values) = entry[..].split_at(NAME_LEN);
                let end = name.iter().position(|b| *b == 0).unwrap_or(NAME_LEN);
                let mut bands_db = [0.0; 10];
                for (db, b) in bands_db.iter_mut().zip(values) {
                    *db = byte_to_db(*b);
                }
                EqPreset {
                    name: String::from_utf8_lossy(&name[..end]).into_owned(),
                    preamp_db: byte_to_db(values[10]),
                    bands_db,
                }
            })
            .collect();
        if presets.is_empty() {
            return Err(EqfError::Empty);
        }
        Ok(presets)
    }

    /// Writes presets as an `.eqf` file. Names longer than the field are cut.
    pub fn write(presets: &[EqPreset]) -> Vec<u8> {
        let mut out = HEADER.to_vec();
        for p in presets {
            let mut name = [0u8; NAME_LEN];
            let bytes = p.name.as_bytes();
            let n = bytes.len().min(NAME_LEN - 1);
            name[..n].copy_from_slice(&bytes[..n]);
            out.extend_from_slice(&name);
            out.extend(p.bands_db.iter().map(|db| db_to_byte(*db)));
            out.push(db_to_byte(p.preamp_db));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_presets_stay_in_range_and_do_not_boost_overall() {
        let all = builtin();
        assert_eq!(all.len(), 18);
        assert!(all.iter().any(|p| p.name == "Rock"));
        let flat = all.iter().find(|p| p.name == "Flat").unwrap();
        assert_eq!((flat.preamp_db, flat.bands_db), (0.0, [0.0; 10]));
        for p in &all {
            for db in p.bands_db.iter().chain([&p.preamp_db]) {
                assert!((-12.0..=12.0).contains(db), "{} {db}", p.name);
                assert_eq!((db * 2.0).fract(), 0.0, "half-dB steps");
            }
            assert!(p.preamp_db <= 0.0, "{}", p.name);
        }
        let mut names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
        let sorted = names.clone();
        names.sort_unstable();
        assert_eq!(names, sorted, "listed by name");
    }

    #[test]
    fn byte_scale_matches_winamp() {
        assert_eq!(eqf::db_to_byte(12.0), 0);
        assert_eq!(eqf::db_to_byte(0.0), 32);
        assert_eq!(eqf::db_to_byte(-12.0), 63, "clamped at the lowest step");
        assert_eq!(eqf::byte_to_db(32), 0.0);
        assert_eq!(eqf::byte_to_db(0), 12.0);
        for b in 0..64u8 {
            assert_eq!(eqf::db_to_byte(eqf::byte_to_db(b)), b);
        }
    }

    #[test]
    fn eqf_round_trip_and_errors() {
        let presets = vec![
            EqPreset {
                name: "Mine".into(),
                preamp_db: -3.0,
                bands_db: [6.0, 3.0, 0.0, -3.0, -6.0, 0.0, 1.5, 3.0, 4.5, 12.0],
            },
            EqPreset {
                name: "x".repeat(300),
                preamp_db: 0.0,
                bands_db: [0.0; 10],
            },
        ];
        let bytes = eqf::write(&presets);
        assert_eq!(bytes.len(), 31 + 2 * 268);
        assert!(bytes.starts_with(b"Winamp EQ library file v1.1"));
        let back = eqf::parse(&bytes).unwrap();
        assert_eq!(back[0], presets[0], "values on the 0.375 dB grid survive");
        assert_eq!(back[1].name.len(), 256);
        assert_eq!(eqf::parse(b"nope"), Err(eqf::EqfError::BadHeader));
        assert_eq!(eqf::parse(&bytes[..31]), Err(eqf::EqfError::Empty));
    }
}
