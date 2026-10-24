#!/bin/bash
# Playlist bottom-bar menus: MISC > SORT opens the sort menu (sort by title, reverse),
# REM > SEL removes the selection, SEL > INV inverts it, ADD > URL explains that streams are
# not supported. The resulting order is checked in the playlist saved on exit.
# Screenshots go to .cache/plmenu-*.png.
# Run inside the container: scripts/dev.sh scripts/gui-playlist-menu-check.sh
set -u
cargo build -q --bin ampcraft || exit 1
G=.cache/gui-plmenu
rm -rf "$G" && mkdir -p "$G"
export AMPCRAFT_CONFIG_DIR="$PWD/$G/config" AMPCRAFT_DATA_DIR="$PWD/$G/data" AMPCRAFT_LOG=info
for t in "1 Charlie" "2 Alpha" "3 Bravo"; do
    set -- $t
    ffmpeg -v error -y -f lavfi -i "sine=f=440:d=5" -ac 2 -metadata title="$2" -c:a flac "$G/$1.flac"
done
Xvfb :89 -screen 0 1024x768x24 -nolisten tcp >/dev/null 2>&1 &
XPID=$!
export DISPLAY=:89
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
target/debug/ampcraft --audio null --skin dracula --scale 1 "$G/1.flac" "$G/2.flac" "$G/3.flac" >"$G/log" 2>&1 &
APID=$!
sleep 3
# Playlist at (100, 332), 275x232; bottom-bar buttons at y = 332 + 202.
PY=$((332 + 202 + 9))
button() { xdotool mousemove $((100 + $1 + 11)) $PY click 1; sleep 0.4; }
# Item n (0 = top) of a menu with `count` items, above a button at x offset $1.
item() { xdotool mousemove $((100 + $1 + 11)) $((332 + 202 - ($2 - 1 - $3) * 18 + 9)) click 1; sleep 0.4; }
row() { xdotool mousemove 150 $((332 + 20 + $1 * 13 + 6)) click 1; sleep 0.3; }

button 101                                  # MISC
import -window root -crop 290x250+95+325 +repage .cache/plmenu-1-misc.png
item 101 3 0                                # SORT
read -r MX MY MW MH <<<"$(menu_geo 1)"
check "SORT opens the sort menu" '[ -n "${MX:-}" ]'
xdotool mousemove $((MX + 30)) $((MY + 1 + 8)) click 1; sleep 0.5   # Sort by title
row 0                                       # select Alpha
button 43                                   # REM
item 43 4 2                                 # SEL: remove selected
row 0                                       # select Bravo
button 72                                   # SEL
item 72 3 0                                 # INV: Charlie selected instead
import -window root -crop 290x250+95+325 +repage .cache/plmenu-2-inverted.png
button 101                                  # MISC > SORT > Reverse list
item 101 3 0
read -r MX MY MW MH <<<"$(menu_geo 1)"
xdotool mousemove $((MX + 30)) $((MY + 1 + 16 * 3 + 7 + 8)) click 1; sleep 0.5
button 14                                   # ADD
item 14 3 0                                 # URL
check "ADD > URL explains (no dialog helper here)" 'grep -q "no dialog helper" "$G/log"'
xdotool mousemove $((100 + 268)) $((100 + 7)) click 1; sleep 1   # close the player
kill -0 $APID 2>/dev/null && { echo "FAIL did not close"; kill $APID; FAIL=1; }
ORDER=$(grep -v '^#' "$G/data/playlist.m3u8" | xargs -n1 basename | tr '\n' ' ')
check "sorted, removed and reversed ($ORDER)" '[ "$ORDER" = "1.flac 3.flac " ]'
kill $XPID 2>/dev/null
[[ $FAIL == 0 ]] && echo "playlist menu check passed" || { echo "playlist menu check FAILED"; tail -5 "$G/log"; exit 1; }
