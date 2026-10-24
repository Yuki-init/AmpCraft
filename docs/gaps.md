# Gaps

Status: 2026-10-24. Every known shortfall against the Crafting Apps standards, ranked. Pick from
the top. When one lands, update or delete its entry and add a line to the `ROADMAP.md` log.

## G1. Decoding depends on FFmpeg

`ampcraft-core` decodes through `ffmpeg-next`, a C library linked dynamically (LGPL builds).
The Crafting Apps are pure Rust, and the Windows package has to ship FFmpeg DLLs.

Plan: pure-Rust demuxers and decoders behind the existing `Decoder` interface. Start with the
common formats (MP3, FLAC, Ogg Vorbis, Opus, AAC in MP4, WAV), keep FFmpeg as an optional
feature until the music library tests pass without it, then remove it. Watch licenses: MPL and
LGPL crates are not allowed (`deny.toml`).

## G2. Custom window backends instead of egui

`ampcraft-platform` has hand-written X11 (`x11rb`) and Win32 (`windows-sys`) backends, and there
is no macOS backend. The Crafting Apps use egui on winit everywhere, which would also bring
macOS, Wayland and the web.

Plan: an egui/eframe frontend that shows the existing software-rendered windows as textures,
with shaped, borderless, docked windows. Docking math in `ampcraft-dock` stays. The hard part is
several cooperating top-level windows that snap together; check what winit allows on each
platform (Wayland cannot place windows; see the Omarchy fork's Hyprland approach).

## G3. `unsafe` in the Win32 modules

`unsafe_code` is denied workspace-wide, but `crates/platform/src/win32.rs`,
`apps/ampcraft/src/dialog/win32.rs`, the Windows parts of `crates/platform/src/dirs.rs` and
`apps/ampcraft/src/appearance.rs` opt out for Win32 FFI. G2 removes most of it; the rest should
move to safe wrapper crates.

## G4. No command layer, control channel or MCP

Crafting Apps route every user action through a command with an id, label, shortcut and tests,
so the UI, CLI, a JSON control channel and an MCP server all do the same thing. AmpCraft's
actions are `MenuCmd` and `PrefsAction` handlers inside the app. The old headless CLI was
dropped in the fork.

Plan: an `ampcraft-engine` crate with the player session and commands (`playback.play`,
`playlist.add`, `theme.set`...), then a control channel and MCP on top.

## G5. Windows is unfinished

File dialogs, message boxes and media keys are missing on Windows.

## G6. Library database bundles SQLite (C)

`ampcraft-library` uses `rusqlite` with the bundled SQLite. Pure-Rust storage would fit the
standards; the schema is small.

## G7. Suite infrastructure not yet adopted

- CI on GitHub Actions (the Crafting Apps use `storytold/craft-common-actions`), SonarCloud and
  CodeRabbit.
- `contributors/` credits and the About window list.
- Shared crates from `storytold/craft-libs` where they fit: `craft-settings` (config storage),
  `craft-atomic` (crash-safe saves), `craft-i18n` (translations), `craft-platform` (desktop
  appearance, which could replace part of `appearance.rs`).
- Packaging for macOS and FreeBSD, and an `omastore.toml`.
