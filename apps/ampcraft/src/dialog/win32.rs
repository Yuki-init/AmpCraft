//! Dialogs from the Windows common dialogs (file open and save), the shell's folder picker,
//! `MessageBoxW`, and a small dialog built from an in-memory template for text prompts.
//! Each runs modally on its own thread, with COM initialized there as the shell requires.
//! While one is open the player's windows ignore input, and it opens centred over them.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::Sender;

use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect,
};
use windows_sys::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
    CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_ALLOWMULTISELECT, OFN_EXPLORER,
    OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST,
    OPENFILENAMEW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled};
use windows_sys::Win32::UI::Shell::{
    BFFM_INITIALIZED, BFFM_SETSELECTIONW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW,
    FileOpenDialog, SHBrowseForFolderW, SHCreateItemFromParsingName, SHGetPathFromIDListW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DLGTEMPLATE, DialogBoxIndirectParamW, EndDialog, EnumThreadWindows,
    GetDlgItemTextW, GetWindowRect, GetWindowThreadProcessId, HCBT_ACTIVATE, HHOOK,
    IsWindowVisible, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MessageBoxW, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_NOZORDER, SendDlgItemMessageW, SendMessageW, SetDlgItemTextW, SetWindowPos,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_CBT, WM_COMMAND, WM_INITDIALOG,
};
use windows_sys::core::{GUID, HRESULT};

use super::win32_data::{
    Area, ID_CANCEL, ID_EDIT, ID_OK, centered, chosen_files, filter, prompt_template, union, wide,
};
use crate::files::AUDIO_EXTENSIONS;

/// `EM_SETSEL` (CommCtrl.h): select a range of an edit control's text.
const EM_SETSEL: u32 = 0x00B1;
/// Room for a long multiple selection.
const FILE_BUFFER: usize = 64 * 1024;
const MAX_PATH: usize = 260;

/// The player's main window, which owns every dialog: they open centred over it and keep
/// it from taking input while open, as in the classic player. 0 until set.
static OWNER: AtomicIsize = AtomicIsize::new(0);

/// Makes `hwnd` (the main window) the owner of dialogs opened from now on.
pub fn set_owner(hwnd: HWND) {
    OWNER.store(hwnd as isize, Ordering::Relaxed);
}

fn owner() -> HWND {
    OWNER.load(Ordering::Relaxed) as HWND
}

/// The player's windows held disabled while a dialog is open. Windows disables only a
/// dialog's owner, the main window, so the equalizer, playlist and other panels are disabled
/// here too: otherwise a click on one raises it over the dialog and acts on it.
struct Blocked {
    /// Windows this guard disabled (as integers so the guard can move to the dialog's
    /// thread); enabled again on drop.
    disabled: Vec<isize>,
    /// Where the player's visible windows are, to centre the dialog over.
    area: Option<Area>,
}

impl Blocked {
    /// Disables every visible, enabled window of `owner`'s thread except `owner` itself,
    /// which the dialog disables and, when it closes, enables and activates.
    fn player(owner: HWND) -> Blocked {
        let mut windows: Vec<HWND> = Vec::new();
        if !owner.is_null() {
            extern "system" fn collect(hwnd: HWND, data: LPARAM) -> i32 {
                // SAFETY: `data` is the `windows` vector below, alive for the enumeration.
                let windows = unsafe { &mut *(data as *mut Vec<HWND>) };
                windows.push(hwnd);
                1
            }
            // SAFETY: reads the thread that made `owner`, then lists its top-level windows
            // into `windows` through `collect`.
            unsafe {
                let thread = GetWindowThreadProcessId(owner, std::ptr::null_mut());
                EnumThreadWindows(thread, Some(collect), &mut windows as *mut _ as LPARAM);
            }
        }
        // SAFETY: plain queries on windows that exist (just enumerated).
        windows.retain(|&w| unsafe { IsWindowVisible(w) } != 0);
        let area = union(windows.iter().filter_map(|&w| window_area(w)));
        let disabled = windows
            .into_iter()
            // SAFETY: as above.
            .filter(|&w| w != owner && unsafe { IsWindowEnabled(w) } != 0)
            .map(|w| {
                // SAFETY: disables one of the player's windows; undone on drop.
                unsafe { EnableWindow(w, 0) };
                w as isize
            })
            .collect();
        Blocked { disabled, area }
    }
}

