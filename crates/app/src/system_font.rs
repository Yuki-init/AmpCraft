//! System-font fallback for titles the skin's bitmap font cannot show.
//!
//! Fonts are found with fontdb (fontconfig directories) and rasterized with fontdue. Each
//! character uses the first preferred font that has a glyph for it, so Japanese titles pick
//! up a CJK font while Latin text stays in a plain sans-serif.

use amp_ui::{Mask, TextRenderer};
use fontdb::{Database, Family, Query};

/// Families tried in order; the database's generic sans-serif comes last.
const PREFERRED: &[&str] = &[
    "Noto Sans CJK JP",
    "Noto Sans JP",
    "Source Han Sans JP",
    "IPAGothic",
    "IPAexGothic",
    "VL Gothic",
    "TakaoGothic",
    "Droid Sans Fallback",
    "Noto Sans",
    "DejaVu Sans",
    "Liberation Sans",
    "Arial",
];

pub struct SystemFont {
    fonts: Vec<fontdue::Font>,
}

impl std::fmt::Debug for SystemFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SystemFont({} fonts)", self.fonts.len())
    }
}

impl SystemFont {
    /// Scans the system's fonts. Finding none is fine: rendering then returns None.
    pub fn load() -> Self {
        let mut db = Database::new();
        db.load_system_fonts();
        let mut fonts = Vec::new();
        let mut seen = Vec::new();
        let families = PREFERRED
            .iter()
            .map(|f| Family::Name(f))
            .chain([Family::SansSerif]);
        for family in families {
            let query = Query {
                families: &[family],
                ..Query::default()
            };
            let Some(id) = db.query(&query) else {
                continue;
            };
            if seen.contains(&id) {
                continue;
            }
            seen.push(id);
            let font = db.with_face_data(id, |data, index| {
                fontdue::Font::from_bytes(
                    data,
                    fontdue::FontSettings {
                        collection_index: index,
                        ..fontdue::FontSettings::default()
                    },
                )
                .ok()
            });
            if let Some(Some(f)) = font {
                fonts.push(f);
            }
        }
        log::info!("system fonts for titles: {}", fonts.len());
        Self { fonts }
    }

    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    fn font_for(&self, c: char) -> Option<&fontdue::Font> {
        self.fonts
            .iter()
            .find(|f| f.lookup_glyph_index(c) != 0)
            .or_else(|| self.fonts.first())
    }
}

impl TextRenderer for SystemFont {
    fn render(&mut self, text: &str, height: u32) -> Option<Mask> {
        let primary = self.fonts.first()?;
        let h = height.max(1) as f32;
        // Size so ascent plus descent fills the height.
        let px = primary
            .horizontal_line_metrics(h)
            .map_or(h, |m| h * h / (m.ascent - m.descent).max(1.0));
        let ascent = primary
            .horizontal_line_metrics(px)
            .map_or(px * 0.8, |m| m.ascent)
            .round() as i32;

        let glyphs: Vec<(fontdue::Metrics, Vec<u8>)> = text
            .chars()
            .map(|c| match self.font_for(c) {
                Some(f) => f.rasterize(c, px),
                None => (fontdue::Metrics::default(), Vec::new()),
            })
            .collect();
        let width: i32 = glyphs
            .iter()
            .map(|(m, _)| m.advance_width.round() as i32)
            .sum::<i32>()
            .max(1);
        let mut alpha = vec![0u8; width as usize * height as usize];
        let mut pen = 0i32;
        for (m, bitmap) in &glyphs {
            let top = ascent - (m.height as i32 + m.ymin);
            for gy in 0..m.height as i32 {
                for gx in 0..m.width as i32 {
                    let (x, y) = (pen + m.xmin + gx, top + gy);
                    if x < 0 || y < 0 || x >= width || y >= height as i32 {
                        continue;
                    }
                    let a = bitmap[(gy * m.width as i32 + gx) as usize];
                    let slot = &mut alpha[(y * width + x) as usize];
                    *slot = (*slot).max(a);
                }
            }
            pen += m.advance_width.round() as i32;
        }
        Some(Mask {
            width: width as u32,
            height,
            alpha,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_japanese_when_fonts_exist() {
        let mut f = SystemFont::load();
        if f.is_empty() {
            eprintln!("no system fonts, skipping");
            return;
        }
        let mask = f.render("ハレ晴レユカイ", 11).unwrap();
        assert_eq!(mask.height, 11);
        assert!(mask.width > 40, "width {}", mask.width);
        let inked = mask.alpha.iter().filter(|a| **a > 128).count();
        assert!(inked > 50, "only {inked} inked pixels");
    }

    #[test]
    fn empty_font_list_renders_nothing() {
        let mut f = SystemFont { fonts: Vec::new() };
        assert!(f.render("abc", 11).is_none());
    }
}
