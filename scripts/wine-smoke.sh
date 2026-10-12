#!/usr/bin/env bash
# Smoke-tests the Windows build of openamp-cli under Wine in the dev container: probe, decode
# (checked for length and level), seek, play to a fast null sink, built-in themes, paths and
# the library, on small generated fixtures (one with a non-ASCII name). Offline.
# Builds dist/windows first unless --no-build is given.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
cd "$REPO_ROOT"

if [[ "${1:-}" != "--no-build" ]]; then
    scripts/build-windows.sh
fi
if [[ ! -f dist/windows/openamp-cli.exe ]]; then
    echo "dist/windows/openamp-cli.exe missing; run scripts/build-windows.sh" >&2
    exit 1
fi

# The test body runs in the container.
timeout 600 scripts/dev.sh bash -euo pipefail -c '
    work=.cache/wine-smoke
    rm -rf "$work"
    mkdir -p "$work/music" "$work/out" "$work/no-music" /tmp/xdg
    export XDG_RUNTIME_DIR=/tmp/xdg
    # Wine converts arguments and file names with the Unix locale; without UTF-8 they mangle.
    export LANG=C.UTF-8
    # A prefix of its own under .cache, kept between runs (first start takes a few seconds).
    export WINEPREFIX=/work/.cache/wine-prefix WINEARCH=win64 WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml="
    # Set the prefix up completely first (see scripts/test-windows.sh).
    if [[ ! -f "$WINEPREFIX/system.reg" ]]; then wineboot -i >/dev/null 2>&1; wineserver -w; fi
    # Wine sees / as drive Z:, so the app gets Windows paths for its folders.
    winpath() { printf "Z:%s" "$(realpath -m "$1" | tr / "\\\\")"; }
    export OPENAMP_CONFIG_DIR="$(winpath "$work/config")"
    export OPENAMP_DATA_DIR="$(winpath "$work/data")"
    export OPENAMP_CACHE_DIR="$(winpath "$work/cache")"
    # An empty folder in place of the Music known folder, so nothing else is scanned.
    export OPENAMP_MUSIC_DIR="$(winpath "$work/no-music")"
    cli() { wine dist/windows/openamp-cli.exe "$@" 2> >(grep -v "XDG_RUNTIME_DIR\|syswow64" >&2); }

    pass=0
    ok() { pass=$((pass + 1)); echo "  ok   $*"; }
    fail() { echo "  FAIL $*" >&2; exit 1; }
    near() { awk -v a="$1" -v b="$2" -v t="$3" "BEGIN { d = a - b; exit !(d <= t && d >= -t) }"; }

    echo "==> fixtures (3 s, 1 kHz sine, -18 dBFS peak)"
    ffmpeg -loglevel error -f lavfi -i "sine=frequency=1000:duration=3:sample_rate=44100" \
        -metadata title="Smoke Tone flac" -metadata artist="OpenAMP Smoke" -metadata album="Wine" \
        "$work/music/tone.flac"
    ffmpeg -loglevel error -i "$work/music/tone.flac" -metadata title="Smoke Tone mp3" -c:a libmp3lame -q:a 4 "$work/music/tone.mp3"
    ffmpeg -loglevel error -i "$work/music/tone.flac" -metadata title="Smoke Tone ogg" -c:a libvorbis "$work/music/tone.ogg"
    ffmpeg -loglevel error -i "$work/music/tone.flac" -metadata title="Smoke Tone opus" -c:a libopus "$work/music/tone.opus"
    ffmpeg -loglevel error -i "$work/music/tone.flac" -metadata title="Smoke Tone m4a" -c:a aac "$work/music/tone.m4a"
    cp "$work/music/tone.flac" "$work/music/テスト ü.flac"
    files=(tone.flac tone.mp3 tone.ogg tone.opus tone.m4a "テスト ü.flac")

    echo "==> openamp-cli.exe under $(wine --version)"
    cli --version >/dev/null && ok "--version"
    cli paths > "$work/out/paths.txt"
    grep -q "^config  Z:" "$work/out/paths.txt" && ok "paths honor OPENAMP_CONFIG_DIR" \
        || fail "paths: $(cat "$work/out/paths.txt")"

    for f in "${files[@]}"; do
        out=$(cli probe "$work/music/$f") || fail "probe $f"
        grep -q "Smoke Tone" <<<"$out" || fail "probe $f: no title in: $out"
        ok "probe $f"
    done

    # Decode every format and check length and level with the Linux FFmpeg.
    for f in "${files[@]}"; do
        wav="$work/out/$f.wav"
        # Lossy encoders overshoot a pure sine by up to about 1.5 dB (the Linux build reads
        # the same files identically); FLAC must match exactly.
        tol=2.0
        [[ "$f" == *.flac ]] && tol=0.1
        cli decode "$work/music/$f" -o "$wav" >/dev/null 2>&1 || fail "decode $f"
        dur=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$wav")
        near "$dur" 3 0.08 || fail "decode $f: duration $dur, expected 3"
        peak=$(ffmpeg -nostats -i "$wav" -af astats=measure_overall=Peak_level:measure_perchannel=none -f null - 2>&1 \
            | sed -n "s/.*Peak level dB: *//p" | tail -1)
        near "$peak" -18.06 "$tol" || fail "decode $f: peak $peak dB, expected -18"
        ok "decode $f (${dur} s, peak ${peak} dB)"
    done

    cli decode "$work/music/tone.flac" -o "$work/out/seek.wav" --seek 1 --length 1 --rate 48000 >/dev/null 2>&1
    dur=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$work/out/seek.wav")
    rate=$(ffprobe -v error -show_entries stream=sample_rate -of csv=p=0 "$work/out/seek.wav")
    near "$dur" 1 0.03 && [[ "$rate" == 48000 ]] && ok "decode --seek 1 --length 1 --rate 48000" \
        || fail "seek: duration $dur, rate $rate"

    timeout 60 wine dist/windows/openamp-cli.exe play --sink fast "$work/music/tone.flac" "$work/music/tone.mp3" "$work/music/テスト ü.flac" </dev/null >/dev/null 2>&1 \
        && ok "play --sink fast (3 tracks, gapless queue)" || fail "play"

    cli skin list | grep -q Nord && ok "skin list" || fail "skin list"
    cli skin export Nord "$work/out/nord.wsz" >/dev/null && unzip -tq "$work/out/nord.wsz" >/dev/null \
        && ok "skin export (valid zip)" || fail "skin export"

    cli library add "$(winpath "$work/music")" >/dev/null || fail "library add"
    cli library scan >/dev/null || fail "library scan"
    stats=$(cli library stats)
    grep -q "^6 tracks, 1 artists, 1 albums" <<<"$stats" || fail "library stats: $stats"
    cli library search "テスト" | grep -q "Smoke Tone" && ok "library add, scan, search (non-ASCII)" \
        || fail "library search"

    # Wine in the container has no audio endpoint service, so device queries fail. They must
    # fail as errors (exit 1), not as a cpal panic (exit 101).
    for args in "devices" "play --volume 0 $work/music/tone.flac"; do
        set +e
        out=$(timeout 60 wine dist/windows/openamp-cli.exe $args </dev/null 2>&1)
        rc=$?
        set -e
        if [[ $rc == 0 ]] || { [[ $rc == 1 ]] && grep -q "audio service is unavailable" <<<"$out"; }; then
            ok "${args%% *} with no audio service: exit $rc, no crash"
        else
            fail "${args%% *}: exit $rc: $(tail -3 <<<"$out")"
        fi
    done
    echo "==> $pass checks passed"
'
