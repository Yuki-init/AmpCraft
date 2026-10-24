# Gaps

Status: 2026-10-24. Every known shortfall against the Crafting Apps standards, ranked. Pick from
the top. When one lands, update or delete its entry and add a line to the `ROADMAP.md` log.

Not a gap: decoding through FFmpeg is a deliberate choice (see `docs/architecture.md`).

## G1. Custom window backends instead of egui

`ampcraft-platform` has hand-written X11 (`x11rb`) and Win32 (`windows-sys`) backends, and there
is no macOS backend. The Crafting Apps use egui on winit everywhere, which would also bring
macOS, Wayland and the web.

Plan: an egui/eframe frontend that shows the existing software-rendered windows as textures,
with shaped, borderless, docked windows. Docking math in `ampcraft-dock` stays. The hard part is
several cooperating top-level windows that snap together; check what winit allows on each
platform (Wayland cannot place windows; see the Omarchy fork's Hyprland approach).

## G2. `unsafe` in the Win32 modules

`unsafe_code` is denied workspace-wide, but `crates/platform/src/win32.rs`,
`apps/ampcraft/src/dialog/win32.rs`, the Windows parts of `crates/platform/src/dirs.rs` and
`apps/ampcraft/src/appearance.rs` opt out for Win32 FFI. G1 removes most of it; the rest should
move to safe wrapper crates.

## G3. No command layer, control channel or MCP

Crafting Apps route every user action through a command with an id, label, shortcut and tests,
so the UI, CLI, a JSON control channel and an MCP server all do the same thing. AmpCraft's
actions are `MenuCmd` and `PrefsAction` handlers inside the app. The old headless CLI was
dropped in the fork.

Plan: an `ampcraft-engine` crate with the player session and commands (`playback.play`,
`playlist.add`, `theme.set`...), then a control channel and MCP on top.

## G4. Windows is unfinished

File dialogs, message boxes and media keys are missing on Windows.

## G5. Suite infrastructure not yet adopted

- CI on GitHub Actions (the Crafting Apps use `storytold/craft-common-actions`), SonarCloud and
  CodeRabbit.
- `contributors/` credits and the About window list.
- Shared crates from `storytold/craft-libs` where they fit: `craft-settings` (config storage),
  `craft-atomic` (crash-safe saves), `craft-i18n` (translations), `craft-platform` (desktop
  appearance, which could replace part of `appearance.rs`).
- Packaging for macOS and FreeBSD, and an `omastore.toml`.
