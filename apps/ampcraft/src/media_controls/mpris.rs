//! MPRIS (`org.mpris.MediaPlayer2`) on the D-Bus session bus, so desktop media keys, panel
//! widgets and tools like `playerctl` can control the player.
//!
//! D-Bus method calls become [`Command`]s on a channel that the UI loop drains. The app
//! pushes its state with [`Mpris::update`]; only properties that changed are signalled.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};

use zbus::blocking::Connection;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use crate::media_controls::{Command, State, Status};

const PATH: &str = "/org/mpris/MediaPlayer2";
const NAME: &str = "org.mpris.MediaPlayer2.ampcraft";

/// The `PlaybackStatus` property value for a status.
fn status_str(status: Status) -> &'static str {
    match status {
        Status::Playing => "Playing",
        Status::Paused => "Paused",
        Status::Stopped => "Stopped",
    }
}

/// Object path naming a track in MPRIS metadata.
pub fn track_path(id: u64) -> String {
    format!("/org/ampcraft/track/{id}")
}

/// Parses a track path back into its id.
pub fn track_id_from_path(path: &str) -> Option<u64> {
    path.strip_prefix("/org/ampcraft/track/")?.parse().ok()
}

/// The `Metadata` dictionary for a state.
pub fn metadata(state: &State) -> HashMap<String, OwnedValue> {
    let mut m = HashMap::new();
    let mut put = |k: &str, v: Value<'_>| {
        if let Ok(owned) = v.try_to_owned() {
            m.insert(k.to_owned(), owned);
        }
    };
    let path = state
        .track_id
        .map(track_path)
        .unwrap_or_else(|| "/org/mpris/MediaPlayer2/TrackList/NoTrack".into());
    if let Ok(p) = ObjectPath::try_from(path) {
        put("mpris:trackid", Value::from(p));
    }
    if let Some(len) = state.length_us {
        put("mpris:length", Value::from(len));
    }
    if let Some(t) = &state.title {
        put("xesam:title", Value::from(t.as_str()));
    }
    if let Some(a) = &state.artist {
        put("xesam:artist", Value::from(vec![a.as_str()]));
    }
    if let Some(a) = &state.album {
        put("xesam:album", Value::from(a.as_str()));
    }
    m
}

struct Root {
    commands: Sender<Command>,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {
        let _ = self.commands.send(Command::Raise);
    }

