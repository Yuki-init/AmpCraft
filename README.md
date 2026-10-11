# OpenAMP (working name)

A cross-platform reimplementation of the classic 2.x era skinnable audio player, written in Rust with FFmpeg as the decoding backend. It loads classic `.wsz` skins. Linux on X11 / XWayland is the first target.

Status: early scaffolding. See `AGENTS.md` for the roadmap and rules, `docs/DECISIONS.md` for design choices and `docs/OVERNIGHT.md` for the work log.

## Building

Everything builds and runs inside rootless Podman.

```sh
scripts/build-image.sh   # once, or after changing the Containerfile or toolchain
scripts/fetch.sh         # download crates (the only networked step)
scripts/dev.sh cargo build
scripts/check.sh         # fmt, clippy, cargo deny, tests
```

## Headless CLI

`openamp-cli` drives the playback engine without the GUI:

```sh
scripts/dev.sh cargo run --bin openamp-cli -- probe FILE...
scripts/dev.sh cargo run --bin openamp-cli -- decode FILE -o out.wav [--rate 48000] [--seek 30] [--length 10]
scripts/dev.sh cargo run --bin openamp-cli -- play FILE... [--sink device|null|fast] [--shuffle] [--repeat all]
```

While playing, type `p` (pause), `n` (next), `b` (previous), `s SECS` (seek), `v 0-100` (volume) or `q` (quit) and press Enter. Inside the container the audio device is ALSA's null plugin, which is not paced in real time; use `--sink null` for real-time pacing. Set `OPENAMP_LOG=debug` for engine logs.

## Themes

Six built-in themes are generated in code from popular color schemes: `dracula`, `nord`, `gruvbox`, `solarized`, `catppuccin` and `tokyonight`. Pick one with `--skin nord` (it is remembered), or press Alt+S in the player to cycle through them. Classic `.wsz` skins still work with `--skin PATH`. `openamp-cli skin list` lists the themes and `openamp-cli skin export nord nord.wsz` writes one as a skin file other players can load. `cargo run -p amp-app --example theme_gallery -- OUT_DIR` renders all of them for a look.

## Running on the host with real audio

The container cannot reach the sound server, so `scripts/host-run.sh` builds an optimized binary in the container (`--debug` for a debug build) and runs it on the host, bundling the FFmpeg libraries the host lacks under `.cache/host-bin/`:

```sh
scripts/host-run.sh openamp-cli devices
scripts/host-run.sh openamp-cli play --volume 30 FILE...
```

If ALSA's default device is unusable (common when PipeWire holds the card and `pipewire-alsa` is not installed), the CLI falls back to the `pipewire` or `pulse` ALSA device. Pass `--device NAME` to choose one.

## Music library tests

When `/path/to/music` is mounted, the scripts mount it read-only at `/music` and an opt-in test decodes a sample of it, printing aggregate results only. `WINAMP_RS_MUSIC_SAMPLE` sets the sample size (default 60).
