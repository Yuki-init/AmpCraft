#!/usr/bin/env bash
# Builds a workspace binary in the container and runs it on the host, so it can reach the
# real sound server. The container's FFmpeg (Debian) has different sonames from the host's,
# so the libraries the host lacks are bundled into .cache/host-bin/lib and loaded through
# LD_LIBRARY_PATH. Nothing is installed on the host.
#
# Usage: scripts/host-run.sh [--debug] BIN [ARGS...]
# Builds optimized (release) binaries by default; --debug builds faster but runs far slower.
# Example: scripts/host-run.sh ampcraft --volume 30 song.flac
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

profile=release
build_flags="--release"
case "${1:-}" in
    --debug) profile=debug; build_flags=""; shift ;;
    --release) shift ;;
esac
bin="${1:?usage: scripts/host-run.sh [--debug] BIN [ARGS...]}"
shift

out="$REPO_ROOT/.cache/host-bin"

# Keep app files (config, library database, caches) inside the repo unless asked otherwise,
# so development runs never write to the home directory. HOST_RUN_REAL_DIRS=1 uses the
# normal per-OS locations.
if [[ "${HOST_RUN_REAL_DIRS:-0}" != "1" ]]; then
    export AMPCRAFT_CONFIG_DIR="$REPO_ROOT/.cache/host-home/config"
    export AMPCRAFT_DATA_DIR="$REPO_ROOT/.cache/host-home/data"
    export AMPCRAFT_CACHE_DIR="$REPO_ROOT/.cache/host-home/cache"
fi
mkdir -p "$out"
# Sonames the host already provides; the container copies only the rest.
ldconfig -p | awk 'NR > 1 {print $1}' | sort -u > "$out/host-sonames.txt"

"$REPO_ROOT/scripts/dev.sh" bash -c "
    set -euo pipefail
    cargo build --quiet $build_flags --bin '$bin'
    rm -rf .cache/host-bin/lib && mkdir -p .cache/host-bin/lib
    cp 'target/$profile/$bin' .cache/host-bin/
    ldd 'target/$profile/$bin' | awk '/=>/ && \$3 ~ /^\// {print \$1, \$3}' |
    while read -r name path; do
        case \"\$name\" in libc.so*|libm.so*|ld-linux*|libasound.so*) continue ;; esac
        grep -qxF \"\$name\" .cache/host-bin/host-sonames.txt || cp -L \"\$path\" .cache/host-bin/lib/
    done
" </dev/null

LD_LIBRARY_PATH="$out/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" exec "$out/$bin" "$@"