impl Drop for Blocked {
    fn drop(&mut self) {
        for &w in &self.disabled {
            // SAFETY: enables a window this guard disabled; harmless if it is gone by now.
            unsafe { EnableWindow(w as HWND, 1) };
        }
    }
}

fn window_area(hwnd: HWND) -> Option<Area> {
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: `r` is a valid out-pointer.
    (unsafe { GetWindowRect(hwnd, &mut r) } != 0).then_some((r.left, r.top, r.right, r.bottom))
}

thread_local! {
    /// The area the next dialog on this thread is centred over; taken by the first one.
    static CENTRE_OVER: Cell<Option<Area>> = const { Cell::new(None) };
    static CENTRE_HOOK: Cell<HHOOK> = const { Cell::new(std::ptr::null_mut()) };
}

/// Moves the first window activated on this thread (the dialog, whichever API made it)
/// over the player, kept inside the work area of the monitor nearest the player.
extern "system" fn centre_hook(code: i32, wp: WPARAM, lp: LPARAM) -> isize {
    if code == HCBT_ACTIVATE as i32
        && let Some(over) = CENTRE_OVER.with(Cell::take)
    {
        centre(wp as HWND, over);
    }
    // SAFETY: passes the event on, as every hook procedure must.
    unsafe { CallNextHookEx(CENTRE_HOOK.with(Cell::get), code, wp, lp) }
}

