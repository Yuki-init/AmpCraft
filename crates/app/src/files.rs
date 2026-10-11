//! Turning command-line paths into a list of audio files.

use std::path::{Path, PathBuf};

pub use amp_library::AUDIO_EXTENSIONS;

/// Most files collected from folders, so a mistaken argument cannot stall startup.
const MAX_FILES: usize = 20_000;

pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Makes `p` absolute against the current folder (without resolving symlinks), so saved
/// playlists and settings work from anywhere.
pub fn absolute(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().map_or_else(|_| p.to_path_buf(), |cwd| cwd.join(p))
    }
}

/// Files are kept as given (in order); folders are walked recursively in name order and
/// contribute their audio files. Hidden entries are skipped. Results are absolute.
pub fn expand(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in paths {
        let p = &absolute(p);
        if p.is_dir() {
            walk(p, &mut out);
        } else {
            out.push(p.clone());
        }
        if out.len() >= MAX_FILES {
            out.truncate(MAX_FILES);
            log::warn!("stopped after {MAX_FILES} files");
            break;
        }
    }
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        log::warn!("cannot read folder {}", dir.display());
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if out.len() >= MAX_FILES {
            return;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_expand_sorted_and_files_stay_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::create_dir(d.join("album")).unwrap();
        for name in [
            "album/02.FLAC",
            "album/01.mp3",
            "album/cover.jpg",
            "album/.hidden.mp3",
        ] {
            std::fs::write(d.join(name), b"").unwrap();
        }
        std::fs::write(d.join("single.ogg"), b"").unwrap();
        let got = expand(&[d.join("single.ogg"), d.join("album")]);
        assert!(got.iter().all(|p| p.is_absolute()));
        let names: Vec<_> = got
            .iter()
            .map(|p| p.strip_prefix(d).unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["single.ogg", "album/01.mp3", "album/02.FLAC"]);
    }

    #[test]
    fn audio_extension_check() {
        assert!(is_audio(Path::new("a/B.Mp3")));
        assert!(!is_audio(Path::new("a/b.txt")));
        assert!(!is_audio(Path::new("noext")));
    }
}
