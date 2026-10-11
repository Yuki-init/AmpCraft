#!/bin/bash
# Screenshots every window with each skin: main, equalizer, playlist, media library and
# preferences, then the shaded bars. Skins come from the arguments, else skins/test/*.wsz
# (the skins/ folder is gitignored; third-party skins are never committed).
# Output: .cache/skin-gallery/<skin>-windows.png and <skin>-shaded.png, plus any loader
# warnings in <skin>.log.
# Run inside the container: scripts/dev.sh scripts/skin-gallery.sh [SKIN.wsz...]
set -u
cargo build -q --bin openamp || exit 1
D=.cache/skin-gallery
mkdir -p "$D"
SKINS=("$@")
[[ ${#SKINS[@]} -eq 0 ]] && SKINS=(skins/test/*.wsz)
Xvfb :102 -screen 0 1400x900x24 -nolisten tcp >/dev/null 2>&1 &
XP=$!
export DISPLAY=:102
sleep 1
openbox >/dev/null 2>&1 &
sleep 1
for t in A B C; do
    [[ -f "$D/$t.flac" ]] || ffmpeg -v error -y -f lavfi -i "sine=d=30" -ac 2 \
        -metadata artist="Artist $t" -metadata title="Title $t" -c:a flac "$D/$t.flac"
done
for s in "${SKINS[@]}"; do
    [[ -f "$s" ]] || { echo "no such skin: $s"; continue; }
    n=$(basename "$s" .wsz)
    rm -rf "$D/home"
    export OPENAMP_CONFIG_DIR="$PWD/$D/home/config" OPENAMP_DATA_DIR="$PWD/$D/home/data" OPENAMP_LOG=warn
    target/debug/openamp --audio null --skin "$s" --scale 1 --library "$D"/{A,B,C}.flac >"$D/$n.log" 2>&1 &
    P=$!
    sleep 3
    xdotool mousemove 113 128 click 1; sleep 1                       # O: preferences
    import -window root -crop 905x505+95+95 +repage "$D/$n-windows.png"
    xdotool key Escape; sleep 0.3
    xdotool mousemove 358 107 click 1; sleep 0.4                     # shade main
    xdotool mousemove 354 121 click 1; sleep 0.4                     # shade equalizer
    xdotool mousemove 358 135 click 1; sleep 0.4                     # shade playlist
    import -window root -crop 290x50+95+95 +repage "$D/$n-shaded.png"
    kill $P; wait $P 2>/dev/null
    echo "$n: $(grep -c . "$D/$n.log") warning(s)"
done
kill $XP 2>/dev/null
exit 0
