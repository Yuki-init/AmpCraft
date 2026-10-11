#!/bin/bash
# Drives openamp-dock-demo under Xvfb with the openbox window manager, using xdotool, and
# prints window positions after each step. Screenshots go to .cache/demo-*.png.
# Run inside the container: scripts/dev.sh scripts/gui-demo-check.sh
set -u
cargo build -q --bin openamp-dock-demo || exit 1
Xvfb :77 -screen 0 1280x1024x24 -nolisten tcp >/dev/null 2>&1 &
XPID=$!
export DISPLAY=:77
sleep 1
openbox >/dev/null 2>&1 &
sleep 1
target/debug/openamp-dock-demo >.cache/demo.log 2>&1 &
DPID=$!
sleep 2
pos() { for n in OpenAMP Equalizer Playlist; do xwininfo -name "$n" | awk -v n="$n" '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width/{w=$2} /Height/{h=$2} END{printf "  %-10s %d,%d %dx%d\n", n, x, y, w, h}'; done; }
echo "start:"; pos
import -window root .cache/demo-1-start.png
# Drag the playlist by its title bar and drop it a few px off the EQ's bottom-left.
xdotool mousemove 520 105 mousedown 1; sleep 0.1
for p in "450 200" "300 300" "200 340" "123 343"; do xdotool mousemove $p; sleep 0.05; done
xdotool mouseup 1; sleep 0.3
echo "after dropping playlist near EQ:"; pos
import -window root .cache/demo-2-snapped.png
# Drag the main window: the docked group follows.
xdotool mousemove 200 105 mousedown 1; sleep 0.1
for p in "300 200" "450 300" "500 305"; do xdotool mousemove $p; sleep 0.05; done
xdotool mouseup 1; sleep 0.3
echo "after dragging main by +300,+200:"; pos
import -window root .cache/demo-3-group.png
# Try to drag the group far off the bottom-right: it must stay reachable.
xdotool mousemove 500 305 mousedown 1; sleep 0.1
for p in "900 700" "1400 1200" "2000 2000"; do xdotool mousemove $p; sleep 0.05; done
xdotool mouseup 1; sleep 0.3
echo "after dragging toward off-screen:"; pos
import -window root .cache/demo-4-clamped.png
xdotool mousemove 1260 900 click 3; sleep 0.5
kill -0 $DPID 2>/dev/null && echo "demo still running (BAD)" || echo "demo exited on right-click"
cat .cache/demo.log
kill $XPID 2>/dev/null
