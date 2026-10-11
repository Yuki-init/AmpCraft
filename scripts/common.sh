#!/usr/bin/env bash
# Shared settings for the container wrapper scripts. Source, do not execute.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${OPENAMP_IMAGE:-localhost/openamp-dev:latest}"
# Personal settings (not committed): scripts/local.env, for example
#   OPENAMP_MUSIC_HOST_DIR=/path/to/music
if [[ -f "$REPO_ROOT/scripts/local.env" ]]; then
    # shellcheck source=/dev/null
    source "$REPO_ROOT/scripts/local.env"
fi
# A music folder to mount read-only for the opt-in library tests. Unset: none.
MUSIC_HOST_DIR="${OPENAMP_MUSIC_HOST_DIR:-}"

# Reads the pinned toolchain version from rust-toolchain.toml.
rust_version() {
    sed -n 's/^channel *= *"\(.*\)"/\1/p' "$REPO_ROOT/rust-toolchain.toml"
}

# Builds the podman run argument list into the RUN_ARGS array.
# Usage: podman_args <network: none|default>
podman_args() {
    local network="$1"
    RUN_ARGS=(
        --rm
        --userns=keep-id
        --network="$network"
        --memory=8g
        --cpus=4
        --security-opt=no-new-privileges
        -v "$REPO_ROOT":/work:Z
        -w /work
        # :U chowns the named volumes to the container user so cargo can write them.
        -v winamp-rs-cargo:/usr/local/cargo/registry:U
        -v winamp-rs-target:/work/target:U
        -e CARGO_TARGET_DIR=/work/target
        -e CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-auto}"
        -e RUST_BACKTRACE=1
        -e HOME=/tmp
    )
    if [[ "$network" == "none" ]]; then
        RUN_ARGS+=(-e CARGO_NET_OFFLINE=true)
    fi
    # The music drive is optional and always read-only. No relabel option on purpose:
    # relabeling would write to the drive.
    if [[ -n "$MUSIC_HOST_DIR" && -d "$MUSIC_HOST_DIR" ]]; then
        RUN_ARGS+=(-v "$MUSIC_HOST_DIR":/music:ro -e WINAMP_RS_MUSIC_DIR=/music)
    fi
    if [[ -t 0 && -t 1 ]]; then
        RUN_ARGS+=(-it)
    fi
}
