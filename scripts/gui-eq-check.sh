#!/bin/bash
# Equalizer: the PRESETS button opens the presets menu and Load > Rock applies that curve;
# the shade bar's volume slider sets the volume; both are saved on exit. Screenshots of the
# window and its shade bar go to .cache/eq-*.png.
# Run inside the container: scripts/dev.sh scripts/gui-eq-check.sh   (SKIN=... to change)
set -u
SKIN=${SKIN:-dracula}
cargo build -q --bin ampcraft || exit 1
G=.cache/gui-eq
rm -rf "$G" && mkdir -p "$G"
export AMPCRAFT_CONFIG_DIR="$PWD/$G/config" AMPCRAFT_DATA_DIR="$PWD/$G/data" AMPCRAFT_LOG=info
Xvfb :99 -screen 0 1024x768x24 -nolisten tcp >/dev/null 2>&1 &
XPID=$!
export DISPLAY=:99
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
target/debug/ampcraft --audio null --skin "$SKIN" --scale 1 >"$G/log" 2>&1 &
APID=$!
sleep 2
EX=100; EY=216                                  # equalizer window
import -window root -crop 275x116+$EX+$EY +repage .cache/eq-1-window.png
xdotool mousemove $((EX + 239)) $((EY + 24)) click 1; sleep 0.5      # PRESETS
read -r MX MY MW MH <<<"$(menu_geo 1)"
check "PRESETS opens a menu" '[ -n "${MX:-}" ]'
xdotool mousemove $((MX + 30)) $((MY + 1 + 8)); sleep 0.5            # Load
read -r SX SY SW SH <<<"$(menu_geo 2)"
check "Load lists the presets ($SH px)" '[ "${SH:-0}" -gt 300 ]'
import -window root -crop 500x420+$((EX + 150))+$((EY - 10)) +repage .cache/eq-2-presets.png
# Rock is the 14th entry (Classical ... Reggae, Rock).
xdotool mousemove $((SX + 20)) $((SY + 1 + 8)) mousemove $((SX + 20)) $((SY + 1 + 13 * 16 + 8)) click 1; sleep 0.5
check "Rock applied" 'grep -q "equalizer preset: Rock" "$G/log"'
import -window root -crop 275x116+$EX+$EY +repage .cache/eq-3-rock.png
xdotool mousemove $((EX + 258)) $((EY + 7)) click 1; sleep 0.5       # shade
import -window root -crop 275x14+$EX+$EY +repage .cache/eq-4-shade.png
xdotool mousemove $((EX + 61 + 50)) $((EY + 7)) mousedown 1; sleep 0.1
xdotool mousemove $((EX + 61)) $((EY + 7)); sleep 0.2; xdotool mouseup 1; sleep 0.3
import -window root -crop 275x14+$EX+$EY +repage .cache/eq-5-shade-quiet.png
xdotool mousemove $((100 + 268)) $((100 + 7)) click 1; sleep 1       # quit
kill -0 $APID 2>/dev/null && { echo "FAIL did not quit"; kill $APID; FAIL=1; }
C="$G/config/config.toml"
check "saved: Rock bands" 'grep -A4 "bands_db" "$C" | tr -d " \n" | grep -q "^bands_db=\[5.5,3.5,-4.0,-5.5"'
check "saved: volume down from the shade bar" 'grep -q "^volume = 0.0" "$C"'
kill $XPID 2>/dev/null
[[ $FAIL == 0 ]] && echo "eq check passed" || { echo "eq check FAILED"; grep -A3 bands_db "$C"; grep volume "$C"; exit 1; }
