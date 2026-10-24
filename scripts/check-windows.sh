#!/usr/bin/env bash
# Lints the Windows target (x86_64-pc-windows-gnu, MinGW) in the dev container.
# With .cache/ffmpeg-win64 (scripts/fetch-ffmpeg-windows.sh) the whole workspace is checked;
# without it, only the crates that do not use FFmpeg.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
cd "$REPO_ROOT"

target=x86_64-pc-windows-gnu
if [[ -f .cache/ffmpeg-win64/include/libavcodec/avcodec.h ]]; then
    scripts/dev.sh env FFMPEG_DIR=/work/.cache/ffmpeg-win64 \
        cargo clippy --quiet --workspace --all-targets --target "$target" -- -D warnings
else
    echo "note: no Windows FFmpeg in .cache/ffmpeg-win64 (scripts/fetch-ffmpeg-windows.sh);" \
        "checking only the crates without FFmpeg" >&2
    scripts/dev.sh cargo clippy --quiet --all-targets --target "$target" \
        -p ampcraft-platform -p ampcraft-dock -p ampcraft-skin -p ampcraft-ui -p ampcraft-dsp -p ampcraft-audio-out \
        -- -D warnings
fi
