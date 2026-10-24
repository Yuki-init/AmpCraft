#!/usr/bin/env bash
# Builds a portable Windows zip: dist/ampcraft-<version>-<release>-windows-x64.zip (release is
# the Arch package's pkgrel, so both platforms share one number) with the player, the
# CLI, the FFmpeg DLLs and notices, the license, a README and the test kit
# (packaging/windows/). Nothing to install: unzip and run ampcraft.exe.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
cd "$REPO_ROOT"

version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)
release=$(sed -n 's/^pkgrel=\([0-9]*\)$/\1/p' packaging/arch/PKGBUILD)
commit=$(git rev-parse --short HEAD)
dirty=$(git status --porcelain --untracked-files=no | grep -q . && echo "+changes" || true)
name="ampcraft-$version-$release-windows-x64"

scripts/build-windows.sh

stage="dist/$name"
rm -rf "$stage" "dist/$name.zip"
mkdir -p "$stage"
cp dist/windows/ampcraft.exe dist/windows/*.dll \
    dist/windows/FFMPEG-LICENSE.txt dist/windows/FFMPEG-SOURCE.txt "$stage/"
cp LICENSE-MIT "$stage/LICENSE-MIT.txt"
cp LICENSE-APACHE "$stage/LICENSE-APACHE.txt"
cp NOTICE "$stage/NOTICE.txt"
cp packaging/windows/README.txt "$stage/"
cp -r packaging/windows/test-kit "$stage/test-kit"
printf 'AmpCraft %s-%s, commit %s%s, built %s\n' "$version" "$release" "$commit" "$dirty" "$(date -u +%Y-%m-%dT%H:%MZ)" \
    > "$stage/BUILD.txt"

# Windows tools expect CRLF in text files.
for f in "$stage"/*.txt "$stage"/test-kit/*; do
    sed -i 's/\r*$/\r/' "$f"
done

scripts/dev.sh python3 -m zipfile -c "dist/$name.zip" "$stage"
ls -l "dist/$name.zip"
cat "$stage/BUILD.txt"
