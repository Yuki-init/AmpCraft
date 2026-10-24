//! The pure parts of the Windows media controls (`smtc.rs`), kept apart so they are tested on
//! every platform. The numbers are the values of the Windows.Media enums.

use super::{Command, SEEK_STEP_US, State, Status};

/// The command for a pressed `SystemMediaTransportControlsButton`, if the player has one.
pub fn button_command(button: i32) -> Option<Command> {
    match button {
        0 => Some(Command::Play),
        1 => Some(Command::Pause),
        2 => Some(Command::Stop),
        4 => Some(Command::Seek(SEEK_STEP_US)),
        5 => Some(Command::Seek(-SEEK_STEP_US)),
        6 => Some(Command::Next),
        7 => Some(Command::Previous),
        // Record, ChannelUp, ChannelDown and anything newer.
        _ => None,
    }
}

/// The command for a request to move to `ticks` in the current track. `track` is the track
/// last published, so a request that arrives just after a track change seeks nothing.
pub fn position_command(track: Option<u64>, ticks: i64) -> Option<Command> {
    let track = track?;
    (ticks >= 0).then_some(Command::SetPosition(track, ticks / 10))
}

/// The `MediaPlaybackStatus` for a status.
pub fn playback_status(status: Status) -> i32 {
    match status {
        Status::Stopped => 2,
        Status::Playing => 3,
        Status::Paused => 4,
    }
}

/// The `MediaPlaybackAutoRepeatMode` for the repeat toggle: List or None. The player has no
/// single-track repeat.
pub fn repeat_mode(repeat: bool) -> i32 {
    if repeat { 2 } else { 0 }
}

/// Whether a requested `MediaPlaybackAutoRepeatMode` turns repeat on (Track or List).
pub fn repeat_requested(mode: i32) -> bool {
    mode != 0
}

/// Windows `TimeSpan` ticks (100 ns) for microseconds.
pub fn ticks(us: i64) -> i64 {
    us.saturating_mul(10)
}

/// Which parts of the controls need sending to Windows.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Changes {
    pub status: bool,
    /// Title, artist and album.
    pub display: bool,
    /// Length and position.
    pub timeline: bool,
    pub shuffle: bool,
    pub repeat: bool,
}

/// What changed from `old` (None before the first update) to `new`. Windows does not move the
/// position on by itself while playing, so `timeline_stale` asks for a refresh.
pub fn changes(old: Option<&State>, new: &State, seeked: bool, timeline_stale: bool) -> Changes {
    let Some(old) = old else {
        return Changes {
            status: true,
            display: true,
            timeline: true,
            shuffle: true,
            repeat: true,
        };
    };
    let display = (&old.track_id, &old.title, &old.artist, &old.album)
        != (&new.track_id, &new.title, &new.artist, &new.album);
    Changes {
        status: old.status != new.status,
        display,
        timeline: display
            || seeked
            || old.status != new.status
            || old.length_us != new.length_us
            || (timeline_stale && new.status == Status::Playing),
        shuffle: old.shuffle != new.shuffle,
        repeat: old.repeat != new.repeat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_map_to_commands() {
        assert_eq!(button_command(0), Some(Command::Play));
        assert_eq!(button_command(1), Some(Command::Pause));
        assert_eq!(button_command(2), Some(Command::Stop));
        assert_eq!(button_command(4), Some(Command::Seek(5_000_000)));
        assert_eq!(button_command(5), Some(Command::Seek(-5_000_000)));
        assert_eq!(button_command(6), Some(Command::Next));
        assert_eq!(button_command(7), Some(Command::Previous));
        for ignored in [3, 8, 9, 10, -1, i32::MAX] {
            assert_eq!(button_command(ignored), None);
        }
    }

    #[test]
    fn position_requests_need_a_track_and_a_sane_position() {
        assert_eq!(
            position_command(Some(4), 420_000_000),
            Some(Command::SetPosition(4, 42_000_000))
        );
        assert_eq!(position_command(None, 10), None);
        assert_eq!(position_command(Some(4), -1), None);
    }

    #[test]
    fn enum_values_match_windows() {
        assert_eq!(playback_status(Status::Stopped), 2);
        assert_eq!(playback_status(Status::Playing), 3);
        assert_eq!(playback_status(Status::Paused), 4);
        assert_eq!(repeat_mode(true), 2);
        assert_eq!(repeat_mode(false), 0);
        assert!(repeat_requested(1) && repeat_requested(2) && !repeat_requested(0));
    }

    #[test]
    fn ticks_saturate() {
        assert_eq!(ticks(1), 10);
        assert_eq!(ticks(i64::MAX), i64::MAX);
        assert_eq!(ticks(i64::MIN), i64::MIN);
    }

    #[test]
    fn only_what_changed_is_sent() {
        let playing = State {
            status: Status::Playing,
            track_id: Some(1),
            title: Some("A".into()),
            length_us: Some(60_000_000),
            position_us: 1_000_000,
            ..State::default()
        };
        let first = changes(None, &playing, false, false);
        assert!(first.status && first.display && first.timeline && first.shuffle && first.repeat);

        let later = State {
            position_us: 2_000_000,
            ..playing.clone()
        };
        assert_eq!(
            changes(Some(&playing), &later, false, false),
            Changes::default()
        );
        assert_eq!(
            changes(Some(&playing), &later, false, true),
            Changes {
                timeline: true,
                ..Changes::default()
            }
        );
        assert!(changes(Some(&playing), &later, true, false).timeline);

        let paused = State {
            status: Status::Paused,
            ..playing.clone()
        };
        let c = changes(Some(&playing), &paused, false, false);
        assert!(c.status && c.timeline && !c.display);
        assert!(
            !changes(Some(&paused), &paused, false, true).timeline,
            "a paused position does not go stale"
        );

        let next = State {
            track_id: Some(2),
            title: Some("B".into()),
            ..playing.clone()
        };
        let c = changes(Some(&playing), &next, false, false);
        assert!(c.display && c.timeline && !c.status);

        let toggles = State {
            shuffle: true,
            repeat: true,
            ..playing.clone()
        };
        let c = changes(Some(&playing), &toggles, false, false);
        assert!(c.shuffle && c.repeat && !c.display && !c.timeline);
    }
}
