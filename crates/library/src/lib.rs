//! Media library: a SQLite database of the user's tracks, filled by a read-only scanner.
//!
//! The scanner never writes to the music folders. It is incremental: files whose size and
//! modification time are unchanged are skipped, changed ones are re-read, and entries for
//! files that disappeared are removed. A folder that is missing entirely (an unmounted drive)
//! is skipped and its tracks are kept.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

/// File extensions treated as audio (compared case-insensitively).
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "opus", "m4a", "mp4", "aac", "wma", "wav", "aif", "aiff", "ape",
    "wv", "mpc", "mp2", "ac3", "alac", "caf", "webm", "mka",
];

/// Rows written per transaction during a scan.
const BATCH: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = LibraryError> = std::result::Result<T, E>;

pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// What a tag reader extracts from one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    pub year: Option<u32>,
    pub duration: Option<Duration>,
    pub codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub bit_rate: Option<u64>,
}

/// Reads tags from a file. The production reader uses FFmpeg; tests use a fake.
pub trait TagReader {
    fn read(&self, path: &Path) -> std::result::Result<FileTags, String>;
}

/// Tag reading through the playback engine's FFmpeg decoder.
#[derive(Debug, Default, Clone, Copy)]
pub struct FfmpegTags;

impl TagReader for FfmpegTags {
    fn read(&self, path: &Path) -> std::result::Result<FileTags, String> {
        let dec = amp_core::Decoder::open(path, 44_100).map_err(|e| e.to_string())?;
        let i = dec.info();
        Ok(FileTags {
            title: i.title.clone(),
            artist: i.artist.clone(),
            album: i.album.clone(),
            album_artist: i.album_artist.clone(),
            genre: i.genre.clone(),
            track: i.track,
            disc: i.disc,
            year: i.year,
            duration: i.duration,
            codec: Some(i.codec.clone()).filter(|c| !c.is_empty()),
            sample_rate: Some(i.sample_rate).filter(|r| *r > 0),
            channels: Some(i.channels).filter(|c| *c > 0),
            bit_rate: i.bit_rate,
        })
    }
}

/// A track in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub id: i64,
    pub path: PathBuf,
    pub tags: FileTags,
}

impl Track {
    /// "Artist - Title", or the file name without extension when untagged.
    pub fn display_title(&self) -> String {
        match (&self.tags.artist, &self.tags.title) {
            (Some(a), Some(t)) => format!("{a} - {t}"),
            (None, Some(t)) => t.clone(),
            _ => self
                .path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }
}

/// What a scan did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanReport {
    pub roots_scanned: usize,
    /// Folders that did not exist (their tracks were kept).
    pub roots_missing: usize,
    pub files_seen: usize,
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub removed: usize,
    /// Files whose tags could not be read (they are still listed, by file name).
    pub unreadable: usize,
}

/// Narrows a track listing. Unset parts match everything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// Album artist, or artist for tracks without one.
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    /// Text searched in title, artist, album and path.
    pub text: String,
}

/// Overall counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    pub tracks: u64,
    pub artists: u64,
    pub albums: u64,
    pub total_duration: Duration,
}

pub struct Library {
    conn: Connection,
}

impl std::fmt::Debug for Library {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Library")
    }
}

const SCHEMA_VERSION: i64 = 1;

const TRACK_COLUMNS: &str = "id, path, title, artist, album, album_artist, genre, track, disc, \
                             year, duration_ms, codec, sample_rate, channels, bit_rate";

