# Bundled asset licenses

Every skin, font, image or audio file committed to this repository must be listed here with its source and a permissive license.

| File | Source | License |
|------|--------|---------|
| (none yet) | | |

Not committed: `skins/` is gitignored. The owner's local copy of the stock base skin (`skins/base-2.91.wsz`) has no permissive license and must not be committed or bundled.

## Reference data credits

Sprite coordinates (`crates/skin/src/sprites.rs`), the TEXT.BMP character map (`crates/skin/src/font.rs`) and the main-window element positions used by `crates/skin/examples/preview.rs` follow the classic skin format and were cross-checked against Webamp (https://github.com/captbaritone/webamp), MIT License, Copyright (c) Jordan Eldredge. No Webamp code was copied.

## Built-in themes

The built-in themes (`crates/skin/src/theme.rs`) are drawn entirely in code by this project: all pixel art, the 5x6 font, digits and icons are original. Only their color values come from these published color schemes, credited here:

| Theme | Color scheme | License |
|-------|--------------|---------|
| Dracula | Dracula, https://draculatheme.com | MIT |
| Nord | Nord, https://www.nordtheme.com (Arctic Ice Studio) | MIT |
| Gruvbox Dark | gruvbox, https://github.com/morhetz/gruvbox (Pavel Pertsev) | MIT |
| Solarized Dark | Solarized, https://ethanschoonover.com/solarized (Ethan Schoonover) | MIT |
| Catppuccin Mocha | Catppuccin, https://catppuccin.com | MIT |
| Tokyo Night | Tokyo Night, https://github.com/enkia/tokyo-night-vscode-theme (enkia) | MIT |
