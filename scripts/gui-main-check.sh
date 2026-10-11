#!/bin/bash
# Drives the real player under Xvfb with openbox, using generated tracks and the paced null
# audio sink, clicking through the main window with xdotool. Screenshots (cropped to the
# window) go to .cache/main-*.png. Needs a skin: SKIN=path (default skins/base-2.91.wsz).
# Run inside the container: scripts/dev.sh scripts/gui-main-check.sh
set -u
SKIN=${SKIN:-skins/base-2.91.wsz}
cargo build -q --bin openamp || exit 1
G=.cache/gui
mkdir -p "$G"
ffmpeg -v error -y -f lavfi -i "sine=f=440:d=40" -ac 2 -metadata title="ハレ晴レユカイ" \
    -metadata artist="Ma15" -c:a flac "$G/01.flac"
ffmpeg -v error -y -f lavfi -i "sine=f=660:d=20" -ac 1 \
    -metadata title="Plain Title That Is Long Enough To Scroll Around" -metadata artist="Generator" \
    -c:a libmp3lame -b:a 128k "$G/02.mp3"

Xvfb :78 -screen 0 1280x1024x24 -nolisten tcp >/dev/null 2>&1 &
XPID=$!
export DISPLAY=:78
sleep 1
openbox >/dev/null 2>&1 &
sleep 1
target/debug/openamp --audio null --skin "$SKIN" "$G/01.flac" "$G/02.mp3" >.cache/main.log 2>&1 &
APID=$!
sleep 2.5

X=100; Y=100; S=1
shot() { import -window root -crop $((275*S))x$((116*S))+$X+$Y +repage ".cache/main-$1.png"; echo "shot $1"; }
at() { echo $((X + $1 * S)) $((Y + $2 * S)); }
click() { xdotool mousemove $(at "$1" "$2") click 1; sleep "${3:-0.4}"; }

shot 1-playing
click 73 97 0.3              # pause
shot 2-paused
click 70 32                  # time display: toggle remaining
click 73 97 0.6              # pause again: resume
shot 3-remaining
xdotool mousemove $(at 139 63) mousedown 1; sleep 0.1
xdotool mousemove $(at 124 63); sleep 0.2
shot 4-volume-drag
xdotool mouseup 1; sleep 0.2
click 119 97 1.5             # next track
shot 5-next
xdotool mousemove $(at 80 77) mousedown 1; sleep 0.1
xdotool mousemove $(at 180 77); sleep 0.2
shot 6-seek-drag
xdotool mouseup 1; sleep 0.6
shot 7-after-seek
click 96 97 0.5              # stop
shot 8-stopped
click 14 51 0.8              # clutter bar D: double size
S=2
shot 9-double
click 268 7 0.8              # close
kill -0 $APID 2>/dev/null && echo "player still running (BAD)" || echo "player exited on close"
cat .cache/main.log
kill $XPID 2>/dev/null