fn modified_secs(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

fn row_to_track(row: &rusqlite::Row<'_>) -> rusqlite::Result<Track> {
    let opt_u32 = |i: usize| -> rusqlite::Result<Option<u32>> {
        Ok(row
            .get::<_, Option<i64>>(i)?
            .and_then(|v| u32::try_from(v).ok()))
    };
    Ok(Track {
        id: row.get(0)?,
        path: PathBuf::from(row.get::<_, String>(1)?),
        tags: FileTags {
            title: row.get(2)?,
            artist: row.get(3)?,
            album: row.get(4)?,
            album_artist: row.get(5)?,
            genre: row.get(6)?,
            track: opt_u32(7)?,
            disc: opt_u32(8)?,
            year: opt_u32(9)?,
            duration: row
                .get::<_, Option<i64>>(10)?
                .and_then(|ms| u64::try_from(ms).ok())
                .map(Duration::from_millis),
            codec: row.get(11)?,
            sample_rate: opt_u32(12)?,
            channels: row
                .get::<_, Option<i64>>(13)?
                .and_then(|v| u16::try_from(v).ok()),
            bit_rate: row
                .get::<_, Option<i64>>(14)?
                .and_then(|v| u64::try_from(v).ok()),
        },
    })
}

/// Escapes `%`, `_` and `\` for a LIKE pattern using `\` as the escape character.
fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Drops roots that are inside other roots, so nested folders are scanned once.
fn distinct_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let norm: Vec<PathBuf> = roots
        .iter()
        .map(|r| std::fs::canonicalize(r).unwrap_or_else(|_| r.clone()))
        .collect();
    let mut out: Vec<PathBuf> = Vec::new();
    for r in &norm {
        if out.iter().any(|o| r.starts_with(o)) {
            continue;
        }
        out.retain(|o| !o.starts_with(r));
        out.push(r.clone());
    }
    out
}

/// Audio files under `root`, in name order, skipping hidden entries.
fn walk(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(root) else {
        log::warn!("cannot read folder {}", root.display());
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() => walk(&path, out),
            Ok(t) if t.is_file() && is_audio(&path) => out.push(path),
            _ => {}
        }
    }
}

