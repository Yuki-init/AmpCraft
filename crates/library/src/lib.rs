//! Media library: the user's tracks, kept in memory and saved as one JSON file, filled by a
//! read-only scanner.
//!
//! The scanner never writes to the music folders. It is incremental: files whose size and
//! modification time are unchanged are skipped, changed ones are re-read, and entries for
//! files that disappeared are removed. A folder that is missing entirely (an unmounted drive)
//! is skipped and its tracks are kept.
//!
//! Scanning runs in two steps so it can happen on another thread while the player keeps using
//! the library: [`Library::snapshot`] copies what the scanner needs, [`scan`] reads the
//! folders and returns the changes, and [`Library::apply`] merges them. Play counts recorded
//! meanwhile are kept.
//!
//! Saving writes a temporary file next to the library and renames it over the old one, so a
//! crash mid-save leaves the previous file intact. A file that cannot be read is moved aside
//! (`library.json.damaged`), never overwritten.

pub mod playlist;

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// File extensions treated as audio (compared case-insensitively).
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "opus", "m4a", "mp4", "aac", "wma", "wav", "aif", "aiff", "ape",
    "wv", "mpc", "mp2", "ac3", "alac", "caf", "webm", "mka",
];

/// Version of the JSON file format.
const FORMAT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("library file error: {0}")]
    Json(#[from] serde_json::Error),
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileTags {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", with = "millis")]
    pub duration: Option<Duration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_rate: Option<u64>,
}

/// Durations are stored as whole milliseconds.
mod millis {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(d: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        match d {
            Some(d) => s.serialize_u64(u64::try_from(d.as_millis()).unwrap_or(u64::MAX)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        Ok(Option::<u64>::deserialize(d)?.map(Duration::from_millis))
    }
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
        let dec = ampcraft_core::Decoder::open(path, 44_100).map_err(|e| e.to_string())?;
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
    /// Text searched in title, artist, album, album artist, genre and path.
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

/// One track as stored in the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    id: i64,
    path: String,
    size: u64,
    /// File modification time, Unix seconds.
    mtime: i64,
    #[serde(flatten)]
    tags: FileTags,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    added_at: Option<i64>,
    #[serde(default, skip_serializing_if = "is_zero")]
    play_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_played: Option<i64>,
    /// Lowercased (ASCII) search text, built on load and on change.
    #[serde(skip)]
    search: String,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl Entry {
    fn index(&mut self) {
        let t = &self.tags;
        let parts = [&t.title, &t.artist, &t.album, &t.album_artist, &t.genre];
        let mut s = String::new();
        for p in parts.into_iter().flatten() {
            s.push_str(p);
            s.push('\u{1}');
        }
        s.push_str(&self.path);
        self.search = s.to_ascii_lowercase();
    }

    /// Album artist, or artist when there is none (the artist pane's grouping).
    fn group_artist(&self) -> Option<&str> {
        self.tags
            .album_artist
            .as_deref()
            .or(self.tags.artist.as_deref())
    }

    fn folder(&self) -> Option<String> {
        folder_of(Path::new(&self.path))
    }

    fn track(&self) -> Track {
        Track {
            id: self.id,
            path: PathBuf::from(&self.path),
            tags: self.tags.clone(),
            size: self.size,
            plays: PlayStats {
                added_at: self.added_at,
                play_count: self.play_count,
                last_played: self.last_played,
            },
        }
    }

    fn matches(&self, f: &Filter, needle: &str) -> bool {
        (needle.is_empty() || self.search.contains(needle))
            && f.artist.as_ref().is_none_or(|a| {
                self.group_artist() == Some(a.as_str()) || self.tags.artist.as_ref() == Some(a)
            })
            && f.album
                .as_ref()
                .is_none_or(|a| self.tags.album.as_ref() == Some(a))
            && f.genre
                .as_ref()
                .is_none_or(|g| self.tags.genre.as_ref() == Some(g))
            && f.folder
                .as_deref()
                .is_none_or(|d| self.folder().as_deref() == Some(folder_key(d).as_str()))
            && match f.view {
                View::MostPlayed => self.play_count > 0,
                View::RecentlyPlayed => self.last_played.is_some(),
                View::NeverPlayed => self.play_count == 0,
                View::Audio | View::RecentlyAdded | View::Folders => true,
            }
    }
}

/// The file's layout.
#[derive(Debug, Serialize, Deserialize)]
struct LibraryFile {
    version: u32,
    next_id: i64,
    tracks: Vec<Entry>,
}

/// What the scanner needs to know about the library: each known file's size and time.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    known: HashMap<String, (u64, i64)>,
}

/// The changes one scan found, for [`Library::apply`].
#[derive(Debug, Clone, Default)]
pub struct ScanChanges {
    /// New or changed files: path, size, modification time and tags.
    upserts: Vec<(String, u64, i64, FileTags)>,
    /// Files under a scanned folder that are gone.
    removed: Vec<String>,
    pub report: ScanReport,
}

/// The media library.
#[derive(Debug, Clone, Default)]
pub struct Library {
    /// Where it is saved; `None` for a library that lives only in memory.
    file: Option<PathBuf>,
    entries: Vec<Entry>,
    /// Index into `entries` by path.
    by_path: HashMap<String, usize>,
    /// Ids in use.
    ids: HashSet<i64>,
    next_id: i64,
    dirty: bool,
}

fn modified_secs(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// How a folder is stored and compared: its path as text.
fn folder_key(folder: &Path) -> String {
    folder.to_string_lossy().into_owned()
}

/// The folder of a track file: its parent directory.
fn folder_of(path: &Path) -> Option<String> {
    path.parent().map(folder_key)
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Case-insensitive (ASCII) order, then exact, so equal-looking names keep a fixed order.
fn nocase(a: &str, b: &str) -> Ordering {
    a.to_ascii_lowercase()
        .cmp(&b.to_ascii_lowercase())
        .then_with(|| a.cmp(b))
}

/// Album order: album artist (or artist), album, disc, track, path. Missing values first.
fn album_order(a: &Entry, b: &Entry) -> Ordering {
    a.group_artist()
        .cmp(&b.group_artist())
        .then_with(|| a.tags.album.cmp(&b.tags.album))
        .then_with(|| a.tags.disc.cmp(&b.tags.disc))
        .then_with(|| a.tags.track.cmp(&b.tags.track))
        .then_with(|| a.path.cmp(&b.path))
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
    let drive = matches!(b, [letter, b':', b'\\', ..] if letter.is_ascii_alphabetic());
    drive.then(|| rest.to_owned())
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

/// Scans `roots` (read-only) against what `snapshot` knows, reading tags of new and changed
/// files. Touches nothing but the files' metadata and tags; [`Library::apply`] takes the
/// result. `progress` gets the number of files handled so far and the total found.
pub fn scan(
    snapshot: &Snapshot,
    roots: &[PathBuf],
    reader: &dyn TagReader,
    progress: &mut dyn FnMut(usize, usize),
) -> ScanChanges {
    let mut out = ScanChanges::default();
    for root in distinct_roots(roots) {
        if !root.is_dir() {
            log::warn!(
                "library folder not available, keeping its tracks: {}",
                root.display()
            );
            out.report.roots_missing += 1;
            continue;
        }
        out.report.roots_scanned += 1;
        let mut files = Vec::new();
        walk(&root, &mut files);
        let mut seen: HashSet<&str> = HashSet::with_capacity(files.len());
        for (i, path) in files.iter().enumerate() {
            progress(i + 1, files.len());
            let Some(key) = path.to_str() else {
                out.report.unreadable += 1;
                continue;
            };
            let Ok(meta) = std::fs::metadata(path) else {
                continue;
            };
            out.report.files_seen += 1;
            seen.insert(key);
            let (size, mtime) = (meta.len(), modified_secs(&meta));
            let known = snapshot.known.get(key);
            if known == Some(&(size, mtime)) {
                out.report.unchanged += 1;
                continue;
            }
            let tags = reader.read(path).unwrap_or_else(|e| {
                log::debug!("unreadable tags: {e}");
                out.report.unreadable += 1;
                FileTags::default()
            });
            out.upserts.push((key.to_owned(), size, mtime, tags));
            if known.is_some() {
                out.report.updated += 1;
            } else {
                out.report.added += 1;
            }
        }
        // Entries under this root whose files are gone.
        let mut gone: Vec<String> = snapshot
            .known
            .keys()
            .filter(|k| Path::new(k).starts_with(&root) && !seen.contains(k.as_str()))
            .cloned()
            .collect();
        gone.sort();
        out.report.removed += gone.len();
        out.removed.extend(gone);
    }
    out
}

impl Library {
    /// Opens the library saved at `path`, or an empty one when there is no file yet. A file
    /// that cannot be read is renamed to `<name>.damaged` and the library starts empty.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut lib = Library {
            file: Some(path.to_path_buf()),
            next_id: 1,
            ..Library::default()
        };
        let text = match std::fs::read(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(lib),
            Err(e) => return Err(e.into()),
        };
        match serde_json::from_slice::<LibraryFile>(&text) {
            Ok(f) if f.version <= FORMAT_VERSION => {
                lib.next_id = f.next_id.max(1);
                for e in f.tracks {
                    lib.insert(e);
                }
            }
            parsed => {
                let damaged = path.with_extension("json.damaged");
                match parsed {
                    Ok(f) => log::warn!(
                        "library file is from a newer version ({}); setting it aside",
                        f.version
                    ),
                    Err(e) => log::warn!("library file unreadable ({e}); setting it aside"),
                }
                std::fs::rename(path, &damaged)?;
            }
        }
        Ok(lib)
    }

    /// A library that is never saved (tests).
    pub fn open_in_memory() -> Result<Self> {
        Ok(Library {
            next_id: 1,
            ..Library::default()
        })
    }

    /// Adds an entry, keeping ids unique and the path index current.
    fn insert(&mut self, mut e: Entry) {
        if self.by_path.contains_key(&e.path) {
            // A duplicate path in a hand-edited file: the first one wins.
            log::debug!("duplicate library entry {}", e.path);
            return;
        }
        if e.id <= 0 || self.ids.contains(&e.id) {
            e.id = self.next_id;
        }
        self.next_id = self.next_id.max(e.id.saturating_add(1));
        self.ids.insert(e.id);
        e.index();
        self.by_path.insert(e.path.clone(), self.entries.len());
        self.entries.push(e);
    }

    fn reindex(&mut self) {
        self.by_path = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.path.clone(), i))
            .collect();
        self.ids = self.entries.iter().map(|e| e.id).collect();
    }

    /// Whether there are changes not yet saved.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Writes the library to its file (atomically: a temporary file, then a rename).
    pub fn save(&mut self) -> Result<()> {
        let Some(path) = self.file.clone() else {
            self.dirty = false;
            return Ok(());
        };
        let file = LibraryFile {
            version: FORMAT_VERSION,
            next_id: self.next_id,
            tracks: self.entries.clone(),
        };
        let tmp = path.with_extension("json.tmp");
        {
            let mut w = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
            serde_json::to_writer(&mut w, &file)?;
            w.flush()?;
            w.get_ref().sync_all()?;
        }
        std::fs::rename(&tmp, &path)?;
        self.dirty = false;
        Ok(())
    }

    /// Saves when there are unsaved changes.
    pub fn save_if_dirty(&mut self) -> Result<()> {
        if self.dirty { self.save() } else { Ok(()) }
    }

    /// What a scan needs to know, cheap enough to hand to another thread.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            known: self
                .entries
                .iter()
                .map(|e| (e.path.clone(), (e.size, e.mtime)))
                .collect(),
        }
    }

    /// Merges a scan's changes. Play statistics of existing tracks are kept.
    pub fn apply(&mut self, changes: ScanChanges) -> ScanReport {
        let now = now_secs();
        for (path, size, mtime, tags) in changes.upserts {
            if let Some(&i) = self.by_path.get(&path)
                && let Some(e) = self.entries.get_mut(i)
            {
                e.size = size;
                e.mtime = mtime;
                e.tags = tags;
                e.index();
            } else {
                self.insert(Entry {
                    id: self.next_id,
                    path,
                    size,
                    mtime,
                    tags,
                    added_at: Some(now),
                    play_count: 0,
                    last_played: None,
                    search: String::new(),
                });
            }
            self.dirty = true;
        }
        if !changes.removed.is_empty() {
            let gone: HashSet<String> = changes.removed.into_iter().collect();
            self.entries.retain(|e| !gone.contains(&e.path));
            self.reindex();
            self.dirty = true;
        }
        changes.report
    }

    /// Scans `roots` (read-only) on this thread, merges the result and saves.
    pub fn scan(
        &mut self,
        roots: &[PathBuf],
        reader: &dyn TagReader,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<ScanReport> {
        let changes = scan(&self.snapshot(), roots, reader, progress);
        let report = self.apply(changes);
        self.save_if_dirty()?;
        Ok(report)
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

    fn matching<'a>(&'a self, f: &'a Filter) -> impl Iterator<Item = &'a Entry> + 'a {
        let needle = f.text.trim().to_ascii_lowercase();
        self.entries.iter().filter(move |e| e.matches(f, &needle))
    }

    /// Tracks matching every set part of `filter`, in the view's order (album order for
    /// the plain audio view).
    pub fn tracks(&self, filter: &Filter, limit: usize) -> Result<Vec<Track>> {
        let mut found: Vec<&Entry> = self.matching(filter).collect();
        match filter.view {
            View::Audio | View::NeverPlayed => found.sort_by(|a, b| album_order(a, b)),
            View::MostPlayed => found.sort_by(|a, b| {
                b.play_count
                    .cmp(&a.play_count)
                    .then_with(|| b.last_played.cmp(&a.last_played))
                    .then_with(|| a.path.cmp(&b.path))
            }),
            View::RecentlyAdded => {
                found.sort_by(|a, b| b.added_at.cmp(&a.added_at).then_with(|| album_order(a, b)));
            }
            View::RecentlyPlayed => found.sort_by(|a, b| {
                b.last_played
                    .cmp(&a.last_played)
                    .then_with(|| a.path.cmp(&b.path))
            }),
            View::Folders => found.sort_by(|a, b| {
                nocase(
                    &a.folder().unwrap_or_default(),
                    &b.folder().unwrap_or_default(),
                )
                .then_with(|| nocase(&a.path, &b.path))
            }),
        }
        Ok(found.into_iter().take(limit).map(Entry::track).collect())
    }

    /// Artists among the tracks the filter's view and text select (its artist and album are
    /// ignored), with album and track counts, by name.
    pub fn artist_summary(&self, filter: &Filter) -> Result<Vec<ArtistSummary>> {
        let f = Filter {
            artist: None,
            album: None,
            ..filter.clone()
        };
        let mut groups: BTreeMap<&str, (BTreeSet<&str>, u64)> = BTreeMap::new();
        for e in self.matching(&f) {
            let Some(name) = e.group_artist().filter(|n| !n.is_empty()) else {
                continue;
            };
            let g = groups.entry(name).or_default();
            if let Some(album) = e.tags.album.as_deref() {
                g.0.insert(album);
            }
            g.1 += 1;
        }
        let mut out: Vec<ArtistSummary> = groups
            .into_iter()
            .map(|(name, (albums, tracks))| ArtistSummary {
                name: name.to_owned(),
                albums: albums.len() as u64,
                tracks,
            })
            .collect();
        out.sort_by(|a, b| nocase(&a.name, &b.name));
        Ok(out)
    }

    /// Albums among the tracks the filter's view, text and artist select (its album is
    /// ignored), with year and track count, by name.
    pub fn album_summary(&self, filter: &Filter) -> Result<Vec<AlbumSummary>> {
        let f = Filter {
            album: None,
            ..filter.clone()
        };
        let mut groups: BTreeMap<&str, (Option<u32>, u64)> = BTreeMap::new();
        for e in self.matching(&f) {
            let Some(name) = e.tags.album.as_deref().filter(|n| !n.is_empty()) else {
                continue;
            };
            let g = groups.entry(name).or_default();
            g.0 = g.0.max(e.tags.year);
            g.1 += 1;
        }
        let mut out: Vec<AlbumSummary> = groups
            .into_iter()
            .map(|(name, (year, tracks))| AlbumSummary {
                name: name.to_owned(),
                year,
                tracks,
            })
            .collect();
        out.sort_by(|a, b| nocase(&a.name, &b.name));
        Ok(out)
    }

    /// Folders that directly hold tracks the filter's view, text, artist and album select
    /// (its folder is ignored), with track counts, by path.
    pub fn folder_summary(&self, filter: &Filter) -> Result<Vec<FolderSummary>> {
        let f = Filter {
            folder: None,
            ..filter.clone()
        };
        let mut groups: BTreeMap<String, u64> = BTreeMap::new();
        for e in self.matching(&f) {
            if let Some(folder) = e.folder() {
                *groups.entry(folder).or_default() += 1;
            }
        }
        let mut out: Vec<(String, u64)> = groups.into_iter().collect();
        out.sort_by(|a, b| nocase(&a.0, &b.0));
        Ok(out
            .into_iter()
            .map(|(path, tracks)| FolderSummary {
                path: PathBuf::from(path),
                tracks,
            })
            .collect())
    }

    /// Forgets every track under `folder` (a folder taken out of the library). Returns how
    /// many were removed. Files are not touched.
    pub fn remove_folder(&mut self, folder: &Path) -> Result<usize> {
        let before = self.entries.len();
        self.entries
            .retain(|e| !Path::new(&e.path).starts_with(folder));
        let removed = before - self.entries.len();
        if removed > 0 {
            self.reindex();
            self.dirty = true;
            self.save()?;
        }
        Ok(removed)
    }

    /// Counts a play of the file at `path` at `at` (Unix seconds). Returns false when the
    /// file is not in the library. Not saved by itself; call [`Library::save`].
    pub fn record_play(&mut self, path: &Path, at: i64) -> Result<bool> {
        let Some(e) = path
            .to_str()
            .and_then(|k| self.by_path.get(k).copied())
            .and_then(|i| self.entries.get_mut(i))
        else {
            return Ok(false);
        };
        e.play_count = e.play_count.saturating_add(1);
        e.last_played = Some(at);
        self.dirty = true;
        Ok(true)
    }

    /// Distinct non-empty values of a grouping with their track counts, sorted by name.
    fn grouped<'a>(&'a self, value: impl Fn(&'a Entry) -> Option<&'a str>) -> Vec<(String, u64)> {
        let mut groups: BTreeMap<&str, u64> = BTreeMap::new();
        for e in &self.entries {
            if let Some(v) = value(e).filter(|v| !v.is_empty()) {
                *groups.entry(v).or_default() += 1;
            }
        }
        let mut out: Vec<(String, u64)> =
            groups.into_iter().map(|(k, n)| (k.to_owned(), n)).collect();
        out.sort_by(|a, b| nocase(&a.0, &b.0));
        out
    }

    /// Album artists (or artists for tracks without one) and their track counts.
    pub fn artists(&self) -> Result<Vec<(String, u64)>> {
        Ok(self.grouped(Entry::group_artist))
    }

    pub fn albums(&self) -> Result<Vec<(String, u64)>> {
        Ok(self.grouped(|e| e.tags.album.as_deref()))
    }

    pub fn genres(&self) -> Result<Vec<(String, u64)>> {
        Ok(self.grouped(|e| e.tags.genre.as_deref()))
    }

    pub fn stats(&self) -> Result<Stats> {
        let artists: HashSet<&str> = self
            .entries
            .iter()
            .filter_map(Entry::group_artist)
            .collect();
        let albums: HashSet<&str> = self
            .entries
            .iter()
            .filter_map(|e| e.tags.album.as_deref())
            .collect();
        Ok(Stats {
            tracks: self.entries.len() as u64,
            artists: artists.len() as u64,
            albums: albums.len() as u64,
            total_duration: self.entries.iter().filter_map(|e| e.tags.duration).sum(),
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
        let mut lib = Library::open(&d.join("db/library.json")).unwrap();
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
        let one = canonicalize(d).unwrap().join("A/01 one.flac");
        let three = canonicalize(d).unwrap().join("A/03 three.flac");
        assert!(lib.record_play(&three, 100).unwrap());
        assert!(lib.record_play(&one, 200).unwrap());
        assert!(lib.record_play(&three, 300).unwrap());
        assert!(!lib.record_play(&d.join("elsewhere.flac"), 1).unwrap());
        let lib = lib;
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
        let d = canonicalize(d).unwrap();
        assert_eq!(lib.remove_folder(&d.join("drop/")).unwrap(), 1);
        assert_eq!(titles(lib.search("", 10).unwrap()), ["01 c", "01 a"]);
        assert!(d.join("drop/01 b.flac").exists(), "files stay");
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
    fn reopening_keeps_tracks_plays_and_ids() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "m/01 a.flac", b"1");
        write(dir.path(), "m/02 b.flac", b"2");
        let file = dir.path().join("lib.json");
        let a = canonicalize(dir.path()).unwrap().join("m/01 a.flac");
        let ids = {
            let mut lib = Library::open(&file).unwrap();
            scan(&mut lib, &[dir.path().join("m")], &FakeTags::new());
            assert!(lib.record_play(&a, 42).unwrap());
            assert!(lib.is_dirty(), "plays wait for the next save");
            lib.save_if_dirty().unwrap();
            assert!(!lib.is_dirty());
            lib.search("", 10)
                .unwrap()
                .iter()
                .map(|t| t.id)
                .collect::<Vec<_>>()
        };
        let mut lib = Library::open(&file).unwrap();
        assert_eq!(lib.stats().unwrap().tracks, 2);
        let t = lib.search("01 a", 1).unwrap().remove(0);
        assert_eq!((t.plays.play_count, t.plays.last_played), (1, Some(42)));
        assert_eq!(t.tags.duration, Some(Duration::from_secs(100)));
        assert_eq!(
            lib.search("", 10)
                .unwrap()
                .iter()
                .map(|t| t.id)
                .collect::<Vec<_>>(),
            ids,
            "ids are stable"
        );
        // A rescan of unchanged files writes nothing.
        let r = scan(&mut lib, &[dir.path().join("m")], &FakeTags::new());
        assert_eq!(r.unchanged, 2);
        assert!(!lib.is_dirty());
        assert!(!dir.path().join("lib.json.tmp").exists());
    }

    #[test]
    fn a_damaged_file_is_set_aside_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("library.json");
        std::fs::write(&file, b"{ not json").unwrap();
        let mut lib = Library::open(&file).unwrap();
        assert_eq!(lib.stats().unwrap().tracks, 0);
        let damaged = dir.path().join("library.json.damaged");
        assert_eq!(std::fs::read(&damaged).unwrap(), b"{ not json");
        lib.save().unwrap();
        assert_eq!(std::fs::read(&damaged).unwrap(), b"{ not json", "kept");

        std::fs::write(&file, br#"{"version":99,"next_id":1,"tracks":[]}"#).unwrap();
        Library::open(&file).unwrap();
        assert!(!file.exists(), "a newer format is set aside too");
    }

    #[test]
    fn hand_edited_files_with_duplicates_or_bad_ids_load() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("library.json");
        std::fs::write(
            &file,
            br#"{"version":1,"next_id":1,"tracks":[
                {"id":5,"path":"/m/a.flac","size":1,"mtime":1,"title":"A","duration":1500},
                {"id":5,"path":"/m/b.flac","size":1,"mtime":1},
                {"id":0,"path":"/m/a.flac","size":1,"mtime":1}]}"#,
        )
        .unwrap();
        let lib = Library::open(&file).unwrap();
        let all = lib.tracks(&Filter::default(), 10).unwrap();
        assert_eq!(all.len(), 2, "the duplicate path is dropped");
        assert_ne!(all[0].id, all[1].id);
        assert_eq!(all[0].tags.duration, Some(Duration::from_millis(1500)));
    }

    #[test]
    fn plays_during_a_background_scan_survive_it() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "A/01 one.flac", b"1");
        let mut lib = Library::open_in_memory().unwrap();
        scan(&mut lib, &[d.to_path_buf()], &FakeTags::new());
        let one = canonicalize(d).unwrap().join("A/01 one.flac");
        // The scan works from a snapshot while the player keeps counting plays.
        write(d, "A/01 one.flac", b"changed");
        write(d, "A/02 two.flac", b"2");
        let snap = lib.snapshot();
        assert!(lib.record_play(&one, 7).unwrap());
        let changes = super::scan(&snap, &[d.to_path_buf()], &FakeTags::new(), &mut |_, _| {});
        let r = lib.apply(changes);
        assert_eq!((r.added, r.updated), (1, 1));
        let t = lib.search("01 one", 1).unwrap().remove(0);
        assert_eq!(t.plays.play_count, 1);
        assert_eq!(t.size, 7);
    }
}