    fn quit(&self) {
        let _ = self.commands.send(Command::Quit);
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> String {
        "AmpCraft".into()
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        "ampcraft".into()
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec!["file".into()]
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        [
            "audio/mpeg",
            "audio/flac",
            "audio/ogg",
            "audio/opus",
            "audio/mp4",
            "audio/x-wav",
        ]
        .iter()
        .map(|s| (*s).into())
        .collect()
    }
}

struct PlayerIface {
    commands: Sender<Command>,
    state: Arc<Mutex<State>>,
}

impl PlayerIface {
    fn state(&self) -> State {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn send(&self, c: Command) {
        let _ = self.commands.send(c);
    }
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl PlayerIface {
    fn play(&self) {
        self.send(Command::Play);
    }

    fn pause(&self) {
        self.send(Command::Pause);
    }

    fn play_pause(&self) {
        self.send(Command::PlayPause);
    }

    fn stop(&self) {
        self.send(Command::Stop);
    }

    fn next(&self) {
        self.send(Command::Next);
    }

    fn previous(&self) {
        self.send(Command::Previous);
    }

    fn seek(&self, offset: i64) {
        self.send(Command::Seek(offset));
    }

    fn set_position(&self, track: ObjectPath<'_>, position: i64) {
        if let Some(id) = track_id_from_path(track.as_str()) {
            self.send(Command::SetPosition(id, position));
        }
    }

    fn open_uri(&self, _uri: &str) {}

    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> String {
        status_str(self.state().status).into()
    }

    #[zbus(property)]
    fn loop_status(&self) -> String {
        if self.state().repeat {
            "Playlist"
        } else {
            "None"
        }
        .into()
    }

    #[zbus(property)]
    fn set_loop_status(&self, value: String) {
        self.send(Command::SetLoop(value != "None"));
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.state().shuffle
    }

    #[zbus(property)]
    fn set_shuffle(&self, value: bool) {
        self.send(Command::SetShuffle(value));
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        metadata(&self.state())
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.state().volume
    }

    #[zbus(property)]
    fn set_volume(&self, value: f64) {
        self.send(Command::SetVolume(value.clamp(0.0, 1.0)));
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.state().position_us
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

/// A running MPRIS service.
pub struct Mpris {
    conn: Connection,
    state: Arc<Mutex<State>>,
    commands: Receiver<Command>,
}

impl std::fmt::Debug for Mpris {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Mpris")
    }
}

impl Mpris {
    /// Connects to the session bus and publishes the player. If another instance holds the
    /// name, a per-process name is used, as the MPRIS spec allows.
    pub fn start() -> zbus::Result<Self> {
        let (tx, commands) = mpsc::channel();
        let state = Arc::new(Mutex::new(State {
            volume: 1.0,
            ..State::default()
        }));
        let conn = zbus::blocking::connection::Builder::session()?
            .serve_at(
                PATH,
                Root {
                    commands: tx.clone(),
                },
            )?
            .serve_at(
                PATH,
                PlayerIface {
                    commands: tx,
                    state: Arc::clone(&state),
                },
            )?
            .build()?;
        if conn.request_name(NAME).is_err() {
            conn.request_name(format!("{NAME}.instance{}", std::process::id()))?;
        }
        Ok(Self {
            conn,
            state,
            commands,
        })
    }

    /// Commands received since the last call.
    pub fn commands(&self) -> Vec<Command> {
        self.commands.try_iter().collect()
    }

    /// Publishes new state, signalling the properties that changed. `seeked` reports a jump
    /// in position (seek or track restart) so clients resync their progress bars.
    pub fn update(&self, new: State, seeked: bool) {
        let old = {
            let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            std::mem::replace(&mut *s, new.clone())
        };
        let Ok(iface) = self.conn.object_server().interface::<_, PlayerIface>(PATH) else {
            return;
        };
        let emitter = iface.signal_emitter();
        let player = iface.get();
        let result: zbus::Result<()> = zbus::block_on(async {
            if old.status != new.status {
                player.playback_status_changed(emitter).await?;
            }
            if (
                &old.track_id,
                &old.title,
                &old.artist,
                &old.album,
                old.length_us,
            ) != (
                &new.track_id,
                &new.title,
                &new.artist,
                &new.album,
                new.length_us,
            ) {
                player.metadata_changed(emitter).await?;
            }
            if old.volume != new.volume {
                player.volume_changed(emitter).await?;
            }
            if old.shuffle != new.shuffle {
                player.shuffle_changed(emitter).await?;
            }
            if old.repeat != new.repeat {
                player.loop_status_changed(emitter).await?;
            }
            if seeked {
                PlayerIface::seeked(emitter, new.position_us).await?;
            }
            Ok(())
        });
        if let Err(e) = result {
            log::debug!("MPRIS signal failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_paths_round_trip() {
        assert_eq!(track_id_from_path(&track_path(42)), Some(42));
        assert_eq!(
            track_id_from_path("/org/mpris/MediaPlayer2/TrackList/NoTrack"),
            None
        );
    }

    #[test]
    fn metadata_has_spec_keys_and_types() {
        let state = State {
            track_id: Some(7),
            title: Some("ハレ晴レユカイ".into()),
            artist: Some("Ma15".into()),
            album: Some("Best".into()),
            length_us: Some(225_000_000),
            ..State::default()
        };
        let m = metadata(&state);
        assert_eq!(
            m["mpris:trackid"],
            OwnedValue::try_from(Value::from(
                ObjectPath::try_from("/org/ampcraft/track/7").unwrap()
            ))
            .unwrap()
        );
        assert_eq!(i64::try_from(&m["mpris:length"]).unwrap(), 225_000_000);
        assert_eq!(
            <&str>::try_from(&m["xesam:title"]).unwrap(),
            "ハレ晴レユカイ"
        );
        let artists: Vec<String> = m["xesam:artist"].try_clone().unwrap().try_into().unwrap();
        assert_eq!(artists, vec!["Ma15".to_owned()]);
        let empty = metadata(&State::default());
        assert!(
            empty.contains_key("mpris:trackid"),
            "NoTrack path when nothing is loaded"
        );
        assert!(!empty.contains_key("xesam:title"));
    }
}
