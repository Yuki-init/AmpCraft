# Architecture

Status: 2026-10-24. How AmpCraft is built today, and where it is heading.

## Workspace

```
apps/ampcraft/        the desktop player: arguments, config, menus, preferences, media
                      controls, the System theme (appearance.rs), wiring
crates/core/          playback engine: demux, decode (FFmpeg), resample, queue, seek, gapless
crates/audio-out/     cpal output, lock-free ring buffer, volume and balance
crates/dsp/           10-band equalizer, preamp, spectrum and oscilloscope taps
crates/skin/          .wsz loader, sprite maps, bitmap font, region masks, built-in themes
crates/dock/          panel graph, edge snapping and group drag (pure, no I/O)
crates/ui/            software renderer, widgets, hit testing, window state machines
crates/platform/      window backends behind traits: X11 (x11rb), Win32 (windows-sys)
crates/library/       the library (in memory, saved as library.json), tags, .m3u and .pls playlists
xtask/                cargo xtask ci, layers, assets
```

## Layers

| Layer | Crates |
|---|---|
| L0 | `dock`, `dsp` |
| L1 | `core`, `skin`, `audio-out` |
| L2 | `library`, `ui` |
| L3 | `platform` |
| apps | `ampcraft` |

A crate depends only on lower layers (dev-dependencies excepted). `cargo xtask layers` checks
it against the table in `xtask/src/main.rs`.

## Threads

- **UI thread**: platform events, rendering, menus, and the app state. It polls channels from
  the other threads once per frame.
- **Decoder thread** (`core`): demux and decode into a lock-free ring buffer.
- **Audio callback** (`audio-out`): pulls from the ring buffer, applies EQ, volume and balance,
  taps the visualizer. Never allocates, locks or does I/O.
- **Library scan**: walks the library folders from a snapshot of the library and sends back
  the changes, which the UI thread merges and saves (plays counted meanwhile are kept).

## Library file

The library lives in memory and is saved as `library.json` in the data folder: a format
version, the next track id, and one object per track (path, size, modification time, tags,
added time, play count, last played). Saves write a temporary file and rename it, so a crash
never leaves a half-written library; a play is saved as soon as it is counted. A file that
cannot be read is renamed to `library.json.damaged` and never overwritten. With 50,000 tracks
(a 19 MB file) it opens in about 70 ms, saves in about 50 ms, and a search takes about 1 ms.
- **Media controls** (`media_controls/`): one interface over MPRIS (a D-Bus object server on
  Linux) and the System Media Transport Controls (Windows' media flyout and media keys, tied
  to the main window). Requests reach the UI thread as commands on a channel; the UI thread
  publishes its state each frame and only changes are passed on.
- **Appearance**: watches the desktop's colors (portal signals and `kdeglobals` on Linux, the
  registry on Windows), debounces bursts, and sends new readings to the UI thread.

## Rendering

Skins are sprite sheets. Every window is drawn in software into a pixel buffer at an integer
scale (1x, 2x, 3x) and handed to the platform backend. Built-in themes and the System theme are
generated as sprite sheets in code from a 13-color `Palette`, so they behave exactly like a
loaded `.wsz` skin.

## The System theme

`apps/ampcraft/src/appearance.rs` reads the desktop:

1. KDE Plasma (`XDG_CURRENT_DESKTOP` contains KDE): the color scheme in `kdeglobals`, mapped to
   a palette (window, view, button and selection colors, and the positive, neutral, negative and
   link colors).
2. Otherwise on Linux: `org.freedesktop.appearance` `color-scheme` and `accent-color` from the
   XDG desktop portal, painted onto neutral greys (`Palette::neutral`).
3. Windows: `AppsUseLightTheme` and the DWM `AccentColor` from the registry.

`Theme::system` then moves the display color toward the text color until display text reaches
4.5:1 contrast. No preference at all keeps the dark greys.

## Where it is heading

The Crafting Apps have no `unsafe`, draw their UI with egui, and expose every action as a
command that a CLI, a control channel and MCP can drive. `docs/gaps.md` lists what AmpCraft
still needs for each.

Decoding stays with FFmpeg by choice: it handles every codec and container people actually
have, far beyond what a pure-Rust decoder set would cover. It is linked dynamically (LGPL
builds), so AmpCraft's own code stays MIT OR Apache-2.0.
