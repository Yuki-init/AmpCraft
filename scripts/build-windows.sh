#!/usr/bin/env bash
# Cross-builds the Windows binaries (x86_64-pc-windows-gnu, MinGW) in the dev container and
# lays them out as shipped: dist/windows/ holds the exes with the FFmpeg DLLs they load next
# to them (Windows has no system FFmpeg), plus FFmpeg's license and where its source is.
# Usage: scripts/build-windows.sh [--debug]
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
cd "$REPO_ROOT"

profile=release
cargo_profile=(--release)
if [[ "${1:-}" == "--debug" ]]; then
    profile=debug
    cargo_profile=()
fi

scripts/fetch-ffmpeg-windows.sh

target=x86_64-pc-windows-gnu
scripts/dev.sh env FFMPEG_DIR=/work/.cache/ffmpeg-win64 \
    cargo build "${cargo_profile[@]}" --target "$target" -p amp-app

out=dist/windows
rm -rf "$out"
mkdir -p "$out"
# target/ is a named volume, so copy the results out from inside the container.
scripts/dev.sh bash -euo pipefail -c '
    cp "target/$1/$2/"openamp*.exe "$3/"
    # Only the libraries the player links (they depend on nothing else outside Windows).
    for lib in avcodec-62 avformat-62 avutil-60 swresample-6; do
        cp ".cache/ffmpeg-win64/bin/$lib.dll" "$3/"
    done
    cp .cache/ffmpeg-win64/LICENSE.txt "$3/FFMPEG-LICENSE.txt"
    cp .cache/ffmpeg-win64/.openamp-source.txt "$3/FFMPEG-SOURCE.txt"
' _ "$target" "$profile" "$out"
ls -l "$out"
