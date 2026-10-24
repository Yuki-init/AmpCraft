#!/usr/bin/env bash
# Downloads a prebuilt LGPL FFmpeg for Windows (headers, import libraries and DLLs) into
# .cache/ffmpeg-win64, for cross-checking and cross-building the Windows target. Needs
# network; everything else stays offline. The archive is pinned and checked by SHA-256.
#
# Source: BtbN/FFmpeg-Builds, "lgpl-shared" variant (no --enable-gpl, no nonfree).
# Dated releases are pruned upstream after a while. To move to a newer one, pick a dated
# "autobuild-*" tag, take the win64-lgpl-shared-8.x zip name and hash from its
# checksums.sha256, and update the three values below.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

TAG="autobuild-2026-10-11-13-04"
NAME="ffmpeg-n8.1.3-17-gc389840894-win64-lgpl-shared-8.1"
SHA256="ba651b56d78b2a09a92d3699fd7751254e8b773dc2c3110081b7f9f4ff3b2c48"

dest="$REPO_ROOT/.cache/ffmpeg-win64"
stamp="$dest/.ampcraft-sha256"
url="https://github.com/BtbN/FFmpeg-Builds/releases/download/$TAG/$NAME.zip"
if [[ -f "$stamp" && "$(cat "$stamp")" == "$SHA256" ]]; then
    echo "Windows FFmpeg already in .cache/ffmpeg-win64 ($NAME)"
else
    mkdir -p "$REPO_ROOT/.cache"
    # Download, verify and extract inside the container, into a fresh directory.
    DEV_NETWORK=1 "$REPO_ROOT/scripts/dev.sh" bash -euo pipefail -c '
        url="$1"; name="$2"; sum="$3"
        tmp=/work/.cache/ffmpeg-win64.tmp
        rm -rf "$tmp" /work/.cache/ffmpeg-win64
        mkdir -p "$tmp"
        curl -fL --retry 3 -o "$tmp/$name.zip" "$url"
        echo "$sum  $tmp/$name.zip" | sha256sum -c -
        unzip -q "$tmp/$name.zip" -d "$tmp"
        mv "$tmp/$name" /work/.cache/ffmpeg-win64
        rm -rf "$tmp"
        echo "$sum" > /work/.cache/ffmpeg-win64/.ampcraft-sha256
    ' _ "$url" "$NAME" "$SHA256"
    echo "Windows FFmpeg ready in .cache/ffmpeg-win64 ($NAME)"
fi

# Where the shipped DLLs come from, for the LGPL notice next to them (scripts/build-windows.sh).
# The version name is `git describe` output: the commit hash follows "-g".
commit="${NAME#*-g}"
commit="${commit%%-*}"
cat > "$dest/.ampcraft-source.txt" <<EOF
The FFmpeg DLLs in this folder (avcodec, avformat, avutil, swresample) are FFmpeg, licensed
under the GNU Lesser General Public License version 3 (this build is configured for LGPL
version 3; the full text is in FFMPEG-LICENSE.txt).
They are unmodified, dynamically linked, and can be replaced with any compatible build.

Build:   $NAME (LGPL variant: no --enable-gpl, no --enable-nonfree)
Archive: $url
SHA-256: $SHA256
FFmpeg source: https://git.ffmpeg.org/ffmpeg.git, commit $commit
Build scripts: https://github.com/BtbN/FFmpeg-Builds/tree/$TAG
EOF
