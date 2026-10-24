//! Windows media controls: the System Media Transport Controls (SMTC) for the player's main
//! window. They show the track in the media flyout and the lock screen, and Windows sends
//! them the keyboard's media keys.
//!
//! Windows calls the event handlers on its own threads; they only turn the request into a
//! [`Command`] on a channel, and never panic (a panic there would abort the player).

use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Media::{
    AutoRepeatModeChangeRequestedEventArgs, MediaPlaybackAutoRepeatMode, MediaPlaybackStatus,
    MediaPlaybackType, PlaybackPositionChangeRequestedEventArgs,
    ShuffleEnabledChangeRequestedEventArgs, SystemMediaTransportControls,
    SystemMediaTransportControlsButtonPressedEventArgs, SystemMediaTransportControlsDisplayUpdater,
    SystemMediaTransportControlsTimelineProperties,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;
use windows::core::{HSTRING, Ref};

use super::smtc_data::{
    Changes, button_command, changes, playback_status, position_command, repeat_mode,
    repeat_requested, ticks,
};
use super::{Command, MediaControlsError, State, Status};

/// The current track id for the event handlers; this value means none.
const NO_TRACK: u64 = u64::MAX;
/// How often the position is resent while playing; Windows does not advance it itself.
const TIMELINE_REFRESH: Duration = Duration::from_secs(5);

impl From<windows::core::Error> for MediaControlsError {
    fn from(e: windows::core::Error) -> Self {
        MediaControlsError::Service(format!("media controls: {e}"))
    }
}

/// Event registrations to remove when the controls are dropped.
#[derive(Default)]
struct Tokens {
    button: Option<i64>,
    position: Option<i64>,
    shuffle: Option<i64>,
    repeat: Option<i64>,
}

pub struct Smtc {
    controls: SystemMediaTransportControls,
    display: SystemMediaTransportControlsDisplayUpdater,
    /// The length and position for the flyout's progress bar. None where Windows lacks the
    /// class (Wine); the controls work without it.
    timeline: Option<SystemMediaTransportControlsTimelineProperties>,
    commands: Receiver<Command>,
    track: Arc<AtomicU64>,
    tokens: Tokens,
    last: Option<State>,
    timeline_at: Instant,
    /// An update failed and was logged; later failures are not.
    failed: bool,
}

impl std::fmt::Debug for Smtc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Smtc")
    }
}

impl Smtc {
    /// Gets the controls for `hwnd`, the player's main window, and listens for their buttons.
    pub fn start(hwnd: *mut c_void) -> Result<Self, MediaControlsError> {
        if hwnd.is_null() {
            return Err(MediaControlsError::Service("no main window".into()));
        }
        let step = |what: &str| {
            let what = what.to_owned();
            move |e: windows::core::Error| MediaControlsError::Service(format!("{what}: {e}"))
        };
        let interop = windows::core::factory::<
            SystemMediaTransportControls,
            ISystemMediaTransportControlsInterop,
        >()
        .map_err(step("SMTC factory"))?;
        // SAFETY: `hwnd` is the player's main window, which lives until the player exits.
        // GetForWindow only looks the window up and returns a reference-counted object that
        // the `windows` crate owns and releases.
        let controls: SystemMediaTransportControls =
            unsafe { interop.GetForWindow(HWND(hwnd)) }.map_err(step("SMTC for window"))?;
        let display = controls.DisplayUpdater().map_err(step("SMTC display"))?;
        let timeline = match SystemMediaTransportControlsTimelineProperties::new() {
            Ok(t) => Some(t),
            Err(e) => {
                log::info!("media controls: no progress bar ({e})");
                None
            }
        };
        let (tx, commands) = mpsc::channel();
        let mut smtc = Self {
            controls,
            display,
            timeline,
            commands,
            track: Arc::new(AtomicU64::new(NO_TRACK)),
            tokens: Tokens::default(),
            last: None,
            timeline_at: Instant::now(),
            failed: false,
        };
        smtc.listen(tx).map_err(step("SMTC events"))?;
        Ok(smtc)
    }

