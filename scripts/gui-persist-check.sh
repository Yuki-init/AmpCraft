#!/bin/bash
# Checks that settings, window layout and the playlist survive a restart: changes state in a
# first run, closes, inspects the saved files, then starts again without arguments.
# Run inside the container: scripts/dev.sh scripts/gui-persist-check.sh
set -u
SKIN=${SKIN:-skins/base-2.91.wsz}
cargo build -q --bin openamp || exit 1
G=.cache/gui-persist
rm -rf "$G" && mkdir -p "$G/music"
export OPENAMP_CONFIG_DIR="$PWD/$G/config" OPENAMP_DATA_DIR="$PWD/$G/data" OPENAMP_MUSIC_DIR="$PWD/$G/music"
ffmpeg -v error -y -f lavfi -i "sine=f=440:d=60" -ac 2 -metadata artist=A -metadata title="First" -c:a flac "$G/music/1.flac"
ffmpeg -v error -y -f lavfi -i "sine=f=550:d=60" -ac 2 -metadata artist=B -metadata title="Second" -c:a flac "$G/music/2.flac"
Xvfb :85 -screen 0 1280x1024x24 -nolisten tcp >/dev/null 2>&1 &
XPID=$!
export DISPLAY=:85
sleep 1
openbox >/dev/null 2>&1 &
sleep 1
FAIL=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; FAIL=1; fi; }

# --- first run: change things, then close
target/debug/openamp --audio null --skin "$SKIN" --volume 80 "$G/music/1.flac" "$G/music/2.flac" >.cache/persist-1.log 2>&1 &
APID=$!
sleep 2
xdotool mousemove 150 105 mousedown 1; sleep 0.1; xdotool mousemove 350 205; sleep 0.1; xdotool mousemove 400 255; sleep 0.2; xdotool mouseup 1; sleep 0.3
read MX MY < <(xwininfo -name OpenAMP | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} END{print x, y}')
echo "main moved to $MX,$MY"
# Wheel down twice (volume -10%), shuffle on, EQ off, remaining time on, next track, close.
xdotool mousemove $((MX + 150)) $((MY + 50)) click 5 click 5; sleep 0.2
xdotool mousemove $((MX + 187)) $((MY + 96)) click 1; sleep 0.2       # shuffle
xdotool mousemove $((MX + 230)) $((MY + 63)) click 1; sleep 0.3       # EQ button: hide equalizer
xdotool mousemove $((MX + 70)) $((MY + 32)) click 1; sleep 0.2        # time: remaining
xdotool mousemove $((MX + 119)) $((MY + 97)) click 1; sleep 0.5       # next track
xdotool mousemove $((MX + 268)) $((MY + 7)) click 1; sleep 1          # close
kill -0 $APID 2>/dev/null && { echo "FAIL first run did not exit"; kill $APID; FAIL=1; }
echo "--- config.toml"; cat "$G/config/config.toml"
echo "--- playlist.m3u8"; cat "$G/data/playlist.m3u8"
check "volume saved" 'grep -q "^volume = 0.7" "$G/config/config.toml"'
check "shuffle saved" 'grep -q "^shuffle = true" "$G/config/config.toml"'
check "remaining saved" 'grep -q "^show_remaining = true" "$G/config/config.toml"'
check "main position saved" 'grep -A2 "\[windows.main\]" "$G/config/config.toml" | grep -q "x = $MX"'
check "equalizer hidden saved" 'grep -A3 "\[windows.equalizer\]" "$G/config/config.toml" | grep -q "visible = false"'
check "current track saved" 'grep -q "^current_track = 1" "$G/config/config.toml"'
check "playlist saved" '[ "$(grep -c "\.flac$" "$G/data/playlist.m3u8")" = 2 ]'

# --- second run: no arguments
target/debug/openamp --audio null >.cache/persist-2.log 2>&1 &
APID=$!
sleep 2.5
import -window root -crop 300x420+$((MX - 10))+$((MY - 10)) +repage .cache/persist-restored.png
pos=$(xwininfo -name OpenAMP | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} END{print x","y}')
check "main window restored at $MX,$MY (got $pos)" '[ "$pos" = "$MX,$MY" ]'
check "title shows the saved track" 'convert .cache/persist-restored.png -crop 150x8+120+35 +repage txt:- | grep -qv "#000000" '
check "equalizer stays hidden" '! xwininfo -name Equalizer | grep -q "IsViewable"'
xdotool mousemove $((MX + 268)) $((MY + 7)) click 1; sleep 1
kill $APID $XPID 2>/dev/null
[[ $FAIL == 0 ]] && echo "persistence check passed" || { echo "persistence check FAILED"; exit 1; }
