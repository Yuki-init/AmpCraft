#!/bin/bash
# Window shade: shades the main window (button), equalizer (title double-click) and playlist
# (button), checks the docked stack closes up, restarts to check it is restored, then
# unshades. Screenshots go to .cache/shade-*.png.
# Run inside the container: scripts/dev.sh scripts/gui-shade-check.sh
set -u
SKIN=${SKIN:-skins/base-2.91.wsz}
cargo build -q --bin ampcraft || exit 1
G=.cache/gui-shade
rm -rf "$G" && mkdir -p "$G"
export AMPCRAFT_CONFIG_DIR="$PWD/$G/config" AMPCRAFT_DATA_DIR="$PWD/$G/data"
ffmpeg -v error -y -f lavfi -i "sine=f=440:d=60" -ac 2 -metadata artist=Ma15 -metadata title="ハレ晴レユカイ" -c:a flac "$G/1.flac"
Xvfb :87 -screen 0 1024x768x24 -nolisten tcp >/dev/null 2>&1 &
XPID=$!
export DISPLAY=:87
sleep 1
openbox >/dev/null 2>&1 &
sleep 1
FAIL=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; FAIL=1; fi; }
geo() { xwininfo -name "$1" | awk '/Absolute upper-left Y/{y=$4} /Height/{h=$2} END{print y","h}'; }
target/debug/ampcraft --audio null --skin "$SKIN" "$G/1.flac" >.cache/shade.log 2>&1 &
APID=$!
sleep 2
xdotool mousemove $((100 + 258)) $((100 + 7)) click 1; sleep 0.5        # main: shade button
xdotool mousemove 200 $((100 + 14 + 6)) click --repeat 2 --delay 100 1; sleep 0.5   # EQ title double-click
xdotool mousemove $((100 + 275 - 17)) $((100 + 28 + 7)) click 1; sleep 0.5   # playlist: shade button
check "main is a 14 px bar ($(geo AmpCraft))" '[ "$(geo AmpCraft)" = "100,14" ]'
check "equalizer docked under it ($(geo Equalizer))" '[ "$(geo Equalizer)" = "114,14" ]'
check "playlist docked under that ($(geo Playlist))" '[ "$(geo Playlist)" = "128,14" ]'
import -window root -crop 290x50+95+95 +repage .cache/shade-1-all.png
xdotool mousemove $((100 + 268)) $((100 + 7)) click 1; sleep 1           # close (shade bar close)
kill -0 $APID 2>/dev/null && { echo "FAIL did not close"; kill $APID; FAIL=1; }
target/debug/ampcraft --audio null >.cache/shade-2.log 2>&1 &
APID=$!
sleep 2
check "shaded state restored ($(geo AmpCraft) $(geo Playlist))" '[ "$(geo AmpCraft)" = "100,14" ] && [ "$(geo Playlist)" = "128,14" ]'
xdotool mousemove $((100 + 258)) $((100 + 7)) click 1; sleep 0.5        # unshade main
xdotool mousemove $((100 + 258)) $((216 + 7)) click 1; sleep 0.5        # unshade EQ
xdotool mousemove $((100 + 275 - 17)) $((332 + 7)) click 1; sleep 0.5   # unshade playlist (now under the full EQ)
check "full layout back ($(geo AmpCraft) $(geo Equalizer) $(geo Playlist))" '[ "$(geo AmpCraft)" = "100,116" ] && [ "$(geo Equalizer)" = "216,116" ] && [ "$(geo Playlist)" = "332,232" ]'
import -window root -crop 290x480+95+95 +repage .cache/shade-2-unshaded.png
kill $APID $XPID 2>/dev/null
[[ $FAIL == 0 ]] && echo "shade check passed" || { echo "shade check FAILED"; exit 1; }
