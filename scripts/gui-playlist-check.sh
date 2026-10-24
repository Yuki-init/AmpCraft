#!/bin/bash
# Drives the player's main, equalizer and playlist windows under Xvfb with openbox, using
# generated tracks and the paced null audio sink. Screenshots go to .cache/pl-*.png.
# Needs a skin: SKIN=path (default skins/base-2.91.wsz).
# Run inside the container: scripts/dev.sh scripts/gui-playlist-check.sh
set -u
SKIN=${SKIN:-skins/base-2.91.wsz}
cargo build -q --bin ampcraft || exit 1
G=.cache/gui-pl
mkdir -p "$G"
mk() { ffmpeg -v error -y -f lavfi -i "sine=f=$1:d=$2" -ac 2 "${@:4}" "$G/$3"; }
mk 440 40 "01 first.flac" -metadata artist="Ma15" -metadata title="ハレ晴レユカイ" -c:a flac
mk 550 75 "02 second.mp3" -metadata artist="KEIKO" -metadata title="God Knows..." -c:a libmp3lame
mk 660 30 "03 untagged.ogg" -c:a libvorbis
mk 770 200 "04 long.opus" -metadata artist="chum" -metadata title="創聖のアクエリオン" -c:a libopus

Xvfb :79 -screen 0 1280x1024x24 -nolisten tcp >/dev/null 2>&1 &
XPID=$!
export DISPLAY=:79
sleep 1
openbox >/dev/null 2>&1 &
sleep 1
target/debug/ampcraft --audio null --skin "$SKIN" "$G" >.cache/pl.log 2>&1 &
APID=$!
sleep 3

shot() { import -window root -crop "${2:-500x560}+${3:-90}+${4:-90}" +repage ".cache/pl-$1.png"; echo "shot $1"; }
click() { xdotool mousemove "$1" "$2" click 1; sleep "${3:-0.4}"; }
PLY=332                       # playlist top
row() { echo $((PLY + 23 + $1 * 13 + 6)); }

shot 1-start
click 127 240                 # EQ ON
xdotool mousemove 185 300 mousedown 1; sleep 0.1; xdotool mousemove 185 260; sleep 0.1; xdotool mouseup 1
xdotool mousemove 221 300 mousedown 1; sleep 0.1; xdotool mousemove 221 285; sleep 0.1; xdotool mouseup 1
sleep 0.3
shot 2-eq
xdotool mousemove 200 "$(row 2)" click --repeat 2 --delay 120 1; sleep 1
shot 3-dblclick-row3
click 200 "$(row 0)" 0.2      # select first row
click 154 $((PLY + 232 - 21)) 0.4   # REM opens its menu
click 154 $((PLY + 232 - 21 - 18)) 0.6   # REM SEL: remove the selection
shot 4-removed
xdotool mousemove 365 $((PLY + 222)) mousedown 1; sleep 0.1
xdotool mousemove 390 $((PLY + 250)); sleep 0.1
xdotool mousemove 415 $((PLY + 282)); sleep 0.2
xdotool mouseup 1; sleep 0.4
shot 5-resized 560x620
click 353 164 0.5             # PL button on main: hide playlist
shot 6-pl-hidden 560x620
click 353 164 0.5             # show again
xdotool mousemove 150 105 mousedown 1; sleep 0.1
xdotool mousemove 300 150; sleep 0.1; xdotool mousemove 450 205; sleep 0.2; xdotool mouseup 1; sleep 0.5
shot 7-group-moved 640x660 340 180
for n in AmpCraft Equalizer Playlist; do xwininfo -name "$n" | awk -v n="$n" '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width/{w=$2} /Height/{h=$2} END{printf "  %-10s %d,%d %dx%d\n", n, x, y, w, h}'; done
read MX MY < <(xwininfo -name AmpCraft | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} END{print x, y}')
click $((MX + 268)) $((MY + 7)) 0.8    # close
kill -0 $APID 2>/dev/null && { echo "player still running (BAD)"; kill $APID; } || echo "player exited on close"
cat .cache/pl.log
kill $XPID 2>/dev/null
