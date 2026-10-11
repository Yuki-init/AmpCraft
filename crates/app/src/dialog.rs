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

fn which(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|dir| PathBuf::from(dir).join(program))
        .find(|p| p.is_file())
}
