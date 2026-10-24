# AGENTS.md: rules for every agent and contributor

AmpCraft is a clean-room, open-source Rust reimplementation of the classic (2.x era) skinnable
music player. It loads classic `.wsz` skins, has built-in themes, and follows the desktop's own
colors. It is built to the conventions of the ArtCraft Crafting Apps (DeckCraft, GridCraft and
their siblings) so that it can be proposed as one of them. It is not an official Crafting App.

These rules bind every AI agent and every human contributor. `CLAUDE.md` holds day-to-day
working notes; if the two disagree, **this file wins**. A maintainer's local `AGENTS.local.md`
(gitignored) may add sandbox rules for their own machine; those add to this file, never relax it.

---

## 1. Asset policy

An **asset** is any non-code material the project includes, ships, embeds, generates for
publication or shows in its docs: icons, images, screenshots, fonts, skins, sounds, sample files
and anything the app renders from those.

### 1.1 Nothing from Winamp, Nullsoft or other vendors

- **Never** commit, bundle or redraw the stock base skin, any third-party skin, or icons,
  images, fonts, sounds or text taken from Winamp, Nullsoft or any other player. `skins/` is
  gitignored for local testing only.
- Do not use the Winamp name or logo in code, assets, package metadata or docs, except to
  describe skin format compatibility.
- Built-in themes are drawn in code (`crates/skin/src/theme.rs`). Only their color values may
  come from published, permissively licensed color schemes, credited in `ATTRIBUTION.md`.
- Screenshots of other players are private observation notes only and are never committed.

### 1.2 Only open, provable licenses

Every asset is open source (MIT, BSD, Apache-2.0, ISC, OFL-1.1), Creative Commons (CC0-1.0,
CC-BY-4.0, CC-BY-SA-4.0), public domain, or contributor-original under `MIT OR Apache-2.0`.
Anything whose license you cannot prove from its original source stays out.

### 1.3 Every asset is attributed

`ATTRIBUTION.md` lists every asset file with author, source, license and use. `NOTICE` keeps
the notices licenses require. `cargo xtask assets` fails when a file under `assets/` or
`docs/images/`, or the app icon, has no row.

---

## 2. Clean-room rules

- Implement from the public skin format docs and black-box observation of the original player.
  Never read, disassemble or copy the leaked or released Winamp source.
- Webamp (MIT) may be read as a reference for skin format quirks, with attribution if anything
  is ported.
- Never copy GPL, LGPL or MPL code. Permissive crates are fine as dependencies (`deny.toml`).
  FFmpeg is the decoder on purpose: it plays every format people actually have. It is linked
  dynamically, LGPL builds only (never `--enable-gpl` or `--enable-nonfree`).

## 3. Never crash

People leave a music player running for hours; a crash loses their playlist and place.
**This outranks feature work.**

- No `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!` or `unimplemented!` in non-test
  code. The workspace lints deny them; `clippy.toml` exempts tests.
- No `unsafe` (`unsafe_code = "deny"`). The Win32 modules are the only opt-out
  (`#[allow(unsafe_code)]` plus a `// SAFETY:` comment on every block) until the egui port
  removes them.
- Errors are `Result<T, E>` with `?`. Data from files, tags, skins and the desktop is hostile:
  `get()` not `[i]`, checked arithmetic, capped allocations, bounded recursion.
- The audio thread never allocates, locks a contended mutex, or does I/O.
- Every crash fix lands with a regression test.

## 4. Architecture

| Layer | Crates |
|---|---|
| L0 | `ampcraft-dock` (panel graph, snapping, group drag; pure), `ampcraft-dsp` (EQ, spectrum) |
| L1 | `ampcraft-core` (playback engine), `ampcraft-skin` (`.wsz` loader, built-in themes), `ampcraft-audio-out` (cpal output) |
| L2 | `ampcraft-library` (library database, playlists), `ampcraft-ui` (software renderer, widgets, window state) |
| L3 | `ampcraft-platform` (window backends: X11, Win32) |
| apps | `ampcraft` (the desktop player) |

A crate depends only on crates in lower layers; `cargo xtask layers` checks it (the table lives
in `xtask/src/main.rs`). Details and the target architecture are in `docs/architecture.md`.

- Keep platform-agnostic logic pure and unit tested (`dock`, `skin`, `ui` state, `dsp`).
- Rendering is software only: skin sprites are blitted into a pixel buffer at integer scales.
- The System theme (`apps/ampcraft/src/appearance.rs`) reads the desktop's colors: the KDE color
  scheme, else the XDG portal's light/dark and accent color, else the Windows registry. It never
  blocks the UI for more than half a second, and it follows changes while running.
- **Never use Tauri.** New UI work should move toward egui (`docs/gaps.md`).
- The app makes no network calls and sends no data anywhere.

## 5. Quality gates (before every commit)

`cargo xtask ci` runs the fmt check, clippy with `-D warnings`, tests (nextest when installed)
and doctests, the layering check and the asset check. Run it in the dev container:

```sh
scripts/check.sh    # cargo xtask ci under Xvfb, plus Windows lint and tests under Wine,
                    # macOS lint and cargo deny
```

Commit after each landed change with a short, plain message (`skin: draw light themes`).

## 6. Building and running

All tooling runs in rootless Podman; nothing is installed on the host.

- `scripts/build-image.sh` builds the dev image, `scripts/fetch.sh` downloads crates (the only
  networked step), `scripts/dev.sh <cmd>` runs a command in the container.
- `scripts/host-run.sh ampcraft [ARGS]` builds in the container and runs the player on the host
  with real audio. Keep host playback short and quiet.
- Headless GUI checks: `scripts/gui-*-check.sh` drive the player under Xvfb with `xdotool`.
- `AMPCRAFT_COLOR_SCHEME=dark|light` pins the System theme for tests.
- **For UI work, look at the result**: render with
  `cargo run -p ampcraft --example theme_gallery -- OUT_DIR 2` or take a screenshot, then read
  the image.

## 7. Planning and progress docs

`plan/` (gitignored) holds local notes: decisions, session logs, observations. The committed
progress docs:

| File | Holds |
|---|---|
| `ROADMAP.md` | Stage, what works, what is next, progress log |
| `docs/gaps.md` | Every known shortfall against the Crafting Apps standards, ranked. Pick from the top |
| `docs/architecture.md` | How AmpCraft is built today, and where it is heading |

When a task lands, update its `gaps.md` entry and add a line to the `ROADMAP.md` progress log.

## Style for prose

Docs, commit messages and comments use plain language and no em dashes.
