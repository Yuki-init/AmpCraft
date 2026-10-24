//! Text for the file info and about boxes.

use std::path::Path;
use std::time::Duration;

use ampcraft_core::TrackInfo;

fn clock(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// What the file info box shows: tags, then the format.
pub fn describe(path: &Path, info: &TrackInfo) -> String {
    let mut lines = vec![path.display().to_string(), String::new()];
    let number = |n: Option<u32>| n.map(|n| n.to_string());
    let tags = [
        ("Title", info.title.clone()),
        ("Artist", info.artist.clone()),
        ("Album", info.album.clone()),
        ("Album artist", info.album_artist.clone()),
        ("Track", number(info.track)),
        ("Disc", number(info.disc)),
        ("Year", number(info.year)),
        ("Genre", info.genre.clone()),
    ];
    let mut any_tag = false;
    for (name, value) in tags {
        if let Some(v) = value.filter(|v| !v.trim().is_empty()) {
            lines.push(format!("{name}: {v}"));
            any_tag = true;
        }
    }
    if !any_tag {
        lines.push("No tags".into());
    }
    lines.push(String::new());
    lines.push(format!("Format: {} in {}", info.codec, info.container));
    let channels = match info.channels {
        1 => "mono".to_owned(),
        2 => "stereo".to_owned(),
        n => format!("{n} channels"),
    };
    lines.push(format!("{} Hz, {channels}", info.sample_rate));
    if let Some(b) = info.bit_rate {
        lines.push(format!("Bit rate: {} kbps", b / 1000));
    }
    if let Some(d) = info.duration {
        lines.push(format!("Length: {}", clock(d)));
    }
    if let Ok(meta) = std::fs::metadata(path) {
        lines.push(format!("Size: {:.1} MB", meta.len() as f64 / 1_000_000.0));
    }
    lines.join("\n")
}

/// The about box.
pub fn about() -> String {
    format!(
        "AmpCraft {}\n\n\
         A classic skinnable audio player.\n\
         Plays classic .wsz skins, follows your desktop's colors\n\
         and comes with built-in themes.\n\n\
         MIT OR Apache-2.0. Audio decoding by FFmpeg (LGPL).",
        env!("CARGO_PKG_VERSION")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> TrackInfo {
        TrackInfo {
            path: "/m/a.flac".into(),
            duration: Some(Duration::from_secs(3725)),
            sample_rate: 44_100,
            channels: 2,
            codec: "flac".into(),
            container: "flac".into(),
            bit_rate: Some(912_000),
            title: Some("God Knows".into()),
            artist: Some("KEIKO".into()),
            album: None,
            album_artist: Some("  ".into()),
            genre: None,
            track: Some(3),
            disc: None,
            year: Some(2006),
        }
    }

    #[test]
    fn describe_lists_tags_and_format() {
        let text = describe(Path::new("/m/a.flac"), &info());
        assert_eq!(
            text,
            "/m/a.flac\n\nTitle: God Knows\nArtist: KEIKO\nTrack: 3\nYear: 2006\n\n\
             Format: flac in flac\n44100 Hz, stereo\nBit rate: 912 kbps\nLength: 1:02:05"
        );
    }

    #[test]
    fn describe_without_tags() {
        let mut i = info();
        (i.title, i.artist, i.track, i.year, i.album_artist) = (None, None, None, None, None);
        i.channels = 6;
        i.duration = Some(Duration::from_secs(65));
        let text = describe(Path::new("/m/a.flac"), &i);
        assert!(text.contains("No tags"));
        assert!(text.contains("6 channels"));
        assert!(text.contains("Length: 1:05"));
    }

    #[test]
    fn about_names_the_version() {
        assert!(about().contains(env!("CARGO_PKG_VERSION")));
    }
}
