//! Dialogs from the Windows common dialogs (file open and save), the shell's folder picker,
//! `MessageBoxW`, and a small dialog built from an in-memory template for text prompts.
//! Each runs modally on its own thread, with COM initialized there as the shell requires.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows_sys::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_ALLOWMULTISELECT, OFN_EXPLORER,
    OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST,
    OPENFILENAMEW,
};
use windows_sys::Win32::UI::Shell::{
    BFFM_INITIALIZED, BFFM_SETSELECTIONW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW,
    SHBrowseForFolderW, SHGetPathFromIDListW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DLGTEMPLATE, DialogBoxIndirectParamW, EndDialog, GetDlgItemTextW, MB_ICONINFORMATION, MB_OK,
    MB_SETFOREGROUND, MessageBoxW, SendDlgItemMessageW, SendMessageW, SetDlgItemTextW, WM_COMMAND,
    WM_INITDIALOG,
};

use super::win32_data::{ID_CANCEL, ID_EDIT, ID_OK, chosen_files, filter, prompt_template, wide};
use crate::files::AUDIO_EXTENSIONS;

/// `EM_SETSEL` (CommCtrl.h): select a range of an edit control's text.
const EM_SETSEL: u32 = 0x00B1;
/// Room for a long multiple selection.
const FILE_BUFFER: usize = 64 * 1024;
const MAX_PATH: usize = 260;

/// Runs `work` on a named thread with COM initialized, sending its result on `done`.
fn spawn<T: Send + 'static>(
    name: &str,
    work: impl FnOnce() -> T + Send + 'static,
    done: Sender<T>,
) -> bool {
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            // SAFETY: initializes COM for this new thread; balanced below.
            let hr = unsafe {
                CoInitializeEx(
                    std::ptr::null(),
                    (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as _,
                )
            };
            let result = work();
            if hr >= 0 {
                // SAFETY: pairs the successful CoInitializeEx on this thread.
                unsafe { CoUninitialize() };
            }
            let _ = done.send(result);
        })
        .is_ok()
}

struct FileRequest {
    title: String,
    filter: Vec<u16>,
    start: Option<PathBuf>,
    /// A save dialog with this suggested name; otherwise an open dialog.
    save_name: Option<String>,
    multiple: bool,
}

/// Shows an open or save dialog and returns the chosen paths (none when cancelled).
fn file_dialog(req: FileRequest) -> Vec<PathBuf> {
    let mut buffer = vec![0u16; FILE_BUFFER];
    if let Some(name) = &req.save_name {
        for (dst, src) in buffer.iter_mut().zip(name.encode_utf16()) {
            *dst = src;
        }
    }
    let title = wide(&req.title);
    let start = req.start.as_ref().map(|p| wide(&p.to_string_lossy()));
    // SAFETY: OPENFILENAMEW is plain data; zero is "unset" for every field.
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.lpstrFilter = req.filter.as_ptr();
    ofn.nFilterIndex = 1;
    ofn.lpstrFile = buffer.as_mut_ptr();
    ofn.nMaxFile = buffer.len() as u32;
    ofn.lpstrInitialDir = start.as_ref().map_or(std::ptr::null(), |s| s.as_ptr());
    ofn.lpstrTitle = title.as_ptr();
    ofn.Flags = OFN_EXPLORER | OFN_NOCHANGEDIR | OFN_HIDEREADONLY | OFN_PATHMUSTEXIST;
    let saving = req.save_name.is_some();
    if saving {
        ofn.Flags |= OFN_OVERWRITEPROMPT;
    } else {
        ofn.Flags |= OFN_FILEMUSTEXIST;
        if req.multiple {
            ofn.Flags |= OFN_ALLOWMULTISELECT;
        }
    }
    // SAFETY: every pointer in `ofn` refers to a buffer that lives until after the call, and
    // `nMaxFile` is the length of `buffer`.
    let ok = unsafe {
        if saving {
            GetSaveFileNameW(&mut ofn)
        } else {
            GetOpenFileNameW(&mut ofn)
        }
    };
    if ok == 0 {
        // SAFETY: reads the common dialogs' last error for this thread.
        let err = unsafe { CommDlgExtendedError() };
        if err != 0 {
            log::warn!("file dialog failed (error {err:#x})");
        }
        return Vec::new();
    }
    chosen_files(&buffer)
}

