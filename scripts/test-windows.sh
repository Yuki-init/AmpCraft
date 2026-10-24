#!/usr/bin/env bash
# Runs tests built for Windows (x86_64-pc-windows-gnu) under Wine with a virtual display, in
# the dev container. Offline. The Wine prefix is kept in .cache/wine-prefix.
# Usage: scripts/test-windows.sh [-p CRATE]... [-- TEST ARGS]   (default: ampcraft-platform)
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
cd "$REPO_ROOT"

args=("$@")
if [[ ${#args[@]} -eq 0 ]]; then
    args=(-p ampcraft-platform)
fi
env_args=()
if [[ -f .cache/ffmpeg-win64/include/libavcodec/avcodec.h ]]; then
    env_args+=(FFMPEG_DIR=/work/.cache/ffmpeg-win64)
fi

timeout 1200 scripts/dev.sh env "${env_args[@]}" bash -euo pipefail -c '
    mkdir -p /tmp/xdg
    export XDG_RUNTIME_DIR=/tmp/xdg LANG=C.UTF-8
    export WINEPREFIX=/work/.cache/wine-prefix WINEARCH=win64 WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml="
    # Set the prefix up completely first: implicit setup finishes in the background and, cut
    # short, leaves COM classes (file dialogs, audio devices) unregistered.
    if [[ ! -f "$WINEPREFIX/system.reg" ]]; then wineboot -i >/dev/null 2>&1; wineserver -w; fi
    # Test binaries that link FFmpeg find its DLLs here.
    if [[ -d .cache/ffmpeg-win64/bin ]]; then export WINEPATH="Z:\\work\\.cache\\ffmpeg-win64\\bin"; fi
    # One display for the whole run; Wine draws its windows there. Started by hand rather
    # than with xvfb-run, whose startup handshake sometimes hung under rootless Podman.
    Xvfb :77 -screen 0 1280x1024x24 -nolisten tcp >/dev/null 2>&1 &
    xvfb=$!
    export DISPLAY=:77
    for _ in $(seq 100); do xdpyinfo >/dev/null 2>&1 && break; sleep 0.1; done
    status=0
    cargo test --target x86_64-pc-windows-gnu "$@" || status=$?
    wineserver -k 2>/dev/null || true
    kill "$xvfb" 2>/dev/null || true
    exit "$status"
' _ "${args[@]}"
