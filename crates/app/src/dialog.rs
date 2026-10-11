//! Native file picking through the desktop's dialog helper (kdialog or zenity), run on a
//! thread so the UI keeps drawing while the dialog is open.

use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::Sender;

use crate::files::AUDIO_EXTENSIONS;

/// Asks the user for audio files. Sends the chosen paths (possibly none) on `done`.
/// Returns false if no dialog helper is installed.
pub fn pick_files(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    let helper = ["kdialog", "zenity"]
        .into_iter()
        .find(|h| which(h).is_some());
    let Some(helper) = helper else {
        log::warn!("no file dialog helper found: install kdialog or zenity");
        return false;
    };
    let start = start
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"));
    let patterns: Vec<String> = AUDIO_EXTENSIONS.iter().map(|e| format!("*.{e}")).collect();
    let mut cmd = Command::new(helper);
    if helper == "kdialog" {
        cmd.arg("--getopenfilename")
            .arg(&start)
            .arg(format!("Audio files ({})", patterns.join(" ")))
            .args(["--multiple", "--separate-output", "--title", "Open files"]);
    } else {
        cmd.args([
            "--file-selection",
            "--multiple",
            "--separator=\n",
            "--title=Open files",
        ])
        .arg(format!("--filename={}/", start.display()))
        .arg(format!(
            "--file-filter=Audio files | {}",
            patterns.join(" ")
        ))
        .arg("--file-filter=All files | *");
    }
    let spawned = std::thread::Builder::new()
        .name("file-dialog".into())
        .spawn(move || {
            let paths = match cmd.output() {
                Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(PathBuf::from)
                    .collect(),
                Ok(_) => Vec::new(), // cancelled
                Err(e) => {
                    log::warn!("file dialog failed: {e}");
                    Vec::new()
                }
            };
            let _ = done.send(paths);
        });
    spawned.is_ok()
}

/// Asks the user for one folder. Sends the choice (or nothing) on `done`.
pub fn pick_folder(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    let helper = ["kdialog", "zenity"]
        .into_iter()
        .find(|h| which(h).is_some());
    let Some(helper) = helper else {
        log::warn!("no file dialog helper found: install kdialog or zenity");
        return false;
    };
    let start = start
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"));
    let mut cmd = Command::new(helper);
    if helper == "kdialog" {
        cmd.arg("--getexistingdirectory")
            .arg(&start)
            .args(["--title", "Add folder to library"]);
    } else {
        cmd.args([
            "--file-selection",
            "--directory",
            "--title=Add folder to library",
        ])
        .arg(format!("--filename={}/", start.display()));
    }
    std::thread::Builder::new()
        .name("folder-dialog".into())
        .spawn(move || {
            let paths = match cmd.output() {
                Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(PathBuf::from)
                    .take(1)
                    .collect(),
                _ => Vec::new(),
            };
            let _ = done.send(paths);
        })
        .is_ok()
}

/// Asks for a playlist file to load (.m3u, .m3u8, .pls). Sends the choice on `done`.
pub fn pick_playlist(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    playlist_dialog(start, done, false)
}

/// Asks where to save the playlist. Sends the chosen path (possibly without an extension).
pub fn pick_playlist_save(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    playlist_dialog(start, done, true)
}

fn playlist_dialog(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>, save: bool) -> bool {
    let helper = ["kdialog", "zenity"]
        .into_iter()
        .find(|h| which(h).is_some());
    let Some(helper) = helper else {
        log::warn!("no file dialog helper found: install kdialog or zenity");
        return false;
    };
    let start = start
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"));
    let title = if save {
        "Save playlist"
    } else {
        "Load playlist"
    };
    let filter = "Playlists (*.m3u *.m3u8 *.pls)";
    let mut cmd = Command::new(helper);
    if helper == "kdialog" {
        let start = if save {
            start.join("playlist.m3u8")
        } else {
            start
        };
        cmd.arg(if save {
            "--getsavefilename"
        } else {
            "--getopenfilename"
        })
        .arg(&start)
        .arg(filter)
        .args(["--title", title]);
    } else {
        cmd.args(["--file-selection", &format!("--title={title}")])
            .arg(format!("--filename={}/", start.display()))
            .arg("--file-filter=Playlists | *.m3u *.m3u8 *.pls")
            .arg("--file-filter=All files | *");
        if save {
            cmd.args(["--save", "--confirm-overwrite"]);
        }
    }
    std::thread::Builder::new()
        .name("playlist-dialog".into())
        .spawn(move || {
            let paths = match cmd.output() {
                Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(PathBuf::from)
                    .take(1)
                    .collect(),
                _ => Vec::new(),
            };
            let _ = done.send(paths);
        })
        .is_ok()
}

/// Asks for a classic skin file (.wsz or .zip). Sends the choice on `done`.
pub fn pick_skin(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    let helper = ["kdialog", "zenity"]
        .into_iter()
        .find(|h| which(h).is_some());
    let Some(helper) = helper else {
        log::warn!("no file dialog helper found: install kdialog or zenity");
        return false;
    };
    let start = start
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"));
    let mut cmd = Command::new(helper);
    if helper == "kdialog" {
        cmd.arg("--getopenfilename")
            .arg(&start)
            .arg("Skins (*.wsz *.zip)")
            .args(["--title", "Load skin"]);
    } else {
        cmd.args(["--file-selection", "--title=Load skin"])
            .arg(format!("--filename={}/", start.display()))
            .arg("--file-filter=Skins | *.wsz *.zip")
            .arg("--file-filter=All files | *");
    }
    std::thread::Builder::new()
        .name("skin-dialog".into())
        .spawn(move || {
            let paths = match cmd.output() {
                Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(PathBuf::from)
                    .take(1)
                    .collect(),
                _ => Vec::new(),
            };
            let _ = done.send(paths);
        })
        .is_ok()
}

/// Shows a message box. `text` runs on the dialog's thread first, so slow work (probing a
/// file on a sleeping disk) does not stall the UI. Returns false if no helper is installed.
pub fn show_message(title: &str, text: impl FnOnce() -> String + Send + 'static) -> bool {
    let helper = ["kdialog", "zenity"]
        .into_iter()
        .find(|h| which(h).is_some());
    let Some(helper) = helper else {
        log::warn!("no dialog helper found: install kdialog or zenity");
        return false;
    };
    let title = title.to_owned();
    std::thread::Builder::new()
        .name("message-dialog".into())
        .spawn(move || {
            let text = text();
            let mut cmd = Command::new(helper);
            if helper == "kdialog" {
                cmd.arg("--title").arg(&title).arg("--msgbox").arg(&text);
            } else {
                cmd.args(["--info", "--no-markup", "--no-wrap"])
                    .arg(format!("--title={title}"))
                    .arg(format!("--text={text}"));
            }
            if let Err(e) = cmd.status() {
                log::warn!("message dialog failed: {e}");
            }
        })
        .is_ok()
}

fn which(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|dir| PathBuf::from(dir).join(program))
        .find(|p| p.is_file())
}
