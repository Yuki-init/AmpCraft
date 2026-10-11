//! The small text files in a skin: `pledit.txt`, `viscolor.txt` and `region.txt`.
//!
//! Real skins are written by hand and are messy: mixed case, stray spaces, missing `#`,
//! comments in odd places, Windows line endings. Parsers here never fail; anything that
//! cannot be read falls back to the classic default.

use crate::bitmap::rgb;

/// Decodes skin text. Skins are mostly ASCII but some use Windows-1252; invalid UTF-8 bytes
/// are mapped through Latin-1 rather than rejected.
pub fn decode_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.trim_start_matches('\u{feff}').to_owned(),
        Err(_) => bytes.iter().map(|b| char::from(*b)).collect(),
    }
}

/// Minimal INI reader: `[section]`, `key=value`, `;` or `//` comments, case-insensitive.
/// Returns (lowercase section, lowercase key, trimmed value) triples in file order.
fn ini(text: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with("//") {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            section = rest.trim_end_matches(']').trim().to_lowercase();
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            // Inline comments: `NumPoints=8    ;Do this LAST!`
            let v = v.split(';').next().unwrap_or("");
            out.push((
                section.clone(),
                k.trim().to_lowercase(),
                v.trim().to_owned(),
            ));
        } else if let Some(last) = out.last_mut().filter(|(s, _, _)| *s == section) {
            // A wrapped value (long PointList lines in region.txt).
            last.2.push(' ');
            last.2.push_str(line);
        }
    }
    out
}

/// Parses `#RRGGBB` or `RRGGBB`, ignoring anything after the six digits.
fn parse_hex_color(v: &str) -> Option<u32> {
    let v = v.trim().trim_start_matches('#');
    let hex = v.get(..6)?;
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some(0xFF00_0000 | n)
}

/// Playlist colors and font from `pledit.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistStyle {
    pub normal: u32,
    pub current: u32,
    pub normal_bg: u32,
    pub selected_bg: u32,
    pub font: String,
}

impl Default for PlaylistStyle {
    fn default() -> Self {
        Self {
            normal: rgb(0x00, 0xFF, 0x00),
            current: rgb(0xFF, 0xFF, 0xFF),
            normal_bg: rgb(0x00, 0x00, 0x00),
            selected_bg: rgb(0x00, 0x00, 0xFF),
            font: "Arial".to_owned(),
        }
    }
}

pub fn parse_pledit(text: &str) -> PlaylistStyle {
    let mut style = PlaylistStyle::default();
    for (section, key, value) in ini(text) {
        if section != "text" {
            continue;
        }
        let slot = match key.as_str() {
            "normal" => &mut style.normal,
            "current" => &mut style.current,
            "normalbg" => &mut style.normal_bg,
            "selectedbg" => &mut style.selected_bg,
            "font" => {
                if !value.is_empty() {
                    style.font = value;
                }
                continue;
            }
            _ => continue,
        };
        if let Some(c) = parse_hex_color(&value) {
            *slot = c;
        }
    }
    style
}

/// Number of visualizer colors in `viscolor.txt`.
pub const VIS_COLORS: usize = 24;

/// The classic default visualizer palette: background, grid dots, 16 spectrum colors from top
/// to bottom, 5 oscilloscope colors, peak dots.
pub const DEFAULT_VIS_COLORS: [u32; VIS_COLORS] = [
    rgb(0, 0, 0),
    rgb(24, 33, 41),
    rgb(239, 49, 16),
    rgb(206, 41, 16),
    rgb(214, 90, 0),
    rgb(214, 102, 0),
    rgb(214, 115, 0),
    rgb(198, 123, 8),
    rgb(222, 165, 24),
    rgb(214, 181, 33),
    rgb(189, 222, 41),
    rgb(148, 222, 33),
    rgb(41, 206, 16),
    rgb(50, 190, 16),
    rgb(57, 181, 16),
    rgb(49, 156, 8),
    rgb(41, 148, 0),
    rgb(24, 132, 8),
    rgb(255, 255, 255),
    rgb(214, 214, 222),
    rgb(181, 189, 189),
    rgb(160, 170, 175),
    rgb(148, 156, 165),
    rgb(150, 150, 150),
];

/// Parses `viscolor.txt`: one `r,g,b` per line, with optional trailing comments. Missing or
/// malformed lines keep the default color for that slot.
pub fn parse_viscolor(text: &str) -> [u32; VIS_COLORS] {
    let mut colors = DEFAULT_VIS_COLORS;
    let mut slot = 0;
    for line in text.lines() {
        if slot >= VIS_COLORS {
            break;
        }
        let line = line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let nums: Vec<u8> = line
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|t| !t.is_empty())
            .take(3)
            .filter_map(|t| t.parse::<u16>().ok().map(|n| n.min(255) as u8))
            .collect();
        if let [r, g, b] = nums[..] {
            colors[slot] = rgb(r, g, b);
        }
        slot += 1;
    }
    colors
}

/// A closed polygon of (x, y) points.
pub type Polygon = Vec<(i32, i32)>;

/// Window shapes from `region.txt`. An empty list means the window is a plain rectangle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Regions {
    pub normal: Vec<Polygon>,
    pub window_shade: Vec<Polygon>,
    pub equalizer: Vec<Polygon>,
    pub equalizer_shade: Vec<Polygon>,
}

