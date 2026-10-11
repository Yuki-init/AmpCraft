#!/usr/bin/env bash
# Builds the Arch Linux package in an Arch container (against Arch's FFmpeg) and writes it
# to dist/. Network is used only for the image build and cargo fetch.
# Install on the host with: sudo pacman -U dist/openamp-*.pkg.tar.zst
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
ARCH_IMAGE="${OPENAMP_ARCH_IMAGE:-localhost/openamp-arch:latest}"

if [[ "${REBUILD_IMAGE:-0}" == "1" ]] || ! podman image exists "$ARCH_IMAGE"; then
    podman build --build-arg RUST_VERSION="$(rust_version)" -t "$ARCH_IMAGE" \
        -f "$REPO_ROOT/containers/arch.Containerfile" "$REPO_ROOT/containers"
fi

mkdir -p "$REPO_ROOT/dist"
run() {
    local network="$1"; shift
    podman run --rm --userns=keep-id --network="$network" --memory=8g --cpus=4 \
        --security-opt=no-new-privileges \
        -v "$REPO_ROOT":/work:Z -w /work/packaging/arch \
        -v openamp-arch-cargo:/usr/local/cargo/registry:U \
        -e HOME=/tmp -e RUSTUP_TOOLCHAIN="$(rust_version)" -e PKGDEST=/work/dist -e BUILDDIR=/work/packaging/arch \
        "$ARCH_IMAGE" "$@"
}
# Fetch with network, then build offline.
run default bash -c 'cd /work && cargo fetch --locked --quiet'
run none bash -c 'CARGO_NET_OFFLINE=true makepkg --force --nodeps --noprepare 2>&1 | tail -15'
ls -la "$REPO_ROOT"/dist/*.pkg.tar.zst