    /// Turns the controls on and registers the event handlers. Only the buttons are
    /// essential; the rest is best effort, since some Windows builds (and Wine) lack parts.
    fn listen(&mut self, tx: Sender<Command>) -> windows::core::Result<()> {
        let c = &self.controls;
        c.SetIsEnabled(true)?;
        let send = tx.clone();
        self.tokens.button = Some(c.ButtonPressed(&TypedEventHandler::new(
            move |_: Ref<SystemMediaTransportControls>,
                  args: Ref<SystemMediaTransportControlsButtonPressedEventArgs>| {
                if let Some(command) = button_command(args.ok()?.Button()?.0) {
                    let _ = send.send(command);
                }
                Ok(())
            },
        ))?);

        let optional = [
            ("play", c.SetIsPlayEnabled(true)),
            ("pause", c.SetIsPauseEnabled(true)),
            ("stop", c.SetIsStopEnabled(true)),
            ("next", c.SetIsNextEnabled(true)),
            ("previous", c.SetIsPreviousEnabled(true)),
            ("fast forward", c.SetIsFastForwardEnabled(true)),
            ("rewind", c.SetIsRewindEnabled(true)),
            ("music type", self.display.SetType(MediaPlaybackType::Music)),
        ];
        for (what, result) in optional {
            if let Err(e) = result {
                log::debug!("media controls: no {what} ({e})");
            }
        }

        let send = tx.clone();
        let track = Arc::clone(&self.track);
        self.tokens.position = registered(
            "seeking",
            c.PlaybackPositionChangeRequested(&TypedEventHandler::new(
                move |_: Ref<SystemMediaTransportControls>,
                      args: Ref<PlaybackPositionChangeRequestedEventArgs>| {
                    let to = args.ok()?.RequestedPlaybackPosition()?.Duration;
                    let current = track.load(Ordering::Relaxed);
                    let current = (current != NO_TRACK).then_some(current);
                    if let Some(command) = position_command(current, to) {
                        let _ = send.send(command);
                    }
                    Ok(())
                },
            )),
        );

        let send = tx.clone();
        self.tokens.shuffle = registered(
            "shuffle",
            c.ShuffleEnabledChangeRequested(&TypedEventHandler::new(
                move |_: Ref<SystemMediaTransportControls>,
                      args: Ref<ShuffleEnabledChangeRequestedEventArgs>| {
                    let on = args.ok()?.RequestedShuffleEnabled()?;
                    let _ = send.send(Command::SetShuffle(on));
                    Ok(())
                },
            )),
        );

        self.tokens.repeat = registered(
            "repeat",
            c.AutoRepeatModeChangeRequested(&TypedEventHandler::new(
                move |_: Ref<SystemMediaTransportControls>,
                      args: Ref<AutoRepeatModeChangeRequestedEventArgs>| {
                    let mode = args.ok()?.RequestedAutoRepeatMode()?.0;
                    let _ = tx.send(Command::SetLoop(repeat_requested(mode)));
                    Ok(())
                },
            )),
        );
        Ok(())
    }

    /// Commands received since the last call.
    pub fn commands(&self) -> Vec<Command> {
        self.commands.try_iter().collect()
    }

    /// Sends what changed to Windows. Failures are logged once and otherwise ignored: the
    /// flyout is a nicety, the player keeps going.
    pub fn update(&mut self, state: State, seeked: bool) {
        self.track
            .store(state.track_id.unwrap_or(NO_TRACK), Ordering::Relaxed);
        let stale = self.timeline_at.elapsed() >= TIMELINE_REFRESH;
        let todo = changes(self.last.as_ref(), &state, seeked, stale);
        if let Err(e) = self.apply(&state, todo)
            && !self.failed
        {
            log::info!("media controls update failed: {e}");
            self.failed = true;
        }
        self.last = Some(state);
    }