fn file_request(
    start: Option<PathBuf>,
    title: &str,
    filter_name: &str,
    extensions: &[&str],
    save_name: Option<&str>,
    multiple: bool,
) -> FileRequest {
    FileRequest {
        title: title.to_owned(),
        filter: filter(filter_name, extensions),
        start,
        save_name: save_name.map(str::to_owned),
        multiple,
    }
}

/// Asks the user for audio files. Sends the chosen paths (possibly none) on `done`.
pub fn pick_files(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    let req = file_request(
        start,
        "Open files",
        "Audio files",
        AUDIO_EXTENSIONS,
        None,
        true,
    );
    spawn("file-dialog", move || file_dialog(req), done)
}

/// Asks for a playlist file to load (.m3u, .m3u8, .pls). Sends the choice on `done`.
pub fn pick_playlist(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    let req = file_request(
        start,
        "Load playlist",
        "Playlists",
        &["m3u", "m3u8", "pls"],
        None,
        false,
    );
    spawn("file-dialog", move || file_dialog(req), done)
}

/// Asks where to save the playlist. Sends the chosen path (possibly without an extension).
pub fn pick_playlist_save(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    let req = file_request(
        start,
        "Save playlist",
        "Playlists",
        &["m3u8", "m3u", "pls"],
        Some("playlist.m3u8"),
        false,
    );
    spawn("file-dialog", move || file_dialog(req), done)
}

/// Asks for an equalizer preset file (.eqf, or a .q1 preset library) to load.
pub fn pick_eqf(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    let req = file_request(
        start,
        "Load equalizer preset",
        "EQ presets",
        &["eqf", "q1"],
        None,
        false,
    );
    spawn("file-dialog", move || file_dialog(req), done)
}

/// Asks where to save an equalizer preset file.
pub fn pick_eqf_save(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    let req = file_request(
        start,
        "Save equalizer preset",
        "EQ presets",
        &["eqf"],
        Some("preset.eqf"),
        false,
    );
    spawn("file-dialog", move || file_dialog(req), done)
}

/// Asks for a classic skin file (.wsz or .zip). Sends the choice on `done`.
pub fn pick_skin(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    let req = file_request(start, "Load skin", "Skins", &["wsz", "zip"], None, false);
    spawn("skin-dialog", move || file_dialog(req), done)
}

extern "system" fn browse_callback(hwnd: HWND, msg: u32, _lp: LPARAM, data: LPARAM) -> i32 {
    if msg == BFFM_INITIALIZED && data != 0 {
        // SAFETY: `data` is the NUL-terminated start folder `pick_folder` passes, alive for
        // the whole dialog; TRUE (1) means it is a path, not an item list.
        unsafe { SendMessageW(hwnd, BFFM_SETSELECTIONW, 1, data) };
    }
    0
}

/// Asks the user for one folder. Sends the choice (or nothing) on `done`.
pub fn pick_folder(start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    let work = move || {
        let title = wide("Add folder to library");
        let start = start.map(|p| wide(&p.to_string_lossy()));
        let mut name = vec![0u16; MAX_PATH];
        let info = BROWSEINFOW {
            hwndOwner: std::ptr::null_mut(),
            pidlRoot: std::ptr::null_mut(),
            pszDisplayName: name.as_mut_ptr(),
            lpszTitle: title.as_ptr(),
            ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
            lpfn: Some(browse_callback),
            lParam: start.as_ref().map_or(0, |s| s.as_ptr() as LPARAM),
            iImage: 0,
        };
        // SAFETY: `info` and the buffers it points to live across the call.
        let pidl = unsafe { SHBrowseForFolderW(&info) };
        if pidl.is_null() {
            return Vec::new();
        }
        let mut path = vec![0u16; 32 * 1024];
        // SAFETY: `pidl` came from the dialog; SHGetPathFromIDListW writes at most MAX_PATH
        // characters, well within `path`. The item list is ours to free.
        let ok = unsafe { SHGetPathFromIDListW(pidl, path.as_mut_ptr()) };
        // SAFETY: frees the item list the shell allocated for us.
        unsafe { CoTaskMemFree(pidl as *const core::ffi::c_void) };
        if ok == 0 {
            return Vec::new();
        }
        chosen_files(&path)
    };
    spawn("folder-dialog", work, done)
}

thread_local! {
    /// The text prompt's default text in, and the entered text out (one prompt per thread).
    static PROMPT: RefCell<(Vec<u16>, Option<String>)> = const { RefCell::new((Vec::new(), None)) };
}

