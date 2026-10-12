#!/usr/bin/env bash
# Pre-commit gate: fetch, fmt, clippy, license/ban checks, tests.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
cd "$REPO_ROOT"

echo "==> fetch (network)"
scripts/fetch.sh --quiet

echo "==> fmt"
scripts/dev.sh cargo fmt --all -- --check

echo "==> clippy"
scripts/dev.sh cargo clippy --workspace --all-targets -- -D warnings

echo "==> Windows lint"
scripts/check-windows.sh

echo "==> Windows platform tests (Win32 backend under Wine)"
scripts/test-windows.sh -p amp-platform

if [[ -f .cache/ffmpeg-win64/include/libavcodec/avcodec.h ]]; then
    echo "==> Windows smoke test (openamp-cli.exe under Wine)"
    scripts/build-windows.sh --debug >/dev/null
    scripts/wine-smoke.sh --no-build
else
    echo "==> Windows smoke test skipped (no .cache/ffmpeg-win64; see scripts/fetch-ffmpeg-windows.sh)"
fi

echo "==> macOS lint (platform crate)"
scripts/dev.sh cargo clippy --quiet -p amp-platform --target aarch64-apple-darwin -- -D warnings

echo "==> cargo deny (licenses, bans, sources)"
scripts/dev.sh cargo deny --offline check licenses bans sources

echo "==> tests"
scripts/test.sh

echo "==> all checks passed"
