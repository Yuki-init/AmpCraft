#!/usr/bin/env bash
# Pre-commit gate: fetch, fmt, clippy, license/ban checks, tests.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
cd "$REPO_ROOT"

echo "==> fetch (network)"
scripts/fetch.sh --quiet

echo "==> cargo xtask ci (fmt, clippy, tests under Xvfb, layering, assets)"
scripts/dev.sh timeout --kill-after=30 2400 dbus-run-session -- \
    xvfb-run --auto-servernum --server-args="-screen 0 1280x1024x24" cargo xtask ci

echo "==> Windows lint"
scripts/check-windows.sh

echo "==> Windows platform tests (Win32 backend under Wine)"
scripts/test-windows.sh -p ampcraft-platform

if [[ -f .cache/ffmpeg-win64/include/libavcodec/avcodec.h ]]; then
    echo "==> Windows dialog tests (under Wine)"
    scripts/test-windows.sh -p ampcraft --lib dialog

    echo "==> Windows build"
    scripts/build-windows.sh --debug >/dev/null
else
    echo "==> Windows build skipped (no .cache/ffmpeg-win64; see scripts/fetch-ffmpeg-windows.sh)"
fi

echo "==> macOS lint (platform crate)"
scripts/dev.sh cargo clippy --quiet -p ampcraft-platform --target aarch64-apple-darwin -- -D warnings

echo "==> cargo deny (licenses, bans, sources)"
scripts/dev.sh cargo deny --offline check licenses bans sources

echo "==> all checks passed"
