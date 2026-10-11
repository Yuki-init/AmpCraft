//! Classic skin loader: `.wsz` archives, sprite maps, bitmap font, region masks.
//!
//! [`Skin`] holds the decoded sprite sheets and parsed text files of one skin. Missing or
//! unreadable sheets are recorded as warnings instead of failing the load, because many real
//! skins omit files; the UI decides how to fill the gaps.
//!
//! Sprite coordinates and the font map follow the classic skin format and were cross-checked
//! against Webamp (MIT, Copyright Jordan Eldredge).

pub mod archive;
pub mod bitmap;
pub mod config;
pub mod font;
pub mod generic;
pub mod region;
pub mod sprites;
pub mod theme;

use std::collections::BTreeMap;
use std::path::Path;

pub use archive::{Limits, SkinFiles};
pub use bitmap::{Bitmap, Rect, rgb};
pub use config::{PlaylistStyle, Polygon, Regions, VIS_COLORS};
pub use sprites::{Sheet, Sprite};

#[derive(Debug, thiserror::Error)]
pub enum SkinError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a valid zip archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("image could not be decoded: {0}")]
    Image(#[from] image::ImageError),
    #[error("skin rejected: {0}")]
    TooLarge(&'static str),
    #[error("no skin files found")]
    Empty,
}

/// A loaded skin.
#[derive(Debug, Clone)]
pub struct Skin {
    sheets: BTreeMap<Sheet, Bitmap>,
    playlist_style: PlaylistStyle,
    vis_colors: [u32; VIS_COLORS],
    regions: Regions,
    warnings: Vec<String>,
}

/// The average color of `r` in `b`, if `r` lies inside it.
fn average_color(b: &Bitmap, r: Rect) -> Option<u32> {
    if r.x < 0 || r.y < 0 || r.x + r.w > b.width() as i32 || r.y + r.h > b.height() as i32 {
        return None;
    }
    let mut sum = [0u64; 3];
    for y in r.y..r.y + r.h {
        for x in r.x..r.x + r.w {
            let p = b.get(x, y)?;
            sum[0] += u64::from((p >> 16) & 0xFF);
            sum[1] += u64::from((p >> 8) & 0xFF);
            sum[2] += u64::from(p & 0xFF);
        }
    }
    let n = (r.w * r.h).max(1) as u64;
    Some(rgb(
        (sum[0] / n) as u8,
        (sum[1] / n) as u8,
        (sum[2] / n) as u8,
    ))
}

impl Skin {
    /// Loads a `.wsz` / `.zip` file or an unpacked skin directory.
    pub fn load(path: &Path) -> Result<Self, SkinError> {
        let files = if path.is_dir() {
            SkinFiles::from_dir(path, Limits::default())?
        } else {
            SkinFiles::from_zip(&std::fs::read(path)?, Limits::default())?
        };
        Self::from_files(&files)
    }

    /// Loads a skin from zip archive bytes.
    pub fn from_wsz(bytes: &[u8]) -> Result<Self, SkinError> {
        Self::from_files(&SkinFiles::from_zip(bytes, Limits::default())?)
    }

    pub fn from_files(files: &SkinFiles) -> Result<Self, SkinError> {
        let mut warnings = Vec::new();
        let mut sheets = BTreeMap::new();
        for sheet in Sheet::ALL {
            let Some(bytes) = files.get(sheet.file_name()) else {
                // The digit sheets are checked together below.
                if sheet != Sheet::NumsEx {
                    warnings.push(format!("missing {}", sheet.file_name()));
                }
                continue;
            };
            match Bitmap::decode(bytes) {
                Ok(b) => {
                    sheets.insert(sheet, b);
                }
                Err(e) => warnings.push(format!("{}: {e}", sheet.file_name())),
            }
        }
        if sheets.is_empty() {
            return Err(SkinError::Empty);
        }
        // Either digit sheet is enough: NUMS_EX replaces NUMBERS when present.
        if sheets.contains_key(&Sheet::NumsEx) {
            warnings.retain(|w| w != "missing numbers.bmp");
        }
        let text = |name: &str| files.get(name).map(config::decode_text);
        let playlist_style: PlaylistStyle = text("pledit.txt")
            .map(|t| config::parse_pledit(&t))
            .unwrap_or_default();
        // Classic skins often skin only the main, equalizer and playlist windows. Give the
        // generic windows (media library, preferences) art in the skin's own colors.
        if !sheets.contains_key(&Sheet::Gen) || !sheets.contains_key(&Sheet::GenEx) {
            let frame = sheets
                .get(&Sheet::Pledit)
                .and_then(|p| average_color(p, sprites::playlist::LEFT_TILE.rect))
                .unwrap_or(rgb(56, 55, 87));
            let (gen_sheet, genex) = theme::generic_fallback(theme::GenericColors {
                frame,
                list_bg: playlist_style.normal_bg,
                list_fg: playlist_style.normal,
                current: playlist_style.current,
                selected_bg: playlist_style.selected_bg,
            });
            warnings.retain(|w| !w.ends_with("gen.bmp") && !w.ends_with("genex.bmp"));
            sheets.entry(Sheet::Gen).or_insert(gen_sheet);
            sheets.entry(Sheet::GenEx).or_insert(genex);
        }
        Ok(Self {
            sheets,
            playlist_style,
            vis_colors: text("viscolor.txt")
                .map(|t| config::parse_viscolor(&t))
                .unwrap_or(config::DEFAULT_VIS_COLORS),
            regions: text("region.txt")
                .map(|t| config::parse_region(&t))
                .unwrap_or_default(),
            warnings,
        })
    }

    /// Builds a skin from already decoded sheets, with default text settings. For skins
    /// generated in code and for tests.
    pub fn from_sheets(sheets: impl IntoIterator<Item = (Sheet, Bitmap)>) -> Self {
        Self {
            sheets: sheets.into_iter().collect(),
            playlist_style: PlaylistStyle::default(),
            vis_colors: config::DEFAULT_VIS_COLORS,
            regions: Regions::default(),
            warnings: Vec::new(),
        }
    }

    /// Replaces the playlist colors and visualizer palette with ones parsed from pledit.txt
    /// and viscolor.txt text.
    pub fn with_text_files(mut self, pledit: &str, viscolor: &str) -> Self {
        self.playlist_style = config::parse_pledit(pledit);
        self.vis_colors = config::parse_viscolor(viscolor);
        self
    }

    pub fn sheet(&self, sheet: Sheet) -> Option<&Bitmap> {
        self.sheets.get(&sheet)
    }

    /// Draws a sprite at (x, y). Returns false if its sheet is missing.
    pub fn draw(&self, dst: &mut Bitmap, sprite: Sprite, x: i32, y: i32) -> bool {
        match self.sheets.get(&sprite.sheet) {
            Some(src) => {
                dst.blit(src, sprite.rect, x, y);
                true
            }
            None => false,
        }
    }

    /// The sheet to use for big time digits: NUMS_EX when present, else NUMBERS.
    pub fn numbers_sheet(&self) -> Option<Sheet> {
        [Sheet::NumsEx, Sheet::Numbers]
            .into_iter()
            .find(|s| self.sheets.contains_key(s))
    }

    /// Draws `text` with the skin's TEXT.BMP font. Returns the width drawn, or None if the
    /// skin has no font.
    pub fn draw_text(&self, dst: &mut Bitmap, text: &str, x: i32, y: i32) -> Option<i32> {
        let font = self.sheets.get(&Sheet::Text)?;
        Some(font::draw_text(dst, font, text, x, y))
    }

    pub fn playlist_style(&self) -> &PlaylistStyle {
        &self.playlist_style
    }

    pub fn vis_colors(&self) -> &[u32; VIS_COLORS] {
        &self.vis_colors
    }

    pub fn regions(&self) -> &Regions {
        &self.regions
    }

    /// Problems found while loading (missing or undecodable files).
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}
