#!/bin/bash
# Clutter bar and menus: O opens the preferences window (Escape closes it), A keeps the
# windows on top, right-click opens the main menu with a working Skins
# submenu, right-click on the visualizer picks its mode, I asks for the file info box, and
# Exit quits with the choices saved. Screenshots go to .cache/menu-*.png.
# Run inside the container: scripts/dev.sh scripts/gui-menu-check.sh
set -u
cargo build -q --bin openamp || exit 1
G=.cache/gui-menu
rm -rf "$G" && mkdir -p "$G"
export OPENAMP_CONFIG_DIR="$PWD/$G/config" OPENAMP_DATA_DIR="$PWD/$G/data" OPENAMP_LOG=info
ffmpeg -v error -y -f lavfi -i "sine=f=440:d=60" -ac 2 -metadata title="Tone" -c:a flac "$G/1.flac"
Xvfb :88 -screen 0 1024x768x24 -nolisten tcp >/dev/null 2>&1 &
XPID=$!
export DISPLAY=:88
sleep 1
openbox >/dev/null 2>&1 &
sleep 1
FAIL=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; FAIL=1; fi; }
menus() { xdotool search --onlyvisible --name '^Menu$' 2>/dev/null | wc -l; }
# Geometry of the n-th visible menu window, as "x y w h", deepest last.
menu_geo() {
    for id in $(xdotool search --onlyvisible --name '^Menu$'); do
        xwininfo -id "$id" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width/{w=$2} /Height/{h=$2} END{print x, y, w, h}'
    done | sort -n | sed -n "${1}p"
}
height() { xwininfo -name OpenAMP | awk '/Height/{print $2}'; }
target/debug/openamp --audio null --skin dracula --scale 1 "$G/1.flac" >"$G/log" 2>&1 &
APID=$!
sleep 2

# O button: the preferences window.
prefs() { xdotool search --onlyvisible --name '^Preferences$' 2>/dev/null | wc -l; }
xdotool mousemove $((100 + 13)) $((100 + 28)) click 1; sleep 0.7
check "O opens the preferences" '[ "$(prefs)" = 1 ]'
xdotool key Escape; sleep 0.4
check "Escape closes them" '[ "$(prefs)" = 0 ]'

# A button: always on top.
xdotool mousemove $((100 + 13)) $((100 + 36)) click 1; sleep 0.5
check "A sets always on top" 'xprop -name OpenAMP _NET_WM_STATE | grep -q ABOVE'
check "on the panels too" 'xprop -name Equalizer _NET_WM_STATE | grep -q ABOVE'

# Right-click: main menu; hover Skins, choose the second theme in the submenu.
xdotool mousemove 200 150 click 3; sleep 0.5
check "right-click opens the main menu" '[ "$(menus)" = 1 ]'
read -r MX MY MW MH <<<"$(menu_geo 1)"
# Rows: About, Preferences, sep, 3 items, sep, 3 items, sep, Skins (16 px rows, 7 px
# separators, 1 px border).
xdotool mousemove $((MX + 30)) $((MY + 1 + 32 + 7 + 48 + 7 + 48 + 7 + 8)); sleep 0.5
check "Skins opens a submenu" '[ "$(menus)" = 2 ]'
import -window root -crop 500x330+150+140 +repage .cache/menu-2-skins.png
read -r SX SY SW SH <<<"$(menu_geo 2)"
xdotool mousemove $((SX + 20)) $((SY + 1 + 8)) mousemove $((SX + 20)) $((SY + 1 + 16 + 8)) click 1; sleep 0.5
check "choosing closes the menu" '[ "$(menus)" = 0 ]'
check "theme changed" 'grep -q "theme: Nord" "$G/log"'

# Right-click on the visualizer: choose Oscilloscope.
xdotool mousemove $((100 + 40)) $((100 + 48)) click 3; sleep 0.5
read -r MX MY MW MH <<<"$(menu_geo 1)"
check "visualizer menu is short ($MH)" '[ "$MH" = $((3 * 16 + 2)) ]'
xdotool mousemove $((MX + 30)) $((MY + 1 + 16 + 8)) click 1; sleep 0.5

# I button: the file info box (no dialog helper in the container, so it says so).
xdotool mousemove $((100 + 13)) $((100 + 43)) click 1; sleep 0.5
check "I asks for the file info box" 'grep -q "no dialog helper" "$G/log"'

# Right-click > Options > Double size (Options rows: Preferences, Skins, sep, elapsed,
# remaining, on top, double).
xdotool mousemove 200 150 click 3; sleep 0.5
read -r MX MY MW MH <<<"$(menu_geo 1)"
xdotool mousemove $((MX + 30)) $((MY + 1 + 32 + 7 + 48 + 7 + 48 + 7 + 16 + 8)); sleep 0.5
read -r SX SY SW SH <<<"$(menu_geo 2)"
xdotool mousemove $((SX + 20)) $((SY + 1 + 8)) mousemove $((SX + 20)) $((SY + 1 + 32 + 7 + 48 + 8)) click 1; sleep 0.7
check "Double size doubles the main window ($(height))" '[ "$(height)" = 232 ]'
import -window root -crop 600x500+95+95 +repage .cache/menu-3-double.png

# Exit from the main menu (last row).
xdotool mousemove 300 300 click 3; sleep 0.5
read -r MX MY MW MH <<<"$(menu_geo 1)"
xdotool mousemove $((MX + 30)) $((MY + MH - 1 - 16)) mousemove $((MX + 30)) $((MY + MH - 1 - 8)) click 1; sleep 1
kill -0 $APID 2>/dev/null && { echo "FAIL Exit did not quit"; kill $APID; FAIL=1; }
C="$G/config/config.toml"
check "saved: always on top" 'grep -q "always_on_top = true" "$C"'
check "saved: oscilloscope" 'grep -q "visualizer = \"oscilloscope\"" "$C"'
check "saved: double size" 'grep -q "scale = 2" "$C"'
check "saved: theme" 'grep -q "skin = \"nord\"" "$C"'
kill $XPID 2>/dev/null
[[ $FAIL == 0 ]] && echo "menu check passed" || { echo "menu check FAILED"; cat "$G/log" | tail -5; exit 1; }
