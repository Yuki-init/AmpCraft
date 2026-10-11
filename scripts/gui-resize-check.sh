#!/bin/bash
# Resizing at 2x: drags the playlist and the media library by the bottom-right corner, the
# right edge and the bottom edge, and checks each changes the expected size (in 25 x 29
# steps, doubled). Screenshot goes to .cache/resize.png.
# Run inside the container: scripts/dev.sh scripts/gui-resize-check.sh
set -u
cargo build -q --bin openamp || exit 1
G=.cache/gui-resize
rm -rf "$G" && mkdir -p "$G"
export OPENAMP_CONFIG_DIR="$PWD/$G/config" OPENAMP_DATA_DIR="$PWD/$G/data"
Xvfb :96 -screen 0 2560x1600x24 -nolisten tcp >/dev/null 2>&1 &
XPID=$!
export DISPLAY=:96
sleep 1
openbox >/dev/null 2>&1 &
sleep 1
FAIL=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; FAIL=1; fi; }
geo() { xwininfo -name "$1" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width/{w=$2} /Height/{h=$2} END{print x, y, w, h}'; }
# Press at (x, y), move by (dx, dy) in two steps, release.
drag() {
    xdotool mousemove "$1" "$2" mousedown 1; sleep 0.2
    xdotool mousemove $(($1 + $3 / 2)) $(($2 + $4 / 2)); sleep 0.1
    xdotool mousemove $(($1 + $3)) $(($2 + $4)); sleep 0.2
    xdotool mouseup 1; sleep 0.4
}
target/debug/openamp --audio null --skin dracula --scale 2 --library >"$G/log" 2>&1 &
APID=$!
sleep 3
for n in Playlist "Media Library"; do
    read -r X Y W H <<<"$(geo "$n")"
    drag $((X + W - 6)) $((Y + H - 6)) 60 70           # corner: +1 step each way
    read -r _ _ W1 H1 <<<"$(geo "$n")"
    check "$n corner ${W}x$H -> ${W1}x$H1" '[ "$W1" = $((W + 50)) ] && [ "$H1" = $((H + 58)) ]'
    drag $((X + W1 - 2)) $((Y + H1 / 2)) 60 200         # right edge: width only
    read -r _ _ W2 H2 <<<"$(geo "$n")"
    check "$n right edge -> ${W2}x$H2" '[ "$W2" = $((W1 + 50)) ] && [ "$H2" = "$H1" ]'
    drag $((X + W2 / 2)) $((Y + H2 - 2)) 200 70         # bottom edge: height only
    read -r _ _ W3 H3 <<<"$(geo "$n")"
    check "$n bottom edge -> ${W3}x$H3" '[ "$W3" = "$W2" ] && [ "$H3" = $((H2 + 58)) ]'
done
import -window root -crop 2000x1500+0+0 +repage .cache/resize.png && convert .cache/resize.png -scale 50% .cache/resize.png
kill $APID $XPID 2>/dev/null
[[ $FAIL == 0 ]] && echo "resize check passed" || { echo "resize check FAILED"; tail -5 "$G/log"; exit 1; }
