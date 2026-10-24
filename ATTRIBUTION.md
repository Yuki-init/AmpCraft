# Attribution

Every asset AmpCraft includes, bundles or shows in its documentation, with its author, source and
license. The policy is in [AGENTS.md](AGENTS.md) §1. `cargo xtask assets` fails if a file under
`assets/` or `docs/images/`, or the app icon, is missing from this table.

AmpCraft contains **no Winamp, Nullsoft or other vendor artwork, skins, fonts or sounds**. Its
built-in themes, their pixel art, the 5x6 font, digits and icons are original work drawn in code
(`crates/skin/src/theme.rs`).

## In this repository

| Asset | Author | Source | License | Used for |
|---|---|---|---|---|
| `packaging/ampcraft.svg` | AmpCraft contributors | Original artwork for this project | MIT OR Apache-2.0 | App icon (desktop entry, packages) |

Not committed: `skins/` is gitignored. A developer's local copy of the stock 2.91 base skin
(`skins/base-2.91.wsz`) has no permissive license and must never be committed or bundled. User
skins (`.wsz`) are loaded at run time and never shipped.

## Reference data credits

Sprite coordinates (`crates/skin/src/sprites.rs`), the TEXT.BMP character map
(`crates/skin/src/font.rs`) and the main-window element positions used by
`crates/skin/examples/preview.rs` follow the classic skin format and were cross-checked against
Webamp (https://github.com/captbaritone/webamp), MIT License, Copyright (c) Jordan Eldredge. No
Webamp code was copied.

## Built-in themes

The built-in themes are drawn entirely in code by this project. Only their color values come
from these published color schemes:

| Theme | Color scheme | License |
|-------|--------------|---------|
| Dracula | Dracula, https://draculatheme.com | MIT |
| Nord | Nord, https://www.nordtheme.com (Arctic Ice Studio) | MIT |
| Gruvbox Dark | gruvbox, https://github.com/morhetz/gruvbox (Pavel Pertsev) | MIT |
| Solarized Dark | Solarized, https://ethanschoonover.com/solarized (Ethan Schoonover) | MIT |
| Catppuccin Mocha | Catppuccin, https://catppuccin.com | MIT |
| Tokyo Night | Tokyo Night, https://github.com/enkia/tokyo-night-vscode-theme (enkia) | MIT |

The System theme uses the colors of the user's own desktop at run time (KDE color scheme, or
the light/dark setting and accent color); its fallback greys are original.