    fn apply(&mut self, s: &State, todo: Changes) -> windows::core::Result<()> {
        if todo.status {
            self.controls
                .SetPlaybackStatus(MediaPlaybackStatus(playback_status(s.status)))?;
        }
        if todo.display {
            if s.track_id.is_none() {
                self.display.ClearAll()?;
                self.display.SetType(MediaPlaybackType::Music)?;
            } else {
                let music = self.display.MusicProperties()?;
                music.SetTitle(&HSTRING::from(s.title.as_deref().unwrap_or_default()))?;
                music.SetArtist(&HSTRING::from(s.artist.as_deref().unwrap_or_default()))?;
                music.SetAlbumTitle(&HSTRING::from(s.album.as_deref().unwrap_or_default()))?;
            }
            self.display.Update()?;
        }
        if todo.shuffle {
            self.controls.SetShuffleEnabled(s.shuffle)?;
        }
        if todo.repeat {
            self.controls
                .SetAutoRepeatMode(MediaPlaybackAutoRepeatMode(repeat_mode(s.repeat)))?;
        }
        if todo.timeline
            && let Some(timeline) = &self.timeline
        {
            let length = s.length_us.unwrap_or(0).max(0);
            let start = TimeSpan { Duration: 0 };
            let end = TimeSpan {
                Duration: ticks(length),
            };
            let position = if s.status == Status::Stopped {
                0
            } else {
                s.position_us.clamp(0, length)
            };
            timeline.SetStartTime(start)?;
            timeline.SetMinSeekTime(start)?;
            timeline.SetEndTime(end)?;
            timeline.SetMaxSeekTime(end)?;
            timeline.SetPosition(TimeSpan {
                Duration: ticks(position),
            })?;
            self.controls.UpdateTimelineProperties(timeline)?;
            self.timeline_at = Instant::now();
        }
        Ok(())
    }
}

/// The token of an optional event registration, logging when Windows refused it.
fn registered(what: &str, result: windows::core::Result<i64>) -> Option<i64> {
    result
        .inspect_err(|e| log::debug!("media controls: no {what} requests ({e})"))
        .ok()
}

impl Drop for Smtc {
    fn drop(&mut self) {
        let c = &self.controls;
        let tokens = std::mem::take(&mut self.tokens);
        if let Some(t) = tokens.button {
            let _ = c.RemoveButtonPressed(t);
        }
        if let Some(t) = tokens.position {
            let _ = c.RemovePlaybackPositionChangeRequested(t);
        }
        if let Some(t) = tokens.shuffle {
            let _ = c.RemoveShuffleEnabledChangeRequested(t);
        }
        if let Some(t) = tokens.repeat {
            let _ = c.RemoveAutoRepeatModeChangeRequested(t);
        }
        let _ = c.SetIsEnabled(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, WS_OVERLAPPEDWINDOW,
    };

    /// Wine's ntdll exports `wine_get_version`; Windows' does not.
    fn running_on_wine() -> bool {
        let ntdll = wide("ntdll.dll");
        // SAFETY: NUL-terminated names; ntdll is loaded in every process and never unloaded.
        unsafe {
            let module = GetModuleHandleW(ntdll.as_ptr());
            !module.is_null()
                && GetProcAddress(module, c"wine_get_version".as_ptr().cast()).is_some()
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    #[test]
    fn no_window_is_an_error_not_a_crash() {
        assert!(Smtc::start(std::ptr::null_mut()).is_err());
    }

    #[test]
    fn starts_for_a_window_and_takes_updates() {
        let class = wide("STATIC");
        let title = wide("AmpCraft SMTC test");
        // SAFETY: plain window creation with valid, NUL-terminated strings; the handle is
        // destroyed below on this thread.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                200,
                100,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null());
        let started = Smtc::start(hwnd);
        if running_on_wine() {
            // Wine has the controls but not their button events.
            let e = started.err().map(|e| e.to_string()).unwrap_or_default();
            assert!(e.contains("SMTC events"), "{e}");
            // SAFETY: `hwnd` was created above on this thread and not destroyed yet.
            unsafe { DestroyWindow(hwnd) };
            return;
        }
        let mut smtc = started.unwrap();
        let playing = State {
            status: Status::Playing,
            track_id: Some(3),
            title: Some("ハレ晴レユカイ".into()),
            artist: Some("Artist".into()),
            album: Some("Album".into()),
            length_us: Some(180_000_000),
            position_us: 12_000_000,
            ..State::default()
        };
        smtc.update(playing.clone(), false);
        smtc.update(
            State {
                status: Status::Paused,
                shuffle: true,
                repeat: true,
                ..playing
            },
            true,
        );
        smtc.update(State::default(), false);
        assert!(!smtc.failed, "an update failed (see the log)");
        assert!(smtc.commands().is_empty());
        drop(smtc);
        // SAFETY: `hwnd` was created above on this thread and not destroyed yet.
        unsafe { DestroyWindow(hwnd) };
    }
}
