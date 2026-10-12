//! Dialogs through the desktop's dialog helper (kdialog or zenity), for Linux and other
//! systems without a Win32 implementation.

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
    file_dialog(
        start,
        "Load playlist",
        "Playlists",
        &["m3u", "m3u8", "pls"],
        None,
        done,
    )
}

/// Asks where to save the playlist. Sends the chosen path (possibly without an extension).
pub fn pick_playlist_save(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    file_dialog(
        start,
        "Save playlist",
        "Playlists",
        &["m3u", "m3u8", "pls"],
        Some("playlist.m3u8"),
        done,
    )
}

/// Asks for an equalizer preset file (.eqf, or a .q1 preset library) to load.
pub fn pick_eqf(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    file_dialog(
        start,
        "Load equalizer preset",
        "EQ presets",
        &["eqf", "q1"],
        None,
        done,
    )
}

/// Asks where to save an equalizer preset file.
pub fn pick_eqf_save(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    file_dialog(
        start,
        "Save equalizer preset",
        "EQ presets",
        &["eqf"],
        Some("preset.eqf"),
        done,
    )
}

/// An open dialog, or a save dialog when `save_name` gives the suggested file name.
fn file_dialog(
    start: Option<PathBuf>,
    title: &str,
    filter: &str,
    extensions: &[&str],
    save_name: Option<&str>,
    done: Sender<Vec<PathBuf>>,
) -> bool {
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
    let patterns: Vec<String> = extensions.iter().map(|e| format!("*.{e}")).collect();
    let mut cmd = Command::new(helper);
    if helper == "kdialog" {
        let start = match save_name {
            Some(name) => start.join(name),
            None => start,
        };
        cmd.arg(if save_name.is_some() {
            "--getsavefilename"
        } else {
            "--getopenfilename"
        })
        .arg(&start)
        .arg(format!("{filter} ({})", patterns.join(" ")))
        .args(["--title", title]);
    } else {
        cmd.args(["--file-selection", &format!("--title={title}")])
            .arg(format!("--filename={}/", start.display()))
            .arg(format!("--file-filter={filter} | {}", patterns.join(" ")))
            .arg("--file-filter=All files | *");
        if save_name.is_some() {
            cmd.args(["--save", "--confirm-overwrite"]);
        }
    }
    spawn_first_line(cmd, "file-dialog", done)
}

/// Asks for a line of text (a preset name). Sends it, or nothing when cancelled.
pub fn ask_text(title: &str, label: &str, default: &str, done: Sender<Vec<String>>) -> bool {
    let helper = ["kdialog", "zenity"]
        .into_iter()
        .find(|h| which(h).is_some());
    let Some(helper) = helper else {
        log::warn!("no dialog helper found: install kdialog or zenity");
        return false;
    };
    let mut cmd = Command::new(helper);
    if helper == "kdialog" {
        cmd.args(["--title", title, "--inputbox", label, default]);
    } else {
        cmd.args(["--entry", &format!("--title={title}")])
            .arg(format!("--text={label}"))
            .arg(format!("--entry-text={default}"));
    }
    std::thread::Builder::new()
        .name("text-dialog".into())
        .spawn(move || {
            let text = match cmd.output() {
                Ok(out) if out.status.success() => {
                    let t = String::from_utf8_lossy(&out.stdout).trim().to_owned();
                    if t.is_empty() { Vec::new() } else { vec![t] }
                }
                _ => Vec::new(),
            };
            let _ = done.send(text);
        })
        .is_ok()
}

/// Runs `cmd` on a thread and sends the first line it prints as a path (or nothing).
fn spawn_first_line(mut cmd: Command, name: &str, done: Sender<Vec<PathBuf>>) -> bool {
    std::thread::Builder::new()
        .name(name.into())
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