fn centre(dialog: HWND, over: Area) {
    let Some(own) = window_area(dialog) else {
        return;
    };
    let rect = RECT {
        left: over.0,
        top: over.1,
        right: over.2,
        bottom: over.3,
    };
    // SAFETY: MONITORINFO is plain data; cbSize is set before the call.
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<MONITORINFO>() as u32;
    // SAFETY: a valid rectangle and out-pointer; NEAREST always yields a monitor.
    if unsafe { GetMonitorInfoW(MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST), &mut info) } == 0
    {
        return;
    }
    let w = &info.rcWork;
    let (x, y) = centered(
        over,
        own.2 - own.0,
        own.3 - own.1,
        (w.left, w.top, w.right, w.bottom),
    );
    // SAFETY: moves the dialog window being activated on this thread.
    unsafe {
        SetWindowPos(
            dialog,
            std::ptr::null_mut(),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
}

/// Centres the next dialog on this thread over `area` until dropped.
struct Centring(HHOOK);

impl Centring {
    fn over(area: Option<Area>) -> Centring {
        let Some(area) = area else {
            return Centring(std::ptr::null_mut());
        };
        CENTRE_OVER.with(|c| c.set(Some(area)));
        // SAFETY: a hook for this thread only, removed on drop; the procedure only touches
        // this thread's state.
        let hook = unsafe {
            SetWindowsHookExW(
                WH_CBT,
                Some(centre_hook),
                std::ptr::null_mut(),
                GetCurrentThreadId(),
            )
        };
        CENTRE_HOOK.with(|h| h.set(hook));
        Centring(hook)
    }
}

impl Drop for Centring {
    fn drop(&mut self) {
        CENTRE_OVER.with(|c| c.set(None));
        if !self.0.is_null() {
            // SAFETY: removes the hook `over` installed.
            unsafe { UnhookWindowsHookEx(self.0) };
        }
    }
}

/// Runs `work` on a named thread with COM initialized, sending its result on `done`. The
/// player's windows are disabled until it returns, and the dialog opens centred over them.
fn spawn<T: Send + 'static>(
    name: &str,
    work: impl FnOnce() -> T + Send + 'static,
    done: Sender<T>,
) -> bool {
    // On the player's thread, so its windows are disabled before it handles another click.
    let blocked = Blocked::player(owner());
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
            let centring = Centring::over(blocked.area);
            let result = work();
            drop(centring);
            if hr >= 0 {
                // SAFETY: pairs the successful CoInitializeEx on this thread.
                unsafe { CoUninitialize() };
            }
            drop(blocked);
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
    ofn.hwndOwner = owner();
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

// The modern folder picker is the common item dialog (IFileOpenDialog) in folder mode.
// windows-sys has no COM method wrappers, so the vtable entries used are declared here, in
// their order in shobjidl.h; slots that are not called are plain pointers.

const IID_IFILEOPENDIALOG: GUID = GUID::from_u128(0xd57c7288_d4ad_4768_be02_9d969532d960);
const IID_ISHELLITEM: GUID = GUID::from_u128(0x43826d1e_e718_42ee_bc55_a1e261c37bfe);
const FOS_PICKFOLDERS: u32 = 0x20;
const FOS_FORCEFILESYSTEM: u32 = 0x40;
const SIGDN_FILESYSPATH: u32 = 0x8005_8000;

type Com = *mut core::ffi::c_void;

#[repr(C)]
struct FileDialogVtbl {
    query_interface: usize,
    add_ref: usize,
    release: unsafe extern "system" fn(Com) -> u32,
    // IModalWindow
    show: unsafe extern "system" fn(Com, HWND) -> HRESULT,
    // IFileDialog
    set_file_types: usize,
    set_file_type_index: usize,
    get_file_type_index: usize,
    advise: usize,
    unadvise: usize,
    set_options: unsafe extern "system" fn(Com, u32) -> HRESULT,
    get_options: unsafe extern "system" fn(Com, *mut u32) -> HRESULT,
    set_default_folder: usize,
    set_folder: unsafe extern "system" fn(Com, Com) -> HRESULT,
    get_folder: usize,
    get_current_selection: usize,
    set_file_name: usize,
    get_file_name: usize,
    set_title: unsafe extern "system" fn(Com, *const u16) -> HRESULT,
    set_ok_button_label: usize,
    set_file_name_label: usize,
    get_result: unsafe extern "system" fn(Com, *mut Com) -> HRESULT,
}

#[repr(C)]
struct ShellItemVtbl {
    query_interface: usize,
    add_ref: usize,
    release: unsafe extern "system" fn(Com) -> u32,
    bind_to_handler: usize,
    get_parent: usize,
    get_display_name: unsafe extern "system" fn(Com, u32, *mut *mut u16) -> HRESULT,
}

/// A COM object's vtable: the object's first field points at it.
///
/// # Safety
/// `obj` must be a live object whose interface is laid out as `V`.
unsafe fn vtbl<'a, V>(obj: Com) -> &'a V {
    // SAFETY: guaranteed by the caller.
    unsafe { &**(obj as *const *const V) }
}

/// A COM object released on drop.
struct Owned(Com);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: an Owned always holds a live object; Release is the third slot of every
        // interface (IUnknown's), which ShellItemVtbl declares.
        unsafe { (vtbl::<ShellItemVtbl>(self.0).release)(self.0) };
    }
}

