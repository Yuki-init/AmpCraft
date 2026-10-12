#!/usr/bin/env bash
# Builds a portable Windows zip: dist/openamp-<version>-windows-x64.zip with the player, the
# CLI, the FFmpeg DLLs and notices, the license, a README and the test kit
# (packaging/windows/). Nothing to install: unzip and run openamp.exe.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
cd "$REPO_ROOT"

version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)
commit=$(git rev-parse --short HEAD)
dirty=$(git status --porcelain --untracked-files=no | grep -q . && echo "+changes" || true)
name="openamp-$version-windows-x64"

scripts/build-windows.sh

stage="dist/$name"
rm -rf "$stage" "dist/$name.zip"
mkdir -p "$stage"
cp dist/windows/openamp.exe dist/windows/openamp-cli.exe dist/windows/*.dll \
    dist/windows/FFMPEG-LICENSE.txt dist/windows/FFMPEG-SOURCE.txt "$stage/"
cp LICENSE "$stage/LICENSE.txt"
cp packaging/windows/README.txt "$stage/"
cp -r packaging/windows/test-kit "$stage/test-kit"
printf 'OpenAMP %s, commit %s%s, built %s\n' "$version" "$commit" "$dirty" "$(date -u +%Y-%m-%dT%H:%MZ)" \
    > "$stage/BUILD.txt"

# Windows tools expect CRLF in text files.
for f in "$stage"/*.txt "$stage"/test-kit/*; do
    sed -i 's/\r*$/\r/' "$f"
done

scripts/dev.sh python3 -m zipfile -c "dist/$name.zip" "$stage"
ls -l "dist/$name.zip"
cat "$stage/BUILD.txt"
