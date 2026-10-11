//! Playlist files: M3U / M3U8 (read and write) and PLS (read).
//!
//! Relative entries are resolved against the playlist's folder, `file://` URLs are decoded,
//! and anything else that looks like a URL is skipped (the player has no network access).

use std::path::{Path, PathBuf};
use std::time::Duration;

/// One playlist entry as written to M3U.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistItem {
    pub path: PathBuf,
    /// Display title for the `#EXTINF` line.
    pub title: Option<String>,
    pub duration: Option<Duration>,
}

/// Decodes `%XX` escapes in a file URL path.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Turns one playlist line into a path, or None for URLs and blanks.
fn entry_path(line: &str, base: &Path) -> Option<PathBuf> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    if let Some(rest) = line.strip_prefix("file://") {
        // file:///abs/path or file://localhost/abs/path
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        return Some(PathBuf::from(percent_decode(rest)));
    }
    if line.contains("://") {
        return None;
    }
    // Windows-style separators from playlists made on Windows.
    let line = if cfg!(windows) {
        line.to_owned()
    } else {
        line.replace('\\', "/")
    };
    let p = PathBuf::from(line);
    Some(if p.is_absolute() { p } else { base.join(p) })
}

/// Parses M3U or M3U8 text. `base` is the playlist's folder.
pub fn parse_m3u(text: &str, base: &Path) -> Vec<PathBuf> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| entry_path(l, base))
        .collect()
}

/// Parses PLS text (`FileN=` entries, in N order).
pub fn parse_pls(text: &str, base: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<(u32, PathBuf)> = text
        .lines()
        .filter_map(|l| {
            let (k, v) = l.trim().split_once('=')?;
            let n: u32 = k.trim().strip_prefix("File")?.parse().ok()?;
            Some((n, entry_path(v, base)?))
        })
        .collect();
    entries.sort_by_key(|(n, _)| *n);
    entries.into_iter().map(|(_, p)| p).collect()
}

/// Reads a playlist file, choosing the format by extension (`.pls`, else M3U).
pub fn read_playlist(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8(bytes)
        .unwrap_or_else(|e| e.into_bytes().iter().map(|b| char::from(*b)).collect());
    let base = path.parent().unwrap_or(Path::new("."));
    let is_pls = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pls"));
    Ok(if is_pls {
        parse_pls(&text, base)
    } else {
        parse_m3u(&text, base)
    })
}

/// Extended M3U text (UTF-8) with absolute paths.
pub fn write_m3u(items: &[PlaylistItem]) -> String {
    let mut out = String::from("#EXTM3U\n");
    for item in items {
        if item.title.is_some() || item.duration.is_some() {
            let secs = item.duration.map_or(-1, |d| d.as_secs() as i64);
            let title = item.title.as_deref().unwrap_or("").replace('\n', " ");
            out.push_str(&format!("#EXTINF:{secs},{title}\n"));
        }
        out.push_str(&item.path.to_string_lossy());
        out.push('\n');
    }
    out
}

/// Writes an M3U8 file atomically.
pub fn save_m3u(path: &Path, items: &[PlaylistItem]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("m3u8.tmp");
    std::fs::write(&tmp, write_m3u(items))?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m3u_round_trip_with_titles_and_unicode() {
        let items = vec![
            PlaylistItem {
                path: "/music/ハレ晴れ/01 a.flac".into(),
                title: Some("Ma15 - ハレ晴レユカイ".into()),
                duration: Some(Duration::from_secs(225)),
            },
            PlaylistItem {
                path: "/music/b: colon.mp3".into(),
                title: None,
                duration: None,
            },
        ];
        let text = write_m3u(&items);
        assert!(text.starts_with("#EXTM3U\n#EXTINF:225,Ma15 - ハレ晴レユカイ\n"));
        let paths = parse_m3u(&text, Path::new("/elsewhere"));
        assert_eq!(paths, vec![items[0].path.clone(), items[1].path.clone()]);
    }

    #[test]
    fn m3u_relative_urls_comments_and_bom() {
        let text = "\u{feff}# comment\nsub/a.mp3\n\nfile:///music/My%20Song.flac\nhttp://radio/stream\n  ../b.ogg  \nC:\\Music\\c.mp3\n";
        let paths = parse_m3u(text, Path::new("/lists"));
        let expected: Vec<PathBuf> = if cfg!(windows) {
            vec![]
        } else {
            vec![
                "/lists/sub/a.mp3".into(),
                "/music/My Song.flac".into(),
                "/lists/../b.ogg".into(),
                "/lists/C:/Music/c.mp3".into(),
            ]
        };
        if !cfg!(windows) {
            assert_eq!(paths, expected);
        }
    }

    #[test]
    fn pls_orders_by_number() {
        let text = "[playlist]\nFile2=/m/two.mp3\nTitle2=Two\nFile1=one.mp3\nFile10=/m/ten.mp3\nNumberOfEntries=3\nVersion=2\n";
        assert_eq!(
            parse_pls(text, Path::new("/base")),
            vec![
                PathBuf::from("/base/one.mp3"),
                "/m/two.mp3".into(),
                "/m/ten.mp3".into()
            ]
        );
    }

    #[test]
    fn save_and_read_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("list/playlist.m3u8");
        let items = vec![PlaylistItem {
            path: "/x/y.flac".into(),
            title: Some("T".into()),
            duration: None,
        }];
        save_m3u(&path, &items).unwrap();
        assert_eq!(
            read_playlist(&path).unwrap(),
            vec![PathBuf::from("/x/y.flac")]
        );
        let pls = dir.path().join("a.PLS");
        std::fs::write(&pls, "File1=z.mp3\n").unwrap();
        assert_eq!(read_playlist(&pls).unwrap(), vec![dir.path().join("z.mp3")]);
    }
}
