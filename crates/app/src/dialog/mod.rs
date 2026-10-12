//! File pickers, a text prompt and message boxes, run on a thread so the UI keeps drawing
//! while a dialog is open. Results come back on a channel; a dialog that cannot be shown
//! returns false.
//!
//! Windows uses the system's common dialogs; elsewhere the desktop's dialog helper (kdialog
//! or zenity) is run. Both offer the same functions.

#[cfg(not(windows))]
mod helper;
#[cfg(not(windows))]
pub use helper::*;

#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub use win32::*;

#[cfg(any(windows, test))]
mod win32_data;
