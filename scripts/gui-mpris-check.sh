#!/bin/bash
# Controls a running player over MPRIS with dbus-send, like media keys and desktop widgets
# do: status, pause, next track, volume and quit. Uses a private session bus.
# Run inside the container: scripts/dev.sh dbus-run-session -- scripts/gui-mpris-check.sh
set -u
SKIN=${SKIN:-skins/base-2.91.wsz}
cargo build -q --bin ampcraft || exit 1
G=.cache/gui-mpris
rm -rf "$G" && mkdir -p "$G/music"
export AMPCRAFT_CONFIG_DIR="$PWD/$G/config" AMPCRAFT_DATA_DIR="$PWD/$G/data"
ffmpeg -v error -y -f lavfi -i "sine=f=440:d=60" -ac 2 -metadata artist=Ma15 -metadata title="ハレ晴レユカイ" -c:a flac "$G/music/1.flac"
ffmpeg -v error -y -f lavfi -i "sine=f=550:d=60" -ac 2 -metadata artist=KEIKO -metadata title="God Knows" -c:a flac "$G/music/2.flac"
Xvfb :86 -screen 0 1024x768x24 -nolisten tcp >/dev/null 2>&1 &
XPID=$!
export DISPLAY=:86
sleep 1
target/debug/ampcraft --audio null --skin "$SKIN" "$G/music/1.flac" "$G/music/2.flac" >.cache/mpris.log 2>&1 &
APID=$!
sleep 2
DEST=org.mpris.MediaPlayer2.ampcraft
call() { dbus-send --session --print-reply --dest=$DEST /org/mpris/MediaPlayer2 "$@"; }
prop() { call org.freedesktop.DBus.Properties.Get string:org.mpris.MediaPlayer2.Player string:"$1"; }
FAIL=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; FAIL=1; fi; }
check "playing at start" 'prop PlaybackStatus | grep -q "\"Playing\""'
check "metadata has the first title" 'prop Metadata | grep -q "ハレ晴レユカイ"'
call org.mpris.MediaPlayer2.Player.Pause >/dev/null; sleep 0.3
check "paused after Pause" 'prop PlaybackStatus | grep -q "\"Paused\""'
call org.mpris.MediaPlayer2.Player.PlayPause >/dev/null; sleep 0.3
check "playing after PlayPause" 'prop PlaybackStatus | grep -q "\"Playing\""'
call org.mpris.MediaPlayer2.Player.Next >/dev/null; sleep 0.5
check "next track title" 'prop Metadata | grep -q "God Knows"'
call org.freedesktop.DBus.Properties.Set string:org.mpris.MediaPlayer2.Player string:Volume variant:double:0.3 >/dev/null; sleep 0.3
check "volume set" 'prop Volume | grep -q "double 0.3"'
import -window root -crop 275x116+100+100 +repage .cache/mpris-main.png
call org.mpris.MediaPlayer2.Quit >/dev/null; sleep 1
check "quit over MPRIS" '! kill -0 $APID 2>/dev/null'
kill $APID $XPID 2>/dev/null
[[ $FAIL == 0 ]] && echo "MPRIS check passed" || { echo "MPRIS check FAILED"; cat .cache/mpris.log; exit 1; }
