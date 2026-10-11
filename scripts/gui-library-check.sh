#!/bin/bash
# Drives the media library window under Xvfb with openbox: builds a sandboxed library from
# generated tracks, opens the window with --library, picks an artist, searches, plays
# by double-click, checks Most Played, clears the search and opens the Library menu.
# Screenshots go to .cache/lib-*.png.
# Run inside the container: scripts/dev.sh scripts/gui-library-check.sh
set -u
SKIN=${SKIN:-skins/base-2.91.wsz}
cargo build -q --bin openamp --bin openamp-cli || exit 1
G=.cache/gui-lib
rm -rf "$G" && mkdir -p "$G/music/Album One" "$G/music/Album Two"
export OPENAMP_CONFIG_DIR="$PWD/$G/config" OPENAMP_DATA_DIR="$PWD/$G/data" OPENAMP_MUSIC_DIR="$PWD/$G/music"
mk() { ffmpeg -v error -y -f lavfi -i "sine=f=$1:d=$2" -ac 2 -metadata artist="$3" -metadata album="$4" -metadata title="$5" -metadata track="$6" -metadata genre="$7" -c:a flac "$G/music/$4/$6 $5.flac"; }
mk 440 30 "Ma15" "Album One" "ハレ晴レユカイ" 1 Trance
mk 550 40 "KEIKO" "Album One" "God Knows" 2 Trance
mk 660 50 "chum" "Album Two" "創聖のアクエリオン" 1 Eurobeat
mk 770 20 "chum" "Album Two" "Butter-fly" 2 Eurobeat
target/debug/openamp-cli library scan

Xvfb :84 -screen 0 1280x1024x24 -nolisten tcp >/dev/null 2>&1 &
XPID=$!
export DISPLAY=:84
sleep 1
openbox >/dev/null 2>&1 &
sleep 1
target/debug/openamp --audio null --library --skin "$SKIN" >.cache/lib.log 2>&1 &
APID=$!
sleep 2.5
# Library window at x 375 (right of main), y 100; default size 625x406. Layout (window
# coordinates): tree rows from y 23; Artist pane at x 117 and Album pane at x 367 with rows
# from y 54; track rows from y 189; buttons at y 374 (Library at x 13, Play at x 117).
LX=375; LY=100
shot() { import -window root -crop 640x420+$((LX - 5))+$((LY - 5)) +repage ".cache/lib-$1.png"; echo "shot $1"; }
click() { xdotool mousemove $((LX + $1)) $((LY + $2)) click 1; sleep "${3:-0.4}"; }
shot 1-open
click 160 $((54 + 13 + 6))           # Artist pane: chum (rows: All, chum, KEIKO, Ma15)
shot 2-artist
xdotool type --delay 50 "fly"; sleep 0.5
shot 3-search
xdotool mousemove $((LX + 300)) $((LY + 189 + 6)) click --repeat 2 --delay 120 1; sleep 1.2
shot 4-playing
click 60 $((23 + 2 * 13 + 6)) 0.8    # tree: Most Played
shot 5-most-played
click 580 29                         # Clear Search
shot 6-cleared
click 60 $((374 + 7)) 0.6            # Library button: its menu
import -window root -crop 300x120+$((LX))+$((LY + 300)) +repage .cache/lib-7-menu.png
xdotool key Escape; sleep 0.3
import -window root -crop 280x120+95+95 +repage .cache/lib-8-main.png
kill $APID $XPID 2>/dev/null
grep -i -E "error|panic" .cache/lib.log || true
