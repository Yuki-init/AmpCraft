#!/bin/bash
# Measures frame costs under Xvfb: plays a track with a large playlist (800 entries) at 2x
# scale with the main, equalizer, playlist and library windows open, and prints the
# OPENAMP_PERF summaries. PROFILE=release (default) or debug.
# Run inside the container: scripts/dev.sh scripts/perf-check.sh
set -u
PROFILE=${PROFILE:-release}
FLAG=""; [[ $PROFILE == release ]] && FLAG="--release"
cargo build -q $FLAG --bin openamp --bin openamp-cli || exit 1
BIN=target/$PROFILE
G=.cache/perf
rm -rf "$G" && mkdir -p "$G/music"
export OPENAMP_CONFIG_DIR="$PWD/$G/config" OPENAMP_DATA_DIR="$PWD/$G/data" OPENAMP_MUSIC_DIR="$PWD/$G/music"
for i in $(seq 1 8); do
  ffmpeg -v error -y -f lavfi -i "sine=f=$((200 + i * 50)):d=120" -ac 2 -metadata artist="Artist $i" -metadata title="ハレ晴レユカイ $i" -c:a flac "$G/music/$i.flac"
done
$BIN/openamp-cli library scan >/dev/null
FILES=()
for i in $(seq 1 100); do for j in $(seq 1 8); do FILES+=("$G/music/$j.flac"); done; done
Xvfb :89 -screen 0 1920x1200x24 -nolisten tcp >/dev/null 2>&1 &
XPID=$!
export DISPLAY=:89
sleep 1
openbox >/dev/null 2>&1 &
sleep 1
OPENAMP_PERF=1 $BIN/openamp --audio null --scale 2 --library --skin dracula "${FILES[@]}" 2>&1 | grep --line-buffered perf &
APP=$!
sleep 16
kill %1 2>/dev/null; sleep 0.5
kill $XPID 2>/dev/null
