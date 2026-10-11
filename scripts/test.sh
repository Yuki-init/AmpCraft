#!/usr/bin/env bash
# Runs the full test suite offline under Xvfb. Extra args go to cargo nextest.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
podman_args none
exec podman run "${RUN_ARGS[@]}" "$IMAGE" \
    timeout --kill-after=30 1800 \
    xvfb-run --auto-servernum --server-args="-screen 0 1280x1024x24" \
    cargo nextest run --workspace --all-targets --no-tests=pass "$@"
