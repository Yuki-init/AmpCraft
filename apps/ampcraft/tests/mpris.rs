//! MPRIS over a real D-Bus session bus. Runs when `DBUS_SESSION_BUS_ADDRESS` is set
//! (scripts/test.sh starts a private bus with dbus-run-session), otherwise skips.
#![cfg(target_os = "linux")]
// Integration tests are test code; panicking on setup failure is the point.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use ampcraft::mpris::{Mpris, MprisCommand, MprisState, Status, track_path};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{ObjectPath, OwnedValue};

const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

fn bus() -> Option<Connection> {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        eprintln!("no session bus, skipping");
        return None;
    }
    Some(Connection::session().unwrap())
}

/// The bus name our service took (the shared name, or a per-process one).
fn service_name(conn: &Connection) -> String {
    let dbus = zbus::blocking::fdo::DBusProxy::new(conn).unwrap();
    dbus.list_names()
        .unwrap()
        .into_iter()
        .map(|n| n.to_string())
        .find(|n| n.starts_with("org.mpris.MediaPlayer2.ampcraft"))
        .expect("service registered")
}

fn wait_commands(mpris: &Mpris, n: usize) -> Vec<MprisCommand> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut got = Vec::new();
    while got.len() < n {
        assert!(Instant::now() < deadline, "only got {got:?}");
        got.extend(mpris.commands());
        std::thread::sleep(Duration::from_millis(10));
    }
    got
}

#[test]
fn remote_calls_become_commands_and_state_is_readable() {
    let Some(client) = bus() else { return };
    let mpris = Mpris::start().unwrap();
    let name = service_name(&client);
    let player = Proxy::new(&client, name.as_str(), "/org/mpris/MediaPlayer2", PLAYER).unwrap();

    player.call_method("PlayPause", &()).unwrap();
    player.call_method("Next", &()).unwrap();
    player.call_method("Seek", &(5_000_000i64)).unwrap();
    let track = ObjectPath::try_from(track_path(3)).unwrap();
    player
        .call_method("SetPosition", &(track, 42_000_000i64))
        .unwrap();
    player.set_property("Volume", 0.25f64).unwrap();
    player.set_property("Shuffle", true).unwrap();
    assert_eq!(
        wait_commands(&mpris, 6),
        vec![
            MprisCommand::PlayPause,
            MprisCommand::Next,
            MprisCommand::Seek(5_000_000),
            MprisCommand::SetPosition(3, 42_000_000),
            MprisCommand::SetVolume(0.25),
            MprisCommand::SetShuffle(true),
        ]
    );

    mpris.update(
        MprisState {
            status: Status::Playing,
            track_id: Some(3),
            title: Some("God Knows".into()),
            artist: Some("KEIKO".into()),
            length_us: Some(230_000_000),
            position_us: 12_000_000,
            volume: 0.25,
            ..MprisState::default()
        },
        true,
    );
    let status: String = player.get_property("PlaybackStatus").unwrap();
    assert_eq!(status, "Playing");
    let position: i64 = player.get_property("Position").unwrap();
    assert_eq!(position, 12_000_000);
    let metadata: HashMap<String, OwnedValue> = player.get_property("Metadata").unwrap();
    assert_eq!(
        <&str>::try_from(&metadata["xesam:title"]).unwrap(),
        "God Knows"
    );
    let root = Proxy::new(
        &client,
        name.as_str(),
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2",
    )
    .unwrap();
    let identity: String = root.get_property("Identity").unwrap();
    assert_eq!(identity, "AmpCraft");
}
