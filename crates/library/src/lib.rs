//! Media library: a SQLite database of the user's tracks, filled by a read-only scanner.
//!
//! The scanner never writes to the music folders. It is incremental: files whose size and
//! modification time are unchanged are skipped, changed ones are re-read, and entries for
//! files that disappeared are removed. A folder that is missing entirely (an unmounted drive)
//! is skipped and its tracks are kept.

pub mod playlist;

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
    /// File size in bytes.
    pub size: u64,
    pub plays: PlayStats,
}

/// When a track joined the library and how it has been played. Times are Unix seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayStats {
    pub added_at: Option<i64>,
    pub play_count: u32,
    pub last_played: Option<i64>,
}

/// The library views (the "Local Media" items of the classic media library).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum View {
    /// Every track, in album order.
    #[default]
    Audio,
    /// Played tracks, most plays first.
    MostPlayed,
    /// Newest additions first.
    RecentlyAdded,
    /// Played tracks, latest first.
    RecentlyPlayed,
    /// Tracks never played.
    NeverPlayed,
    /// Every track, in path order, for browsing by folder (untagged files included).
    Folders,
}

/// One row of the artist filter: name, number of albums, number of tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistSummary {
    pub name: String,
    pub albums: u64,
    pub tracks: u64,
}

/// One row of the album filter: name, year (latest tagged), number of tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumSummary {
    pub name: String,
    pub year: Option<u32>,
    pub tracks: u64,
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

/// One row of the folder filter: a folder that directly holds tracks, and how many.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderSummary {
    pub path: PathBuf,
    pub tracks: u64,
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
    pub view: View,
    /// Album artist, or artist for tracks without one.
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    /// Folder that directly holds the tracks (subfolders are not included).
    pub folder: Option<PathBuf>,
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

const SCHEMA_VERSION: i64 = 3;

const TRACK_COLUMNS: &str = "id, path, title, artist, album, album_artist, genre, track, disc, \
                             year, duration_ms, codec, sample_rate, channels, bit_rate, size, \
                             added_at, play_count, last_played";

fn modified_secs(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

/// The WHERE clause every listing shares: ?1 search pattern, ?2 artist, ?3 album, ?4 genre,
/// ?5 view, ?6 folder.
const FILTER_WHERE: &str = "(title LIKE ?1 ESCAPE '\\' OR artist LIKE ?1 ESCAPE '\\'
        OR album LIKE ?1 ESCAPE '\\' OR album_artist LIKE ?1 ESCAPE '\\'
        OR genre LIKE ?1 ESCAPE '\\' OR path LIKE ?1 ESCAPE '\\')
    AND (?2 IS NULL OR COALESCE(album_artist, artist) = ?2 OR artist = ?2)
    AND (?3 IS NULL OR album = ?3)
    AND (?4 IS NULL OR genre = ?4)
    AND (?6 IS NULL OR folder = ?6)
    AND CASE ?5 WHEN 1 THEN play_count > 0
                WHEN 3 THEN last_played IS NOT NULL
                WHEN 4 THEN play_count = 0
                ELSE 1 END";

/// Parameters for [`FILTER_WHERE`].
type FilterParams = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    Option<String>,
);

fn filter_params(f: &Filter) -> FilterParams {
    let view = match f.view {
        View::Audio | View::Folders => 0,
        View::MostPlayed => 1,
        View::RecentlyAdded => 2,
        View::RecentlyPlayed => 3,
        View::NeverPlayed => 4,
    };
    (
        format!("%{}%", like_escape(f.text.trim())),
        f.artist.clone(),
        f.album.clone(),
        f.genre.clone(),
        view,
        f.folder.as_deref().map(folder_key),
    )
}

/// How a folder is stored and compared: its path as text.
fn folder_key(folder: &Path) -> String {
    folder.to_string_lossy().into_owned()
}