/// Parses `region.txt`: per section, `NumPoints` lists the point count of each polygon and
/// `PointList` the coordinates, in pairs. Sections whose numbers do not add up are ignored.
pub fn parse_region(text: &str) -> Regions {
    let mut regions = Regions::default();
    let mut current: Option<(String, Vec<usize>, Vec<i32>)> = None;

    let finish = |regions: &mut Regions, entry: Option<(String, Vec<usize>, Vec<i32>)>| {
        let Some((section, counts, coords)) = entry else {
            return;
        };
        let target = match section.as_str() {
            "normal" => &mut regions.normal,
            "windowshade" => &mut regions.window_shade,
            "equalizer" => &mut regions.equalizer,
            "equalizerws" => &mut regions.equalizer_shade,
            _ => return,
        };
        let needed: usize = counts.iter().sum();
        if counts.is_empty() || coords.len() < needed * 2 || counts.contains(&0) {
            return;
        }
        let mut polys = Vec::with_capacity(counts.len());
        let mut i = 0;
        for n in counts {
            let poly = coords[i..i + n * 2]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|p| (p[0], p[1]))
                .collect();
            polys.push(poly);
            i += n * 2;
        }
        *target = polys;
    };

    let numbers = |v: &str| -> Vec<i32> {
        v.split(|c: char| c == ',' || c.is_whitespace())
            .filter_map(|t| t.trim().parse().ok())
            .collect()
    };

    let mut last_section = String::new();
    for (section, key, value) in ini(text) {
        if section != last_section {
            finish(&mut regions, current.take());
            current = Some((section.clone(), Vec::new(), Vec::new()));
            last_section = section;
        }
        if let Some((_, counts, coords)) = current.as_mut() {
            match key.as_str() {
                "numpoints" => {
                    *counts = numbers(&value)
                        .into_iter()
                        .filter_map(|n| usize::try_from(n).ok())
                        .collect();
                }
                "pointlist" => coords.extend(numbers(&value)),
                _ => {}
            }
        }
    }
    finish(&mut regions, current.take());
    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pledit_parses_messy_input() {
        let text = "[Text]\r\nNormal=#00FF00\r\ncurrent = FFFFFF ; white\r\nNormalBG=#000000\r\n\
                    SELECTEDBG=#0000c6\r\nFont=Arial Narrow\r\n";
        let s = parse_pledit(text);
        assert_eq!(s.current, rgb(255, 255, 255));
        assert_eq!(s.selected_bg, rgb(0, 0, 0xc6));
        assert_eq!(s.font, "Arial Narrow");
    }

    #[test]
    fn pledit_bad_values_keep_defaults() {
        let s = parse_pledit("[text]\nnormal=#zzz\nfont=\n[other]\ncurrent=#123456\n");
        assert_eq!(s, PlaylistStyle::default());
        assert_eq!(parse_pledit(""), PlaylistStyle::default());
    }

    #[test]
    fn viscolor_with_comments_and_short_files() {
        let text = "0,0,0, // background\n 1 , 2 , 3 // dots\n\nbad line\n255,255,300\n";
        let c = parse_viscolor(text);
        assert_eq!(c[0], rgb(0, 0, 0));
        assert_eq!(c[1], rgb(1, 2, 3));
        assert_eq!(c[2], DEFAULT_VIS_COLORS[2], "malformed line keeps default");
        assert_eq!(c[3], rgb(255, 255, 255), "values clamp to 255");
        assert_eq!(c[23], DEFAULT_VIS_COLORS[23], "missing lines keep defaults");
    }

    #[test]
    fn region_polygons_split_by_counts() {
        let text = "[Normal]\nNumPoints=4, 4\nPointList=0,0, 275,0, 275,14, 0,14,\n\
                    0,14 275,14 275,116 0,116\n[WindowShade]\nNumPoints=4\n\
                    PointList=0,0,275,0,275,14,0,14\n";
        let r = parse_region(text);
        assert_eq!(r.normal.len(), 2);
        assert_eq!(r.normal[1], vec![(0, 14), (275, 14), (275, 116), (0, 116)]);
        assert_eq!(r.window_shade.len(), 1);
        assert!(r.equalizer.is_empty());
    }

    #[test]
    fn region_documented_example_with_inline_comments() {
        // The example format documented in the stock skin's region.txt.
        let text = "; header comment\r\n[Normal]\r\nNumPoints=8    ;Do this LAST!\r\n\
                    PointList=0,1,  275,1,  275,14,  274,14,  274,115,  1,115,  1,14,  0,14  \r\n";
        let r = parse_region(text);
        assert_eq!(r.normal.len(), 1);
        assert_eq!(r.normal[0].len(), 8);
        assert_eq!(r.normal[0][4], (274, 115));
    }

    #[test]
    fn fully_commented_region_is_empty() {
        let r = parse_region(";[Normal]\n;NumPoints=4\n;PointList=0,1, 275,1, 275,14, 0,14\n");
        assert_eq!(r, Regions::default());
    }

    #[test]
    fn region_with_wrong_counts_is_ignored() {
        let r =
            parse_region("[Normal]\nNumPoints=5\nPointList=0,0,1,1\n[Equalizer]\nNumPoints=0\n");
        assert_eq!(r, Regions::default());
    }

    #[test]
    fn latin1_text_does_not_fail() {
        let s = decode_text(b"Font=Caf\xe9\n");
        assert_eq!(s, "Font=Café\n");
        assert_eq!(decode_text("\u{feff}[Text]".as_bytes()), "[Text]");
    }
}
