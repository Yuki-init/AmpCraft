#!/bin/bash
# Playlist, 5.x behaviors: Q queues a track to play next (Next then plays it), dragging a
# selected track moves it, and the right-click track menu removes one. The saved playlist
# and current track are checked after quitting. Screenshots go to .cache/pl5-*.png.
# Run inside the container: scripts/dev.sh scripts/gui-playlist-5x-check.sh
set -u
cargo build -q --bin openamp || exit 1
G=.cache/gui-pl5
rm -rf "$G" && mkdir -p "$G"
export OPENAMP_CONFIG_DIR="$PWD/$G/config" OPENAMP_DATA_DIR="$PWD/$G/data" OPENAMP_LOG=info
for t in A B C D; do
    ffmpeg -v error -y -f lavfi -i "sine=f=440:d=60" -ac 2 -metadata title="$t" -c:a flac "$G/$t.flac"
done
Xvfb :100 -screen 0 1024x768x24 -nolisten tcp >/dev/null 2>&1 &
XPID=$!
export DISPLAY=:100
sleep 1
openbox >/dev/null 2>&1 &
sleep 1
FAIL=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; FAIL=1; fi; }
menu_geo() {
    for id in $(xdotool search --onlyvisible --name '^Menu$'); do
        xwininfo -id "$id" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width/{w=$2} /Height/{h=$2} END{print x, y, w, h}'
    done | sort -n | sed -n "${1}p"
}
target/debug/openamp --audio null --skin dracula --scale 1 "$G"/{A,B,C,D}.flac >"$G/log" 2>&1 &
APID=$!
sleep 3
xdotool key x; sleep 0.5                         # stop the autoplay restart: play from A
row() { echo $((332 + 20 + $1 * 13 + 6)); }
xdotool mousemove 160 "$(row 3)" click 1; sleep 0.3   # select D
xdotool key q; sleep 0.4                         # queue it
import -window root -crop 290x240+95+325 +repage .cache/pl5-1-queued.png
xdotool key b; sleep 0.8                         # Next: the queued D, not B
import -window root -crop 290x240+95+325 +repage .cache/pl5-2-next.png
# Drag A (row 0) down to row 2: B C A D.
xdotool mousemove 160 "$(row 0)" mousedown 1; sleep 0.2
xdotool mousemove 160 "$(row 1)"; sleep 0.1; xdotool mousemove 160 "$(row 2)"; sleep 0.2
xdotool mouseup 1; sleep 0.4
import -window root -crop 290x240+95+325 +repage .cache/pl5-3-dragged.png
# Right-click C (row 1), choose Remove (rows: Play, Queue, separator, Remove).
xdotool mousemove 160 "$(row 1)" click 3; sleep 0.5
read -r MX MY MW MH <<<"$(menu_geo 1)"
check "right-click opens the track menu" '[ -n "${MX:-}" ]'
import -window root -crop 400x300+95+325 +repage .cache/pl5-4-menu.png
xdotool mousemove $((MX + 30)) $((MY + 1 + 32 + 7 + 8)) click 1; sleep 0.5
xdotool mousemove $((100 + 268)) $((100 + 7)) click 1; sleep 1       # quit
kill -0 $APID 2>/dev/null && { echo "FAIL did not quit"; kill $APID; FAIL=1; }
ORDER=$(grep -v '^#' "$G/data/playlist.m3u8" | xargs -n1 basename | tr '\n' ' ')
check "dragged and removed ($ORDER)" '[ "$ORDER" = "B.flac A.flac D.flac " ]'
check "Next played the queued track (D is current)" 'grep -q "current_track = 2" "$G/config/config.toml"'
kill $XPID 2>/dev/null
[[ $FAIL == 0 ]] && echo "playlist 5.x check passed" || { echo "playlist 5.x check FAILED"; grep current_track "$G/config/config.toml"; tail -5 "$G/log"; exit 1; }
