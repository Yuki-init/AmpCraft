#!/bin/bash
# Preferences window: opens with O, changes settings on each page (always on top, size,
# shuffle, visualizer, theme), removes a library folder, moves the window by its title bar,
# closes, and checks the saved config. Screenshots go to .cache/prefs-*.png.
# Run inside the container: scripts/dev.sh scripts/gui-prefs-check.sh
set -u
cargo build -q --bin ampcraft || exit 1
G=.cache/gui-prefs
rm -rf "$G" && mkdir -p "$G/config" "$G/one" "$G/two"
export AMPCRAFT_CONFIG_DIR="$PWD/$G/config" AMPCRAFT_DATA_DIR="$PWD/$G/data" AMPCRAFT_LOG=info
cat >"$G/config/config.toml" <<TOML
[player]
scale = 1

[library]
include_music_folder = false
folders = ["$PWD/$G/one", "$PWD/$G/two"]
TOML
Xvfb :97 -screen 0 1024x768x24 -nolisten tcp >/dev/null 2>&1 &
XPID=$!
export DISPLAY=:97
sleep 1
openbox >/dev/null 2>&1 &
sleep 1
FAIL=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; FAIL=1; fi; }
geo() { xwininfo -name "$1" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width/{w=$2} /Height/{h=$2} END{print x, y, w, h}'; }
target/debug/ampcraft --audio null --skin dracula >"$G/log" 2>&1 &
APID=$!
sleep 2
xdotool mousemove $((100 + 13)) $((100 + 28)) click 1; sleep 0.8    # O
read -r PX PY PW PH <<<"$(geo Preferences)"
check "preferences open (${PW}x$PH)" '[ "$PW" = 400 ] && [ "$PH" = 290 ]'
# Page layout (window coordinates): page list rows at y 30, 44, 58, 72 (x 30); items at x 131
# from y 24 in 14 px rows, with 4 px more above each heading after the first.
at() { xdotool mousemove $((PX + $1)) $((PY + $2)) click 1; sleep 0.4; }
import -window root -crop 410x300+$((PX - 5))+$((PY - 5)) +repage .cache/prefs-1-general.png
at 131 45                       # Keep the player above other windows
check "always on top" 'xprop -name AmpCraft _NET_WM_STATE | grep -q ABOVE'
at 131 105                      # Double (2x): rows Auto 77, 1x 91, 2x 105
read -r _ _ MW MH <<<"$(geo AmpCraft)"
check "double size from the preferences (${MW}x$MH)" '[ "$MW" = 550 ]'
read -r PX PY PW PH <<<"$(geo Preferences)"
check "preferences follow the scale (${PW}x$PH)" '[ "$PW" = 800 ]'
at $((131 * 2)) $((91 * 2))     # Normal (1x), now at 2x coordinates
read -r PX PY PW PH <<<"$(geo Preferences)"
at 30 44                        # Playback page
at 131 59                       # Shuffle (Repeat 45, Shuffle 59)
at 131 105                      # Oscilloscope (Spectrum 91, Oscilloscope 105)
import -window root -crop 410x300+$((PX - 5))+$((PY - 5)) +repage .cache/prefs-2-playback.png
at 30 58                        # Skins page
at 131 45                       # System colors (System 45, Dracula 59, Nord 73)
check "system theme chosen" 'grep -q "theme: System" "$G/log"'
at 131 73                       # Nord
check "theme switched" 'grep -q "theme: Nord" "$G/log"'
at 30 72                        # Media Library page
import -window root -crop 410x300+$((PX - 5))+$((PY - 5)) +repage .cache/prefs-3-library.png
at 360 60                       # Remove the first folder
import -window root -crop 410x300+$((PX - 5))+$((PY - 5)) +repage .cache/prefs-4-removed.png
xdotool mousemove $((PX + 150)) $((PY + 8)) mousedown 1; sleep 0.1
xdotool mousemove $((PX + 190)) $((PY + 48)); sleep 0.1
xdotool mousemove $((PX + 230)) $((PY + 88)); sleep 0.2; xdotool mouseup 1; sleep 0.4
read -r QX QY _ _ <<<"$(geo Preferences)"
check "title bar drag moves it ($PX,$PY -> $QX,$QY)" '[ "$QX" = $((PX + 80)) ] && [ "$QY" = $((PY + 80)) ]'
c=$(xwininfo -name Preferences | awk '/Width/{print $2}')
xdotool mousemove $((QX + c - 7)) $((QY + 7)) click 1; sleep 0.5      # close button
check "close button hides it" '[ -z "$(xdotool search --onlyvisible --name "^Preferences$")" ]'
xdotool mousemove $((100 + 268)) $((100 + 7)) click 1; sleep 1          # quit
kill -0 $APID 2>/dev/null && { echo "FAIL did not quit"; kill $APID; FAIL=1; }
C="$G/config/config.toml"
check "saved: on top, shuffle, oscilloscope, nord, scale 1" 'grep -q "always_on_top = true" "$C" && grep -q "shuffle = true" "$C" && grep -q "oscilloscope" "$C" && grep -q "skin = \"nord\"" "$C" && grep -q "scale = 1" "$C"'
check "saved: one folder left" '! grep -q "gui-prefs/one" "$C" && grep -q "gui-prefs/two" "$C"'
kill $XPID 2>/dev/null
[[ $FAIL == 0 ]] && echo "prefs check passed" || { echo "prefs check FAILED"; tail -5 "$G/log"; exit 1; }