impl Library {
    /// Opens (creating if needed) the database at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < SCHEMA_VERSION {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS tracks (
                    id INTEGER PRIMARY KEY,
                    path TEXT NOT NULL UNIQUE,
                    size INTEGER NOT NULL,
                    mtime INTEGER NOT NULL,
                    title TEXT, artist TEXT, album TEXT, album_artist TEXT, genre TEXT,
                    track INTEGER, disc INTEGER, year INTEGER, duration_ms INTEGER,
                    codec TEXT, sample_rate INTEGER, channels INTEGER, bit_rate INTEGER
                );
                CREATE INDEX IF NOT EXISTS tracks_artist ON tracks(artist);
                CREATE INDEX IF NOT EXISTS tracks_album ON tracks(album);
                PRAGMA user_version = 1;",
            )?;
        }
        Ok(Self { conn })
    }

    /// Scans `roots` (read-only), updating the database. `progress` gets the number of files
    /// handled so far and the total found.
    pub fn scan(
        &mut self,
        roots: &[PathBuf],
        reader: &dyn TagReader,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<ScanReport> {
        let mut report = ScanReport::default();
        for root in distinct_roots(roots) {
            if !root.is_dir() {
                log::warn!(
                    "library folder not available, keeping its tracks: {}",
                    root.display()
                );
                report.roots_missing += 1;
                continue;
            }
            report.roots_scanned += 1;
            let mut files = Vec::new();
            walk(&root, &mut files);
            self.scan_files(&root, &files, reader, &mut report, progress)?;
        }
        Ok(report)
    }

    fn scan_files(
        &mut self,
        root: &Path,
        files: &[PathBuf],
        reader: &dyn TagReader,
        report: &mut ScanReport,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<()> {
        let mut seen: HashSet<String> = HashSet::with_capacity(files.len());
        for (batch_index, batch) in files.chunks(BATCH).enumerate() {
            let tx = self.conn.transaction()?;
            for (i, path) in batch.iter().enumerate() {
                progress(batch_index * BATCH + i + 1, files.len());
                let Some(key) = path.to_str().map(str::to_owned) else {
                    report.unreadable += 1;
                    continue;
                };
                let Ok(meta) = std::fs::metadata(path) else {
                    continue;
                };
                report.files_seen += 1;
                seen.insert(key.clone());
                let (size, mtime) = (meta.len() as i64, modified_secs(&meta));
                let known: Option<(i64, i64)> = tx
                    .query_row(
                        "SELECT size, mtime FROM tracks WHERE path = ?1",
                        [&key],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                if known == Some((size, mtime)) {
                    report.unchanged += 1;
                    continue;
                }
                let tags = match reader.read(path) {
                    Ok(t) => t,
                    Err(e) => {
                        log::debug!("unreadable tags: {e}");
                        report.unreadable += 1;
                        FileTags::default()
                    }
                };
                tx.execute(
                    "INSERT INTO tracks (path, size, mtime, title, artist, album, album_artist,
                        genre, track, disc, year, duration_ms, codec, sample_rate, channels,
                        bit_rate)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
                     ON CONFLICT(path) DO UPDATE SET size = ?2, mtime = ?3, title = ?4,
                        artist = ?5, album = ?6, album_artist = ?7, genre = ?8, track = ?9,
                        disc = ?10, year = ?11, duration_ms = ?12, codec = ?13,
                        sample_rate = ?14, channels = ?15, bit_rate = ?16",
                    params![
                        key,
                        size,
                        mtime,
                        tags.title,
                        tags.artist,
                        tags.album,
                        tags.album_artist,
                        tags.genre,
                        tags.track,
                        tags.disc,
                        tags.year,
                        tags.duration.map(|d| d.as_millis() as i64),
                        tags.codec,
                        tags.sample_rate,
                        tags.channels,
                        tags.bit_rate.map(|b| b as i64),
                    ],
                )?;
                if known.is_some() {
                    report.updated += 1;
                } else {
                    report.added += 1;
                }
            }
            tx.commit()?;
        }

        // Remove entries under this root whose files are gone.
        let prefix = format!("{}{}", root.to_string_lossy().trim_end_matches('/'), '/');
        let pattern = format!("{}%", like_escape(&prefix));
        let stale: Vec<i64> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id, path FROM tracks WHERE path LIKE ?1 ESCAPE '\\'")?;
            let rows = stmt.query_map([&pattern], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?;
            let mut stale = Vec::new();
            for row in rows {
                let (id, path) = row?;
                if !seen.contains(&path) {
                    stale.push(id);
                }
            }
            stale
        };
        let tx = self.conn.transaction()?;
        for id in &stale {
            tx.execute("DELETE FROM tracks WHERE id = ?1", [id])?;
        }
        tx.commit()?;
        report.removed += stale.len();
        Ok(())
    }

    /// Tracks whose title, artist, album or path contains `text` (case-insensitive for ASCII),
    /// in album order. An empty query lists everything.
    pub fn search(&self, text: &str, limit: usize) -> Result<Vec<Track>> {
        self.tracks(
            &Filter {
                text: text.to_owned(),
                ..Filter::default()
            },
            limit,
        )
    }

    /// Tracks matching every set part of `filter`, in album order.
    pub fn tracks(&self, filter: &Filter, limit: usize) -> Result<Vec<Track>> {
        let pattern = format!("%{}%", like_escape(filter.text.trim()));
        let sql = format!(
            "SELECT {TRACK_COLUMNS} FROM tracks
             WHERE (title LIKE ?1 ESCAPE '\\' OR artist LIKE ?1 ESCAPE '\\'
                    OR album LIKE ?1 ESCAPE '\\' OR album_artist LIKE ?1 ESCAPE '\\'
                    OR path LIKE ?1 ESCAPE '\\')
               AND (?2 IS NULL OR COALESCE(album_artist, artist) = ?2 OR artist = ?2)
               AND (?3 IS NULL OR album = ?3)
               AND (?4 IS NULL OR genre = ?4)
             ORDER BY COALESCE(album_artist, artist), album, disc, track, path
             LIMIT ?5"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params![
                pattern,
                filter.artist,
                filter.album,
                filter.genre,
                limit as i64
            ],
            row_to_track,
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Distinct values of a grouping with their track counts, sorted by name. Untagged tracks
    /// are not listed.
    fn grouped(&self, expr: &str) -> Result<Vec<(String, u64)>> {
        let sql = format!(
            "SELECT {expr} AS v, COUNT(*) FROM tracks WHERE v IS NOT NULL AND v != ''
             GROUP BY v ORDER BY v COLLATE NOCASE"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Album artists (or artists for tracks without one) and their track counts.
    pub fn artists(&self) -> Result<Vec<(String, u64)>> {
        self.grouped("COALESCE(album_artist, artist)")
    }

    pub fn albums(&self) -> Result<Vec<(String, u64)>> {
        self.grouped("album")
    }

    pub fn genres(&self) -> Result<Vec<(String, u64)>> {
        self.grouped("genre")
    }

    pub fn stats(&self) -> Result<Stats> {
        let (tracks, artists, albums, ms): (i64, i64, i64, Option<i64>) = self.conn.query_row(
            "SELECT COUNT(*), COUNT(DISTINCT COALESCE(album_artist, artist)),
                    COUNT(DISTINCT album), SUM(duration_ms) FROM tracks",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        Ok(Stats {
            tracks: tracks as u64,
            artists: artists as u64,
            albums: albums as u64,
            total_duration: Duration::from_millis(ms.unwrap_or(0).max(0) as u64),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Tags derived from the file name; counts reads; fails for names containing "bad".
    struct FakeTags {
        reads: Cell<usize>,
    }

    impl FakeTags {
        fn new() -> Self {
            Self {
                reads: Cell::new(0),
            }
        }
    }

    impl TagReader for FakeTags {
        fn read(&self, path: &Path) -> std::result::Result<FileTags, String> {
            self.reads.set(self.reads.get() + 1);
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            if stem.contains("bad") {
                return Err("corrupt".into());
            }
            let album = path
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            Ok(FileTags {
                title: Some(stem.clone()),
                artist: Some("Artist".into()),
                album: Some(album),
                track: stem.get(..2).and_then(|n| n.parse().ok()),
                duration: Some(Duration::from_secs(100)),
                ..FileTags::default()
            })
        }
    }

    fn write(dir: &Path, rel: &str, bytes: &[u8]) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    fn scan(lib: &mut Library, roots: &[PathBuf], tags: &FakeTags) -> ScanReport {
        lib.scan(roots, tags, &mut |_, _| {}).unwrap()
    }

    #[test]
    fn incremental_scan_adds_skips_updates_and_removes() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Album A/01 one.flac", b"a");
        write(d, "Album A/02 two.MP3", b"b");
        write(d, "Album A/cover.jpg", b"c");
        write(d, "Album B/.hidden.mp3", b"d");
        write(d, "Album B/03 three.ogg", b"e");
        let mut lib = Library::open(&d.join("db/library.sqlite3")).unwrap();
        let tags = FakeTags::new();
        let roots = vec![d.to_path_buf()];

        let r = scan(&mut lib, &roots, &tags);
        assert_eq!((r.added, r.files_seen, r.removed), (3, 3, 0));
        assert_eq!(tags.reads.get(), 3);

        let r = scan(&mut lib, &roots, &tags);
        assert_eq!((r.added, r.unchanged), (0, 3));
        assert_eq!(tags.reads.get(), 3, "unchanged files are not re-read");

        write(d, "Album A/01 one.flac", b"longer now");
        std::fs::remove_file(d.join("Album B/03 three.ogg")).unwrap();
        let r = scan(&mut lib, &roots, &tags);
        assert_eq!((r.updated, r.removed, r.unchanged), (1, 1, 1));
        assert_eq!(lib.stats().unwrap().tracks, 2);
    }

    #[test]
    fn missing_root_keeps_its_tracks() {
        let dir = tempfile::tempdir().unwrap();
        let drive = dir.path().join("drive");
        write(&drive, "Album/01 a.flac", b"x");
        let mut lib = Library::open_in_memory().unwrap();
        let tags = FakeTags::new();
        scan(&mut lib, std::slice::from_ref(&drive), &tags);
        // The drive is unmounted.
        let parked = dir.path().join("parked");
        std::fs::rename(&drive, &parked).unwrap();
        let r = scan(&mut lib, std::slice::from_ref(&drive), &tags);
        assert_eq!((r.roots_missing, r.removed), (1, 0));
        assert_eq!(lib.stats().unwrap().tracks, 1);
    }

    #[test]
    fn nested_roots_are_scanned_once_and_unreadable_files_still_listed() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Music/Sub/01 x.flac", b"1");
        write(d, "Music/02 bad.mp3", b"2");
        let mut lib = Library::open_in_memory().unwrap();
        let tags = FakeTags::new();
        let r = scan(&mut lib, &[d.join("Music"), d.join("Music/Sub")], &tags);
        assert_eq!((r.roots_scanned, r.added, r.unreadable), (1, 2, 1));
        let bad = lib.search("bad", 10).unwrap();
        assert_eq!(bad.len(), 1);
        assert_eq!(
            bad[0].display_title(),
            "02 bad",
            "untagged shows the file name"
        );
    }

    #[test]
    fn search_orders_by_album_and_track_and_escapes_wildcards() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "B side/02 song.flac", b"1");
        write(d, "B side/01 song.flac", b"2");
        write(d, "A side/05 100% done.flac", b"3");
        write(d, "ハレ晴れ/01 ユカイ.flac", b"4");
        let mut lib = Library::open_in_memory().unwrap();
        scan(&mut lib, &[d.to_path_buf()], &FakeTags::new());
        let all = lib.search("", 100).unwrap();
        let titles: Vec<_> = all.iter().map(|t| t.tags.title.clone().unwrap()).collect();
        assert_eq!(titles, ["05 100% done", "01 song", "02 song", "01 ユカイ"]);
        assert_eq!(lib.search("100%", 10).unwrap().len(), 1);
        assert_eq!(
            lib.search("_", 10).unwrap().len(),
            0,
            "underscore is literal"
        );
        assert_eq!(
            lib.search("SONG", 10).unwrap().len(),
            2,
            "ASCII case-insensitive"
        );
        assert_eq!(lib.search("ユカイ", 10).unwrap().len(), 1);
        let s = lib.stats().unwrap();
        assert_eq!((s.tracks, s.artists, s.albums), (4, 1, 3));
        assert_eq!(s.total_duration, Duration::from_secs(400));
    }

    #[test]
    fn groupings_and_filters() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Alpha/01 a.flac", b"1");
        write(d, "Alpha/02 b.flac", b"2");
        write(d, "beta/01 c.flac", b"3");
        let mut lib = Library::open_in_memory().unwrap();
        scan(&mut lib, &[d.to_path_buf()], &FakeTags::new());
        assert_eq!(
            lib.albums().unwrap(),
            vec![("Alpha".into(), 2), ("beta".into(), 1)]
        );
        assert_eq!(lib.artists().unwrap(), vec![("Artist".into(), 3)]);
        assert!(
            lib.genres().unwrap().is_empty(),
            "untagged genres are not listed"
        );
        let f = Filter {
            album: Some("Alpha".into()),
            text: "b".into(),
            ..Filter::default()
        };
        let got: Vec<_> = lib
            .tracks(&f, 10)
            .unwrap()
            .into_iter()
            .map(|t| t.tags.title.unwrap())
            .collect();
        assert_eq!(got, vec!["02 b"]);
        let by_artist = Filter {
            artist: Some("Artist".into()),
            ..Filter::default()
        };
        assert_eq!(lib.tracks(&by_artist, 10).unwrap().len(), 3);
        assert_eq!(lib.tracks(&by_artist, 2).unwrap().len(), 2, "limit applies");
    }

    #[test]
    fn reopening_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "m/01 a.flac", b"1");
        let db = dir.path().join("lib.sqlite3");
        {
            let mut lib = Library::open(&db).unwrap();
            scan(&mut lib, &[dir.path().join("m")], &FakeTags::new());
        }
        let lib = Library::open(&db).unwrap();
        assert_eq!(lib.stats().unwrap().tracks, 1);
    }
}