extern "system" fn prompt_proc(dlg: HWND, msg: u32, wp: WPARAM, _lp: LPARAM) -> isize {
    match msg {
        WM_INITDIALOG => {
            PROMPT.with(|p| {
                let p = p.borrow();
                // SAFETY: `dlg` is the dialog being created and the text is NUL-terminated.
                unsafe {
                    SetDlgItemTextW(dlg, i32::from(ID_EDIT), p.0.as_ptr());
                    SendDlgItemMessageW(dlg, i32::from(ID_EDIT), EM_SETSEL, 0, -1);
                }
            });
            // Focus the first control with WS_TABSTOP: the edit box.
            1
        }
        WM_COMMAND => {
            let id = (wp & 0xffff) as u16;
            if id == ID_OK {
                let mut buf = vec![0u16; 1024];
                // SAFETY: `buf` holds `buf.len()` characters including the terminator.
                let n = unsafe {
                    GetDlgItemTextW(dlg, i32::from(ID_EDIT), buf.as_mut_ptr(), buf.len() as i32)
                };
                let text = String::from_utf16_lossy(&buf[..n as usize]);
                PROMPT.with(|p| p.borrow_mut().1 = Some(text));
                // SAFETY: ends the modal dialog this procedure belongs to.
                unsafe { EndDialog(dlg, 1) };
                1
            } else if id == ID_CANCEL {
                // SAFETY: as above.
                unsafe { EndDialog(dlg, 0) };
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}

/// Asks for a line of text (a preset name). Sends it, or nothing when cancelled.
pub fn ask_text(title: &str, label: &str, default: &str, done: Sender<Vec<String>>) -> bool {
    let template = prompt_template(title, label);
    let default = wide(default);
    let work = move || {
        PROMPT.with(|p| *p.borrow_mut() = (default, None));
        // SAFETY: the template is a complete, DWORD-aligned DLGTEMPLATE that lives across the
        // modal call; `prompt_proc` only touches this thread's PROMPT state.
        let r = unsafe {
            DialogBoxIndirectParamW(
                GetModuleHandleW(std::ptr::null()),
                template.as_ptr().cast::<DLGTEMPLATE>(),
                std::ptr::null_mut(),
                Some(prompt_proc),
                0,
            )
        };
        if r == -1 {
            log::warn!("text prompt could not be shown");
        }
        let text = PROMPT.with(|p| p.borrow_mut().1.take());
        text.map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
            .into_iter()
            .collect()
    };
    spawn("text-dialog", work, done)
}

/// Shows a message box. `text` runs on the dialog's thread first, so slow work (probing a
/// file on a sleeping disk) does not stall the UI.
pub fn show_message(title: &str, text: impl FnOnce() -> String + Send + 'static) -> bool {
    let title = wide(title);
    let (tx, _rx) = std::sync::mpsc::channel();
    spawn(
        "message-dialog",
        move || {
            let text = wide(&text());
            // SAFETY: both strings are NUL-terminated and outlive the modal call.
            unsafe {
                MessageBoxW(
                    std::ptr::null_mut(),
                    text.as_ptr(),
                    title.as_ptr(),
                    MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND,
                )
            };
        },
        tx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateDialogIndirectParamW, DestroyWindow, GetDlgItem,
    };

    #[test]
    fn the_prompt_template_makes_a_dialog() {
        let template = prompt_template("Save preset", "Name:");
        PROMPT.with(|p| *p.borrow_mut() = (wide("Rock"), None));
        // SAFETY: test code; the template lives across the call and the dialog is destroyed.
        unsafe {
            let dlg = CreateDialogIndirectParamW(
                GetModuleHandleW(std::ptr::null()),
                template.as_ptr().cast::<DLGTEMPLATE>(),
                std::ptr::null_mut(),
                Some(prompt_proc),
                0,
            );
            assert!(!dlg.is_null(), "the system accepted the template");
            for id in [ID_EDIT, ID_OK, ID_CANCEL] {
                assert!(!GetDlgItem(dlg, i32::from(id)).is_null(), "control {id}");
            }
            let mut buf = [0u16; 16];
            let n = GetDlgItemTextW(dlg, i32::from(ID_EDIT), buf.as_mut_ptr(), 16);
            assert_eq!(String::from_utf16_lossy(&buf[..n as usize]), "Rock");
            DestroyWindow(dlg);
        }
    }
}
