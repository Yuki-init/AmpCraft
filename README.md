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
