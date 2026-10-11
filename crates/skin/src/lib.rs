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
                // NUMS_EX is optional by design; NUMBERS is the fallback.
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
        let text = |name: &str| files.get(name).map(config::decode_text);
        Ok(Self {
            sheets,
            playlist_style: text("pledit.txt")
                .map(|t| config::parse_pledit(&t))
                .unwrap_or_default(),
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
