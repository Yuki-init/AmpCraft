//! System-font fallback for titles the skin's bitmap font cannot show.
//!
//! Fonts are found with fontdb (fontconfig directories) and rendered with swash using
//! hinting, so strokes land on whole pixels at the small sizes the skin layout needs; a mild
//! contrast curve keeps thin CJK strokes from washing out. Each character uses the first
//! preferred font that has a glyph for it, so Japanese titles pick up a CJK font while Latin
//! text stays in a well-hinted sans-serif.

use ampcraft_ui::{Mask, TextRenderer};
use fontdb::{Database, Family, Query};
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::Format;
use swash::{CacheKey, FontRef};

/// Families tried in order; the database's generic sans-serif comes last. Latin first:
/// DejaVu and Noto Sans hint well at small sizes, and CJK fonts fill in what they lack: the
/// common Linux ones, then the ones every Windows install has (Japanese, Korean, Chinese).
const PREFERRED: &[&str] = &[
    "DejaVu Sans",
    "Noto Sans",
    "Liberation Sans",
    "Arial",
    "Noto Sans CJK JP",
    "Noto Sans JP",
    "Source Han Sans JP",
    "IPAGothic",
    "IPAexGothic",
    "VL Gothic",
    "TakaoGothic",
    "Droid Sans Fallback",
    "Yu Gothic UI",
    "Yu Gothic",
    "Meiryo UI",
    "Meiryo",
    "MS Gothic",
    "Malgun Gothic",
    "Microsoft YaHei UI",
    "Microsoft YaHei",
    "Microsoft JhengHei UI",
    "Microsoft JhengHei",
];

/// Exponent applied to coverage; below 1 darkens partial pixels for crisper small text.
const CONTRAST: f32 = 0.75;

struct LoadedFont {
    data: Vec<u8>,
    offset: u32,
    key: CacheKey,
}

impl LoadedFont {
    fn as_ref(&self) -> FontRef<'_> {
        FontRef {
            data: &self.data,
            offset: self.offset,
            key: self.key,
        }
    }
}

pub struct SystemFont {
    fonts: Vec<LoadedFont>,
    context: ScaleContext,
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
            let loaded = db.with_face_data(id, |data, index| {
                let font = FontRef::from_index(data, index as usize)?;
                Some(LoadedFont {
                    data: data.to_vec(),
                    offset: font.offset,
                    key: font.key,
                })
            });
            if let Some(Some(f)) = loaded {
                fonts.push(f);
            }
        }
        log::info!("system fonts for titles: {}", fonts.len());
        Self {
            fonts,
            context: ScaleContext::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }
}

impl TextRenderer for SystemFont {
    fn render(&mut self, text: &str, height: u32) -> Option<Mask> {
        let primary = self.fonts.first()?.as_ref();
        let h = height.max(1) as f32;
        // Size so ascent plus descent fills the height, then put the baseline on a pixel.
        let unit = primary.metrics(&[]).scale(1.0);
        let px = h / (unit.ascent + unit.descent).max(0.1);
        let ascent = (unit.ascent * px).round() as i32;

        struct Placed {
            left: i32,
            top: i32,
            width: u32,
            height: u32,
            data: Vec<u8>,
            pen: i32,
        }
        let mut placed = Vec::new();
        let mut pen = 0i32;
        for c in text.chars() {
            let font = self
                .fonts
                .iter()
                .map(LoadedFont::as_ref)
                .find(|f| f.charmap().map(c) != 0)
                .unwrap_or(primary);
            let glyph = font.charmap().map(c);
            let advance = font
                .glyph_metrics(&[])
                .scale(px)
                .advance_width(glyph)
                .round() as i32;
            let mut scaler = self.context.builder(font).size(px).hint(true).build();
            let image = Render::new(&[Source::Bitmap(StrikeWith::BestFit), Source::Outline])
                .format(Format::Alpha)
                .render(&mut scaler, glyph);
            if let Some(img) = image {
                placed.push(Placed {
                    left: img.placement.left,
                    top: img.placement.top,
                    width: img.placement.width,
                    height: img.placement.height,
                    data: img.data,
                    pen,
                });
            }
            pen += advance;
        }

        let width = pen.max(1);
        let mut alpha = vec![0u8; width as usize * height as usize];
        let curve: Vec<u8> = (0..=255u32)
            .map(|a| ((a as f32 / 255.0).powf(CONTRAST) * 255.0).round() as u8)
            .collect();
        for g in placed {
            for gy in 0..g.height as i32 {
                for gx in 0..g.width as i32 {
                    let (x, y) = (g.pen + g.left + gx, ascent - g.top + gy);
                    if x < 0 || y < 0 || x >= width || y >= height as i32 {
                        continue;
                    }
                    let a = curve[usize::from(g.data[(gy * g.width as i32 + gx) as usize])];
                    let slot = &mut alpha[(y * width + x) as usize];
                    *slot = (*slot).max(a);
                }
            }
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
    fn renders_japanese_and_latin_when_fonts_exist() {
        let mut f = SystemFont::load();
        if f.is_empty() {
            eprintln!("no system fonts, skipping");
            return;
        }
        for (text, min_width) in [("ハレ晴レユカイ", 40), ("God Knows... 1:15", 50)] {
            let mask = f.render(text, 11).unwrap();
            assert_eq!(mask.height, 11);
            assert!(mask.width > min_width, "{text}: width {}", mask.width);
            let inked = mask.alpha.iter().filter(|a| **a > 128).count();
            assert!(inked > 40, "{text}: only {inked} inked pixels");
        }
    }

    #[test]
    fn doubling_the_height_roughly_doubles_the_width() {
        let mut f = SystemFont::load();
        if f.is_empty() {
            return;
        }
        let one = f.render("Scale test", 10).unwrap().width as f32;
        let two = f.render("Scale test", 20).unwrap().width as f32;
        assert!((two / one - 2.0).abs() < 0.25, "{one} -> {two}");
    }

    #[test]
    fn empty_font_list_renders_nothing() {
        let mut f = SystemFont {
            fonts: Vec::new(),
            context: ScaleContext::new(),
        };
        assert!(f.render("abc", 11).is_none());
    }
}

#[cfg(test)]
mod preferred_tests {
    use super::PREFERRED;

    #[test]
    fn windows_cjk_fonts_are_in_the_fallback_list() {
        // Regression: on Windows only Arial matched, so Japanese titles showed as boxes.
        for family in ["Yu Gothic UI", "Meiryo", "Malgun Gothic", "Microsoft YaHei"] {
            assert!(PREFERRED.contains(&family), "{family}");
        }
        // Latin fonts stay ahead of every CJK font.
        let arial = PREFERRED.iter().position(|f| *f == "Arial");
        let first_cjk = PREFERRED.iter().position(|f| f.contains("CJK"));
        assert!(arial < first_cjk);
    }
}