/// Creates the common item dialog in folder mode with `title` and, if it exists, `start`.
fn new_folder_dialog(title: &[u16], start: Option<&[u16]>) -> Option<Owned> {
    let mut obj: Com = std::ptr::null_mut();
    // SAFETY: standard COM creation with valid GUIDs and an out-pointer; COM is initialized
    // on this thread by `spawn` (or the test).
    let hr = unsafe {
        CoCreateInstance(
            &FileOpenDialog,
            std::ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_IFILEOPENDIALOG,
            &mut obj,
        )
    };
    if hr < 0 || obj.is_null() {
        return None;
    }
    let dialog = Owned(obj);
    // SAFETY: `dialog` is a live IFileOpenDialog.
    let v = unsafe { vtbl::<FileDialogVtbl>(dialog.0) };
    let mut options = 0;
    // SAFETY: a live object, a valid out-pointer, and a NUL-terminated title.
    unsafe {
        if (v.get_options)(dialog.0, &mut options) < 0
            || (v.set_options)(dialog.0, options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM) < 0
        {
            return None;
        }
        (v.set_title)(dialog.0, title.as_ptr());
    }
    if let Some(start) = start {
        let mut item: Com = std::ptr::null_mut();
        // SAFETY: `start` is NUL-terminated; SetFolder takes its own reference to the item.
        let hr = unsafe {
            SHCreateItemFromParsingName(
                start.as_ptr(),
                std::ptr::null_mut(),
                &IID_ISHELLITEM,
                &mut item,
            )
        };
        if hr >= 0 && !item.is_null() {
            let item = Owned(item);
            // SAFETY: both objects are live.
            unsafe { (v.set_folder)(dialog.0, item.0) };
        }
    }
    Some(dialog)
}

/// Shows the modern folder picker. None when it is not available (the older one is used
/// then); otherwise the chosen folder, or nothing when cancelled.
fn modern_folder_dialog(title: &[u16], start: Option<&[u16]>) -> Option<Vec<PathBuf>> {
    let dialog = new_folder_dialog(title, start)?;
    // SAFETY: `dialog` is a live IFileOpenDialog.
    let v = unsafe { vtbl::<FileDialogVtbl>(dialog.0) };
    // SAFETY: a modal call on this thread; the owner is null or the player's main window.
    if unsafe { (v.show)(dialog.0, owner()) } < 0 {
        // Cancelled or closed: an answer, not a reason to fall back.
        return Some(Vec::new());
    }
    let mut item: Com = std::ptr::null_mut();
    // SAFETY: a live object and a valid out-pointer.
    if unsafe { (v.get_result)(dialog.0, &mut item) } < 0 || item.is_null() {
        return Some(Vec::new());
    }
    let item = Owned(item);
    // SAFETY: `item` is a live IShellItem.
    let iv = unsafe { vtbl::<ShellItemVtbl>(item.0) };
    let mut name: *mut u16 = std::ptr::null_mut();
    // SAFETY: a live object and a valid out-pointer; the shell allocates `name`.
    let hr = unsafe { (iv.get_display_name)(item.0, SIGDN_FILESYSPATH, &mut name) };
    if hr < 0 || name.is_null() {
        return Some(Vec::new());
    }
    // SAFETY: `name` is a NUL-terminated UTF-16 string from the shell; read to the end.
    let path = unsafe {
        let mut len = 0;
        while *name.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(name, len))
    };
    // SAFETY: frees the string the shell allocated for us.
    unsafe { CoTaskMemFree(name as *const core::ffi::c_void) };
    Some(vec![PathBuf::from(path)])
}

