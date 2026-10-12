#!/usr/bin/env bash
# Runs tests built for Windows (x86_64-pc-windows-gnu) under Wine with a virtual display, in
# the dev container. Offline. The Wine prefix is kept in .cache/wine-prefix.
# Usage: scripts/test-windows.sh [-p CRATE]... [-- TEST ARGS]   (default: amp-platform)
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
cd "$REPO_ROOT"

args=("$@")
if [[ ${#args[@]} -eq 0 ]]; then
    args=(-p amp-platform)
fi
env_args=()
if [[ -f .cache/ffmpeg-win64/include/libavcodec/avcodec.h ]]; then
    env_args+=(FFMPEG_DIR=/work/.cache/ffmpeg-win64)
fi

timeout 1200 scripts/dev.sh env "${env_args[@]}" bash -euo pipefail -c '
    mkdir -p /tmp/xdg
    export XDG_RUNTIME_DIR=/tmp/xdg LANG=C.UTF-8
    export WINEPREFIX=/work/.cache/wine-prefix WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml="
    # Test binaries that link FFmpeg find its DLLs here.
    if [[ -d .cache/ffmpeg-win64/bin ]]; then export WINEPATH="Z:\\work\\.cache\\ffmpeg-win64\\bin"; fi
    # One display for the whole run; Wine draws its windows there.
    xvfb-run -a -s "-screen 0 1280x1024x24" \
        cargo test --target x86_64-pc-windows-gnu "$@" 2> >(grep -v "XDG_RUNTIME_DIR\|syswow64" >&2)
' _ "${args[@]}"