/// The stored folder of a track file: its parent directory.
fn folder_of(path: &Path) -> Option<String> {
    path.parent().map(folder_key)
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
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
        size: u64::try_from(row.get::<_, i64>(15)?).unwrap_or(0),
        plays: PlayStats {
            added_at: row.get(16)?,
            play_count: u32::try_from(row.get::<_, i64>(17)?).unwrap_or(0),
            last_played: row.get(18)?,
        },
    })
}

/// Escapes `%`, `_` and `\` for a LIKE pattern using `\` as the escape character.
fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// `std::fs::canonicalize`, but without the `\\?\` prefix Windows adds to every result when
/// the path works without it (drive and network paths), so stored and shown paths look
/// normal. Rust's file APIs add the prefix again by themselves when a path is too long.
pub fn canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    let full = std::fs::canonicalize(path)?;
    Ok(full
        .to_str()
        .and_then(strip_verbatim)
        .map_or(full.clone(), PathBuf::from))
}

/// `\\?\C:\x` to `C:\x` and `\\?\UNC\server\share` to `\\server\share`; None for anything
/// else (other verbatim forms keep their prefix).
fn strip_verbatim(p: &str) -> Option<String> {
    let rest = p.strip_prefix(r"\\?\")?;
    if let Some(unc) = rest.strip_prefix(r"UNC\") {
        return Some(format!(r"\\{unc}"));
    }
    let b = rest.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\')
        .then(|| rest.to_owned())
}

/// Drops roots that are inside other roots, so nested folders are scanned once.
fn distinct_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let norm: Vec<PathBuf> = roots
        .iter()
        .map(|r| canonicalize(r).unwrap_or_else(|_| r.clone()))
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
        if version < 1 {
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
        if version < 2 {
            // Play statistics. Tracks from before this version count as added when their
            // file was last modified.
            conn.execute_batch(
                "ALTER TABLE tracks ADD COLUMN added_at INTEGER;
                 ALTER TABLE tracks ADD COLUMN play_count INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE tracks ADD COLUMN last_played INTEGER;
                 UPDATE tracks SET added_at = mtime;
                 PRAGMA user_version = 2;",
            )?;
        }
        if version < SCHEMA_VERSION {
            // The folder holding each track, for the folder view.
            conn.execute_batch(
                "ALTER TABLE tracks ADD COLUMN folder TEXT;
                 CREATE INDEX IF NOT EXISTS tracks_folder ON tracks(folder);",
            )?;
            let rows: Vec<(i64, String)> = {
                let mut stmt = conn.prepare("SELECT id, path FROM tracks")?;
                let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
                rows.collect::<rusqlite::Result<_>>()?
            };
            let tx = conn.unchecked_transaction()?;
            for (id, path) in rows {
                tx.execute(
                    "UPDATE tracks SET folder = ?2 WHERE id = ?1",
                    params![id, folder_of(Path::new(&path))],
                )?;
            }
            tx.execute_batch("PRAGMA user_version = 3;")?;
            tx.commit()?;
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
                        bit_rate, added_at, folder)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                        ?17, ?18)
                     ON CONFLICT(path) DO UPDATE SET size = ?2, mtime = ?3, title = ?4,
                        artist = ?5, album = ?6, album_artist = ?7, genre = ?8, track = ?9,
                        disc = ?10, year = ?11, duration_ms = ?12, codec = ?13,
                        sample_rate = ?14, channels = ?15, bit_rate = ?16, folder = ?18",
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
                        now_secs(),
                        folder_of(path),
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

    /// Tracks matching every set part of `filter`, in the view's order (album order for
    /// the plain audio view).
    pub fn tracks(&self, filter: &Filter, limit: usize) -> Result<Vec<Track>> {
        let order = match filter.view {
            View::Audio | View::NeverPlayed => {
                "COALESCE(album_artist, artist), album, disc, track, path"
            }
            View::MostPlayed => "play_count DESC, last_played DESC, path",
            View::RecentlyAdded => {
                "added_at DESC, COALESCE(album_artist, artist), album, disc, track, path"
            }
            View::RecentlyPlayed => "last_played DESC, path",
            View::Folders => "folder COLLATE NOCASE, path COLLATE NOCASE",
        };
        let sql = format!(
            "SELECT {TRACK_COLUMNS} FROM tracks WHERE {FILTER_WHERE} ORDER BY {order} LIMIT ?7"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let p = filter_params(filter);
        let rows = stmt.query_map(
            params![p.0, p.1, p.2, p.3, p.4, p.5, limit as i64],
            row_to_track,
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Artists among the tracks the filter's view and text select (its artist and album are
    /// ignored), with album and track counts, by name.
    pub fn artist_summary(&self, filter: &Filter) -> Result<Vec<ArtistSummary>> {
        let f = Filter {
            artist: None,
            album: None,
            ..filter.clone()
        };
        let sql = format!(
            "SELECT COALESCE(album_artist, artist) AS v, COUNT(DISTINCT album), COUNT(*)
             FROM tracks WHERE {FILTER_WHERE} AND v IS NOT NULL AND v != ''
             GROUP BY v ORDER BY v COLLATE NOCASE"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let p = filter_params(&f);
        let rows = stmt.query_map(params![p.0, p.1, p.2, p.3, p.4, p.5], |r| {
            Ok(ArtistSummary {
                name: r.get(0)?,
                albums: r.get::<_, i64>(1)? as u64,
                tracks: r.get::<_, i64>(2)? as u64,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Albums among the tracks the filter's view, text and artist select (its album is
    /// ignored), with year and track count, by name.
    pub fn album_summary(&self, filter: &Filter) -> Result<Vec<AlbumSummary>> {
        let f = Filter {
            album: None,
            ..filter.clone()
        };
        let sql = format!(
            "SELECT album AS v, MAX(year), COUNT(*)
             FROM tracks WHERE {FILTER_WHERE} AND v IS NOT NULL AND v != ''
             GROUP BY v ORDER BY v COLLATE NOCASE"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let p = filter_params(&f);
        let rows = stmt.query_map(params![p.0, p.1, p.2, p.3, p.4, p.5], |r| {
            Ok(AlbumSummary {
                name: r.get(0)?,
                year: r
                    .get::<_, Option<i64>>(1)?
                    .and_then(|y| u32::try_from(y).ok()),
                tracks: r.get::<_, i64>(2)? as u64,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Folders that directly hold tracks the filter's view, text, artist and album select
    /// (its folder is ignored), with track counts, by path.
    pub fn folder_summary(&self, filter: &Filter) -> Result<Vec<FolderSummary>> {
        let f = Filter {
            folder: None,
            ..filter.clone()
        };
        let sql = format!(
            "SELECT folder AS v, COUNT(*) FROM tracks WHERE {FILTER_WHERE} AND v IS NOT NULL
             GROUP BY v ORDER BY v COLLATE NOCASE"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let p = filter_params(&f);
        let rows = stmt.query_map(params![p.0, p.1, p.2, p.3, p.4, p.5], |r| {
            Ok(FolderSummary {
                path: PathBuf::from(r.get::<_, String>(0)?),
                tracks: r.get::<_, i64>(1)? as u64,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Forgets every track under `folder` (a folder taken out of the library). Returns how
    /// many were removed. Files are not touched.
    pub fn remove_folder(&mut self, folder: &Path) -> Result<usize> {
        let prefix = format!("{}/", folder.to_string_lossy().trim_end_matches('/'));
        let pattern = format!("{}%", like_escape(&prefix));
        Ok(self.conn.execute(
            "DELETE FROM tracks WHERE path LIKE ?1 ESCAPE '\\'",
            [&pattern],
        )?)
    }

    /// Counts a play of the file at `path` at `at` (Unix seconds). Returns false when the
    /// file is not in the library.
    pub fn record_play(&self, path: &Path, at: i64) -> Result<bool> {
        let Some(key) = path.to_str() else {
            return Ok(false);
        };
        let n = self.conn.execute(
            "UPDATE tracks SET play_count = play_count + 1, last_played = ?2 WHERE path = ?1",
            params![key, at],
        )?;
        Ok(n > 0)
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

    #[test]
    fn verbatim_prefix_is_dropped_where_it_is_not_needed() {
        // Regression: the Windows build showed library folders as \\?\C:\...
        assert_eq!(
            strip_verbatim(r"\\?\C:\Users\u\Music").as_deref(),
            Some(r"C:\Users\u\Music")
        );
        assert_eq!(
            strip_verbatim(r"\\?\UNC\nas\music\a").as_deref(),
            Some(r"\\nas\music\a")
        );
        assert_eq!(strip_verbatim(r"\\?\Volume{1234}\x"), None);
        assert_eq!(strip_verbatim(r"C:\already\plain"), None);
        assert_eq!(strip_verbatim("/home/u/Music"), None);
    }

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
        write(d, "Alpha/01 alfa.flac", b"1");
        write(d, "Alpha/02 bravo.flac", b"2");
        write(d, "beta/01 charlie.flac", b"3");
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
            text: "bravo".into(),
            ..Filter::default()
        };
        let got: Vec<_> = lib
            .tracks(&f, 10)
            .unwrap()
            .into_iter()
            .map(|t| t.tags.title.unwrap())
            .collect();
        assert_eq!(got, vec!["02 bravo"]);
        let by_artist = Filter {
            artist: Some("Artist".into()),
            ..Filter::default()
        };
        assert_eq!(lib.tracks(&by_artist, 10).unwrap().len(), 3);
        assert_eq!(lib.tracks(&by_artist, 2).unwrap().len(), 2, "limit applies");
    }

    fn titles(tracks: Vec<Track>) -> Vec<String> {
        tracks.into_iter().map(|t| t.tags.title.unwrap()).collect()
    }

    #[test]
    fn summaries_count_albums_and_tracks() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Alpha/01 alfa.flac", b"1");
        write(d, "Alpha/02 bravo.flac", b"2");
        write(d, "beta/01 charlie.flac", b"3");
        let mut lib = Library::open_in_memory().unwrap();
        scan(&mut lib, &[d.to_path_buf()], &FakeTags::new());
        let artists = lib.artist_summary(&Filter::default()).unwrap();
        assert_eq!(
            artists,
            vec![ArtistSummary {
                name: "Artist".into(),
                albums: 2,
                tracks: 3
            }]
        );
        // The search narrows the panes; the album pane follows the artist.
        let f = Filter {
            text: "charlie".into(),
            artist: Some("Artist".into()),
            album: Some("ignored".into()),
            ..Filter::default()
        };
        assert_eq!(
            lib.album_summary(&f).unwrap(),
            vec![AlbumSummary {
                name: "beta".into(),
                year: None,
                tracks: 1
            }]
        );
        assert_eq!(lib.artist_summary(&f).unwrap()[0].tracks, 1);
        let nobody = Filter {
            artist: Some("Nobody".into()),
            ..Filter::default()
        };
        assert!(lib.album_summary(&nobody).unwrap().is_empty());
    }

    #[test]
    fn plays_drive_the_views() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "A/01 one.flac", b"1");
        write(d, "A/02 two.flac", b"2");
        write(d, "A/03 three.flac", b"3");
        let mut lib = Library::open_in_memory().unwrap();
        scan(&mut lib, &[d.to_path_buf()], &FakeTags::new());
        let one = d.join("A/01 one.flac");
        let three = d.join("A/03 three.flac");
        assert!(lib.record_play(&three, 100).unwrap());
        assert!(lib.record_play(&one, 200).unwrap());
        assert!(lib.record_play(&three, 300).unwrap());
        assert!(!lib.record_play(&d.join("elsewhere.flac"), 1).unwrap());
        let view = |v| {
            titles(
                lib.tracks(
                    &Filter {
                        view: v,
                        ..Filter::default()
                    },
                    10,
                )
                .unwrap(),
            )
        };
        assert_eq!(view(View::MostPlayed), ["03 three", "01 one"]);
        assert_eq!(view(View::RecentlyPlayed), ["03 three", "01 one"]);
        assert_eq!(view(View::NeverPlayed), ["02 two"]);
        assert_eq!(view(View::Audio).len(), 3);
        assert_eq!(view(View::RecentlyAdded).len(), 3);
        let t = lib.search("three", 1).unwrap().remove(0);
        assert_eq!(t.plays.play_count, 2);
        assert_eq!(t.plays.last_played, Some(300));
        assert!(t.plays.added_at.is_some());
        assert_eq!(t.size, 1);
    }

    #[test]
    fn removing_a_folder_forgets_only_its_tracks() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "keep/01 a.flac", b"1");
        write(d, "drop/01 b.flac", b"2");
        write(d, "drop_too_similar/01 c.flac", b"3");
        let mut lib = Library::open_in_memory().unwrap();
        scan(&mut lib, &[d.to_path_buf()], &FakeTags::new());
        assert_eq!(lib.remove_folder(&d.join("drop/")).unwrap(), 1);
        assert_eq!(titles(lib.search("", 10).unwrap()), ["01 c", "01 a"]);
        assert!(d.join("drop/01 b.flac").exists(), "files stay");
    }

    #[test]
    fn version_1_databases_are_upgraded() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("old.sqlite3");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE tracks (
                    id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, size INTEGER NOT NULL,
                    mtime INTEGER NOT NULL, title TEXT, artist TEXT, album TEXT,
                    album_artist TEXT, genre TEXT, track INTEGER, disc INTEGER, year INTEGER,
                    duration_ms INTEGER, codec TEXT, sample_rate INTEGER, channels INTEGER,
                    bit_rate INTEGER);
                 INSERT INTO tracks (path, size, mtime, title) VALUES ('/m/a.flac', 5, 1234, 'A');
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        }
        let lib = Library::open(&db).unwrap();
        let t = lib.search("", 10).unwrap().remove(0);
        assert_eq!(t.plays.added_at, Some(1234), "added when last modified");
        assert_eq!(t.plays.play_count, 0);
        assert!(lib.record_play(Path::new("/m/a.flac"), 99).unwrap());
        assert_eq!(
            lib.folder_summary(&Filter::default()).unwrap(),
            vec![FolderSummary {
                path: PathBuf::from("/m"),
                tracks: 1
            }],
            "folders filled in for old rows"
        );
        drop(lib);
        // Reopening an upgraded database does not migrate again.
        Library::open(&db).unwrap();
    }

    #[test]
    fn folder_view_lists_folders_and_their_own_tracks() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "b/02 two.flac", b"1");
        write(d, "b/01 one.flac", b"2");
        write(d, "b/deeper/01 bad.mp3", b"3");
        write(d, "A/01 x.flac", b"4");
        let mut lib = Library::open_in_memory().unwrap();
        scan(&mut lib, &[d.to_path_buf()], &FakeTags::new());
        let view = Filter {
            view: View::Folders,
            ..Filter::default()
        };
        let canon = canonicalize(d).unwrap();
        assert_eq!(
            lib.folder_summary(&view).unwrap(),
            vec![
                FolderSummary {
                    path: canon.join("A"),
                    tracks: 1
                },
                FolderSummary {
                    path: canon.join("b"),
                    tracks: 2
                },
                FolderSummary {
                    path: canon.join("b/deeper"),
                    tracks: 1
                },
            ]
        );
        let in_b = Filter {
            folder: Some(canon.join("b")),
            ..view.clone()
        };
        assert_eq!(
            titles(lib.tracks(&in_b, 10).unwrap()),
            ["01 one", "02 two"],
            "file order, subfolders left out"
        );
        // Untagged files are listed under their folder too.
        let deeper = Filter {
            folder: Some(canon.join("b/deeper")),
            ..view.clone()
        };
        let t = lib.tracks(&deeper, 10).unwrap();
        assert_eq!(t[0].display_title(), "01 bad");
        // Search narrows the folder list.
        let found = Filter {
            text: "two".into(),
            ..view
        };
        assert_eq!(lib.folder_summary(&found).unwrap().len(), 1);
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