/// Asks the user for one folder, with `title` as the caption. Sends the choice (or nothing)
/// on `done`. Uses the modern folder picker, or the older tree dialog where it is missing.
pub fn pick_folder(title: &str, start: Option<PathBuf>, done: Sender<Vec<PathBuf>>) -> bool {
    let title = wide(title);
    let work = move || {
        let start = start.map(|p| wide(&p.to_string_lossy()));
        if let Some(chosen) = modern_folder_dialog(&title, start.as_deref()) {
            return chosen;
        }
        log::info!("modern folder picker unavailable; using the older one");
        let mut name = vec![0u16; MAX_PATH];
        let info = BROWSEINFOW {
            hwndOwner: owner(),
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
                owner(),
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
                    owner(),
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
        CreateDialogIndirectParamW, CreateWindowExW, DestroyWindow, GetDlgItem, SW_SHOW,
        ShowWindow, WS_POPUP, WS_VISIBLE,
    };

    /// Held by tests that make windows. Activating a window on one test thread while
    /// another thread's window is active waits on the other thread, which a test thread does
    /// not answer while it runs, so these take turns.
    static WINDOWS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn windows_lock() -> std::sync::MutexGuard<'static, ()> {
        WINDOWS.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn popup(x: i32, y: i32, w: i32, h: i32) -> HWND {
        let class = wide("STATIC");
        // SAFETY: test code; a plain popup of a system class, destroyed by the test.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                WS_POPUP | WS_VISIBLE,
                x,
                y,
                w,
                h,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null());
        hwnd
    }

    fn enabled(hwnd: HWND) -> bool {
        // SAFETY: test code; a plain query.
        unsafe { IsWindowEnabled(hwnd) != 0 }
    }

    #[test]
    fn the_players_other_windows_ignore_input_while_a_dialog_is_open() {
        let _windows = windows_lock();
        // Regression: only the main window (the owner) was disabled, so a click on the
        // playlist raised it over the dialog and opened its ADD menu.
        let main = popup(250, 0, 550, 232);
        let eq = popup(250, 232, 550, 232);
        let playlist = popup(250, 464, 550, 464);
        let off = popup(900, 0, 100, 100);
        // SAFETY: test code.
        unsafe { EnableWindow(off, 0) };
        let blocked = Blocked::player(main);
        assert!(enabled(main), "the dialog disables its owner itself");
        assert!(!enabled(eq) && !enabled(playlist));
        assert_eq!(blocked.area, Some((250, 0, 1000, 928)));
        drop(blocked);
        assert!(enabled(main) && enabled(eq) && enabled(playlist));
        assert!(!enabled(off), "a window disabled before stays disabled");
        // SAFETY: test code.
        unsafe {
            for w in [main, eq, playlist, off] {
                DestroyWindow(w);
            }
        }
    }

    #[test]
    fn dialogs_open_centred_over_the_player() {
        let _windows = windows_lock();
        let template = prompt_template("Save preset", "Name:");
        PROMPT.with(|p| *p.borrow_mut() = (wide("Rock"), None));
        let centring = Centring::over(Some((100, 100, 700, 600)));
        // SAFETY: test code; the template lives across the call and the dialog is destroyed.
        let dlg = unsafe {
            CreateDialogIndirectParamW(
                GetModuleHandleW(std::ptr::null()),
                template.as_ptr().cast::<DLGTEMPLATE>(),
                std::ptr::null_mut(),
                Some(prompt_proc),
                0,
            )
        };
        assert!(!dlg.is_null());
        // Showing it activates it, as DialogBox does for a modal one.
        // SAFETY: test code; `dlg` is live.
        unsafe { ShowWindow(dlg, SW_SHOW) };
        drop(centring);
        let (l, t, r, b) = window_area(dlg).unwrap();
        // Centred on (400, 350), give or take a pixel of rounding.
        assert!(((l + r) / 2 - 400).abs() <= 1, "x {l}..{r}");
        assert!(((t + b) / 2 - 350).abs() <= 1, "y {t}..{b}");
        // SAFETY: test code.
        unsafe { DestroyWindow(dlg) };
    }

    #[test]
    fn the_modern_folder_picker_is_set_up_through_its_vtable() {
        // SAFETY: test code; COM is initialized for this thread and uninitialized at the end.
        unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as _) };
        let start = wide(&std::env::temp_dir().to_string_lossy());
        let dialog = new_folder_dialog(&wide("Add folder to playlist"), Some(&start))
            .expect("the common item dialog is available");
        // SAFETY: `dialog` is a live IFileOpenDialog.
        let v = unsafe { vtbl::<FileDialogVtbl>(dialog.0) };
        let mut options = 0;
        // Reading the options back through another slot shows the layout lines up.
        // SAFETY: a live object and a valid out-pointer.
        assert!(unsafe { (v.get_options)(dialog.0, &mut options) } >= 0);
        assert_eq!(
            options & (FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM),
            FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM
        );
        drop(dialog);
        // SAFETY: pairs the CoInitializeEx above.
        unsafe { CoUninitialize() };
    }

    #[test]
    fn the_prompt_template_makes_a_dialog() {
        let _windows = windows_lock();
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
