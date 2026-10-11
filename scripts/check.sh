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

echo "==> cross-OS lint (platform crate for Windows and macOS)"
scripts/dev.sh cargo clippy --quiet -p amp-platform --target x86_64-pc-windows-gnu --target aarch64-apple-darwin -- -D warnings

echo "==> cargo deny (licenses, bans, sources)"
scripts/dev.sh cargo deny --offline check licenses bans sources

echo "==> tests"
scripts/test.sh

echo "==> all checks passed"
