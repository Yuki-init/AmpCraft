# AmpCraft roadmap

**Stage:** early development. The player is usable every day on Linux (X11 and XWayland);
Windows runs under Wine and is being finished. macOS has no window backend yet.

Status: 2026-10-24.

## Where it stands

| Area | State |
|---|---|
| Playback (decode, seek, gapless, queue) | Works, through FFmpeg |
| Main window, playlist, equalizer, docking | Works |
| Skins: `.wsz`, built-in themes, System theme | Works |
| Media library, playlists, MPRIS | Works on Linux |
| Windows | Runs; file dialogs, message boxes and media keys missing |
| macOS | Not started |
| Crafting Apps standards | Layout, licensing, lints and gate done; see `docs/gaps.md` |

## Next

1. Finish Windows: file dialogs, message boxes, media keys (`docs/gaps.md` G4).
2. Move window and UI code onto egui and winit, removing the X11 and Win32 backends and the
   last `unsafe` (G1, G2).
3. Commands for every action, a JSON control channel and an MCP server (G3).
4. Propose AmpCraft to the Crafting Apps through an RFC in `storytold/craft-contrib`.

## Progress log

- 2026-10-24: Forked from OpenAMP 0.1.0-2 as AmpCraft. Relicensed to MIT OR Apache-2.0.
  Crafting Apps layout (`apps/` and `crates/`, `ampcraft-*` crates), `cargo xtask ci` with
  layering and asset checks, never-crash lints, no `unsafe` outside the Win32 modules. Dropped
  the headless CLI (`--rescan` replaces its library scan). New System theme, the default: KDE
  color schemes, portal light/dark and accent color, Windows light/dark and accent, followed
  live.
- 2026-10-24: FFmpeg stays as the decoder by choice; removed from the gaps list.
- 2026-10-24: The library is a JSON file (`library.json`) instead of SQLite: no C dependency,
  atomic saves, plays saved at once, damaged files set aside.
