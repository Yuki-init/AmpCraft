# OpenAMP (working name)

A cross-platform reimplementation of the classic 2.x era skinnable audio player, written in Rust with FFmpeg as the decoding backend. It loads classic `.wsz` skins. Linux on X11 / XWayland is the first target.

Status: playable on Linux (X11 and XWayland): main window, playlist, equalizer with presets, media library, preferences, built-in themes, MPRIS and media keys. Windows and macOS backends are not written yet. Design choices are recorded in `docs/DECISIONS.md` and the development log in `docs/OVERNIGHT.md`.

Builds and tests run in a Podman container (see Building); nothing needs installing on the host except Podman.

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

## Menus and keys

The buttons on the left of the main window: **O** opens the preferences (also Ctrl+P), **A** keeps the
player above other windows, **I** shows the current track's file info, **D** toggles double
size and **V** opens the visualizer menu. Right-click the main window for the main menu (or
the visualizer for its menu).

The preferences window has four pages: General (always on top, size, time display),
Playback (repeat, shuffle, visualizer, output device), Skins (built-in themes, loading a
`.wsz`) and Media Library (folders, rescan). Changes apply at once and are saved, except the
output device, which is used from the next start.

In the playlist, drag selected tracks to move them, right-click tracks for Play, Queue,
Remove, Crop, File info, Select and Sort, and press Q to queue the selected tracks to play
next (they show [1], [2], ... and play before the normal order).

The equalizer's PRESETS button loads one of 18 built-in presets (Classical, Club, Dance,
Flat, Full Bass, Full Bass & Treble, Full Treble, Headphones, Large Hall, Live, Party, Pop,
Reggae, Rock, Ska, Soft, Soft Rock, Techno), saves and deletes your own (kept in
`eq-presets.toml` in the config folder), saves an auto-load preset for the current track
(applied whenever that file plays while AUTO is on), and imports or exports Winamp `.eqf`
files. Shaded, the equalizer shows volume and balance sliders.

The playlist's bottom buttons open their classic menus (click to open, then click an item,
or press and drag to an item): **ADD** URL, folder or files; **REM** all, crop to the
selection, selected, or misc (missing files, duplicates); **SEL** invert, none, all; **MISC**
sort (title, file name, path, reverse, randomize), file info, options (reread titles); **LIST**
new, save (.m3u8) and load (.m3u, .m3u8, .pls).

The media library (Alt+L) follows the 5.x layout: a Local Media tree (Audio, Most Played,
Recently Added, Recently Played, Never Played), a search bar, Artist and Album filter panes
(double-click one to play it) and a sortable track list. The Library button adds folders and
rescans. Play counts are kept in the library database (in the data folder, never on the music
drive).

| Key | Does |
| --- | --- |
| Z X C V B | Previous, play, pause, stop, next |
| Left / Right | Back / forward 5 seconds |
| Up / Down | Volume |
| L | Play files |
| R / S | Repeat / shuffle |
| Ctrl+A | Always on top |
| Ctrl+D | Double size |
| Ctrl+T | Time elapsed / remaining |
| Alt+E / Alt+G / Alt+L | Playlist / equalizer / media library |
| Alt+S | Next built-in skin |
| Alt+3 | File info |
| Q (playlist) | Queue or unqueue the selected tracks |
| Ctrl+P | Preferences |

## Arch Linux package

`scripts/build-arch-package.sh` builds a release package in an Arch container, linked against
Arch's own FFmpeg, and writes it to `dist/`. Install it with:

```
sudo pacman -U dist/openamp-0.1.0-1-x86_64.pkg.tar.zst
```

This installs `openamp` and `openamp-cli` to `/usr/bin`, a desktop entry and an icon. Remove it
with `sudo pacman -R openamp`. Config lives in `~/.config/openamp`, the library database in
`~/.local/share/openamp`. Without a `--skin`, the first built-in theme is used (Alt+S cycles).
`packaging/arch/PKGBUILD` also works with plain `makepkg -si` on an Arch machine.

## Running on the host with real audio

The container cannot reach the sound server, so `scripts/host-run.sh` builds an optimized binary in the container (`--debug` for a debug build) and runs it on the host, bundling the FFmpeg libraries the host lacks under `.cache/host-bin/`:

```sh
scripts/host-run.sh openamp-cli devices
scripts/host-run.sh openamp-cli play --volume 30 FILE...
```

If ALSA's default device is unusable (common when PipeWire holds the card and `pipewire-alsa` is not installed), the CLI falls back to the `pipewire` or `pulse` ALSA device. Pass `--device NAME` to choose one.

## Music library tests

Set `OPENAMP_MUSIC_HOST_DIR` to a music folder (in the environment, or in `scripts/local.env`, which is not committed) and the scripts mount it read-only at `/music`; an opt-in test then decodes a sample of it, printing aggregate results only. Without it, those tests are skipped. `WINAMP_RS_MUSIC_SAMPLE` sets the sample size (default 60).

## License

GPL-3.0-or-later; see `LICENSE`. FFmpeg is linked dynamically (LGPL builds). Third-party
material and credits are listed in `assets/LICENSES.md`.
