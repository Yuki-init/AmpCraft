#!/bin/bash
# Checks that activating any player window brings all of them to the front, and that
# activating another app still covers them: an xmessage window overlaps the EQ and playlist,
# then the main window, the other app and the playlist are clicked in turn, sampling pixels
# that belong to the EQ and playlist. Exits non-zero on failure. Screenshots: .cache/raise-*.png
# Run inside the container: scripts/dev.sh scripts/gui-raise-check.sh
set -u
SKIN=${SKIN:-skins/base-2.91.wsz}
cargo build -q --bin ampcraft || exit 1
Xvfb :81 -screen 0 1280x1024x24 -nolisten tcp >/dev/null 2>&1 &
XPID=$!
export DISPLAY=:81
sleep 1
openbox >/dev/null 2>&1 &
sleep 1
target/debug/ampcraft --audio null --skin "$SKIN" >.cache/raise.log 2>&1 &
APID=$!
sleep 2
# Main 100..375 x 100..216, EQ below to 332, playlist below to 564. The other app covers
# x 40..240 from y 200, overlapping the EQ and playlist but leaving x 40..100 its own.
xmessage -geometry 200x420+40+200 -bg white "another application" &
OPID=$!
sleep 1
px() { import -window root -crop 1x1+$1+$2 +repage txt:- | tail -1 | grep -o "#[0-9A-F]\{6\}" | head -1; }
shot() { import -window root -crop 600x560+30+80 +repage ".cache/raise-$1.png"; }
FAIL=0
expect() { # name, want "ours" or "other"
    local eq pl
    eq=$(px 200 260); pl=$(px 200 420)
    local got=ours
    [[ "$eq" == "#FFFFFF" && "$pl" == "#FFFFFF" ]] && got=other
    printf "%-28s EQ %s  playlist %s  -> %s\n" "$1" "$eq" "$pl" "$got"
    [[ "$got" == "$2" ]] || { echo "  FAIL: expected $2 on top"; FAIL=1; }
    shot "$1"
}
expect 1-other-app-on-top other
xdotool mousemove 200 105 click 1; sleep 0.6       # main window title bar
expect 2-main-clicked ours
xdotool mousemove 60 520 click 1; sleep 0.6        # the other app, outside our windows
expect 3-other-app-clicked other
xdotool mousemove 330 420 click 1; sleep 0.6       # visible part of the playlist
expect 4-playlist-clicked ours
kill $OPID $APID 2>/dev/null
kill $XPID 2>/dev/null
[[ $FAIL == 0 ]] && echo "raise check passed" || { echo "raise check FAILED"; exit 1; }
