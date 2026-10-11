//! Loads skins generated in code. Every pixel encodes its sheet and coordinates, so a sprite
//! drawn from the wrong place is caught exactly.

// Integration tests are test code; panicking on setup failure is the point.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{Cursor, Write};

use amp_skin::sprites::{main, numbers};
use amp_skin::{Bitmap, Sheet, Skin, font, rgb};
use zip::write::SimpleFileOptions;

/// Classic sheet sizes.
fn size(sheet: Sheet) -> (u32, u32) {
    match sheet {
        Sheet::Main => (275, 116),
        Sheet::Cbuttons => (136, 36),
        Sheet::Titlebar => (344, 87),
        Sheet::Shufrep => (92, 85),
        Sheet::Text => (155, 18),
        Sheet::Numbers => (99, 13),
        Sheet::NumsEx => (108, 13),
        Sheet::Playpaus => (42, 9),
        Sheet::Monoster => (58, 24),
        Sheet::Posbar => (307, 10),
        Sheet::Volume => (68, 433),
        Sheet::Balance => (47, 433),
        Sheet::EqMain => (275, 315),
        Sheet::EqEx => (275, 82),
        Sheet::Pledit => (280, 186),
        Sheet::Gen => (194, 109),
        Sheet::GenEx => (98, 92),
    }
}

/// Pixel value encoding sheet, x and y (y modulo 256).
fn code(sheet: Sheet, x: u32, y: u32) -> [u8; 3] {
    let idx = Sheet::ALL.iter().position(|s| *s == sheet).unwrap() as u8;
    [(idx << 2) | (x >> 8) as u8, x as u8, y as u8]
}

fn sheet_image(sheet: Sheet) -> image::RgbImage {
    let (w, h) = size(sheet);
    image::RgbImage::from_fn(w, h, |x, y| image::Rgb(code(sheet, x, y)))
}

fn encode(img: &image::RgbImage, format: image::ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, format).unwrap();
    out.into_inner()
}

struct SkinZip {
    writer: zip::ZipWriter<Cursor<Vec<u8>>>,
}

impl SkinZip {
    fn new() -> Self {
        Self {
            writer: zip::ZipWriter::new(Cursor::new(Vec::new())),
        }
    }

    fn add(&mut self, name: &str, data: &[u8]) -> &mut Self {
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        self.writer.start_file(name, opts).unwrap();
        self.writer.write_all(data).unwrap();
        self
    }

    fn finish(self) -> Vec<u8> {
        self.writer.finish().unwrap().into_inner()
    }
}

/// A complete skin with every sheet, in a folder with upper-case names, like many real ones.
fn full_skin() -> Vec<u8> {
    let mut z = SkinZip::new();
    for sheet in Sheet::ALL {
        let name = format!("Generated/{}", sheet.file_name().to_uppercase());
        z.add(&name, &encode(&sheet_image(sheet), image::ImageFormat::Bmp));
    }
    z.add(
        "Generated/PLEDIT.TXT",
        b"[Text]\r\nNormal=#112233\r\nFont=Tahoma\r\n",
    );
    z.add("Generated/VISCOLOR.TXT", b"9,8,7, // bg\n");
    z.add(
        "Generated/REGION.TXT",
        b"[Normal]\nNumPoints=4\nPointList=0,0,275,0,275,116,0,116\n",
    );
    z.finish()
}

fn rgb_of(c: [u8; 3]) -> u32 {
    rgb(c[0], c[1], c[2])
}

#[test]
fn full_skin_loads_without_warnings() {
    let skin = Skin::from_wsz(&full_skin()).unwrap();
    assert!(skin.warnings().is_empty(), "{:?}", skin.warnings());
    for sheet in Sheet::ALL {
        let b = skin.sheet(sheet).unwrap();
        assert_eq!((b.width(), b.height()), size(sheet), "{sheet:?}");
    }
    assert_eq!(skin.playlist_style().normal, rgb(0x11, 0x22, 0x33));
    assert_eq!(skin.playlist_style().font, "Tahoma");
    assert_eq!(skin.vis_colors()[0], rgb(9, 8, 7));
    assert_eq!(skin.regions().normal.len(), 1);
}

#[test]
fn every_main_sprite_fits_its_classic_sheet() {
    for s in main::ALL {
        let (w, h) = size(s.sheet);
        let r = s.rect;
        assert!(
            r.x + r.w <= w as i32 && r.y + r.h <= h as i32,
            "{s:?} exceeds {w}x{h}"
        );
    }
}

#[test]
fn sprites_are_taken_from_the_right_place() {
    let skin = Skin::from_wsz(&full_skin()).unwrap();
    for sprite in [
        main::PLAY_PRESSED,
        main::CLOSE,
        main::SHUFFLE_ON,
        main::WORKING,
        main::VOLUME_THUMB,
    ] {
        let mut canvas = Bitmap::new(100, 100, 0);
        assert!(skin.draw(&mut canvas, sprite, 3, 4));
        let r = sprite.rect;
        for (dx, dy) in [(0, 0), (r.w - 1, r.h - 1), (r.w / 2, r.h / 2)] {
            let expected = rgb_of(code(sprite.sheet, (r.x + dx) as u32, (r.y + dy) as u32));
            assert_eq!(
                canvas.get(3 + dx, 4 + dy),
                Some(expected),
                "{sprite:?} at +{dx},{dy}"
            );
        }
        assert_eq!(
            canvas.get(3 + r.w, 4),
            Some(0),
            "nothing drawn past the sprite"
        );
    }
}

#[test]
fn text_renders_glyph_cells_in_order() {
    let skin = Skin::from_wsz(&full_skin()).unwrap();
    let mut canvas = Bitmap::new(60, 10, 0);
    let w = skin.draw_text(&mut canvas, "Hi 9?", 1, 2).unwrap();
    assert_eq!(w, 25);
    for (i, c) in "Hi 9?".chars().enumerate() {
        let g = font::glyph_rect(c);
        let x = 1 + i as i32 * font::CHAR_W;
        assert_eq!(
            canvas.get(x, 2),
            Some(rgb_of(code(Sheet::Text, g.x as u32, g.y as u32))),
            "{c}"
        );
        assert_eq!(
            canvas.get(x + 4, 7),
            Some(rgb_of(code(
                Sheet::Text,
                (g.x + 4) as u32,
                (g.y + 5) as u32
            )))
        );
    }
}

#[test]
fn numbers_prefer_nums_ex_and_fall_back() {
    let skin = Skin::from_wsz(&full_skin()).unwrap();
    assert_eq!(skin.numbers_sheet(), Some(Sheet::NumsEx));

    let mut z = SkinZip::new();
    z.add(
        "main.bmp",
        &encode(&sheet_image(Sheet::Main), image::ImageFormat::Bmp),
    );
    z.add(
        "numbers.bmp",
        &encode(&sheet_image(Sheet::Numbers), image::ImageFormat::Bmp),
    );
    let skin = Skin::from_wsz(&z.finish()).unwrap();
    assert_eq!(skin.numbers_sheet(), Some(Sheet::Numbers));
    let mut canvas = Bitmap::new(20, 20, 0);
    assert!(skin.draw(&mut canvas, numbers::digit(Sheet::Numbers, 4), 0, 0));
    assert_eq!(canvas.get(0, 0), Some(rgb_of(code(Sheet::Numbers, 36, 0))));
}

#[test]
fn partial_skin_loads_with_warnings() {
    let mut z = SkinZip::new();
    z.add(
        "main.bmp",
        &encode(&sheet_image(Sheet::Main), image::ImageFormat::Bmp),
    );
    z.add("cbuttons.bmp", b"this is not an image");
    let skin = Skin::from_wsz(&z.finish()).unwrap();
    assert!(skin.sheet(Sheet::Main).is_some());
    assert!(skin.sheet(Sheet::Cbuttons).is_none());
    assert!(
        skin.warnings()
            .iter()
            .any(|w| w.starts_with("cbuttons.bmp:"))
    );
    assert!(skin.warnings().iter().any(|w| w == "missing text.bmp"));
    assert!(
        !skin.warnings().iter().any(|w| w.contains("nums_ex")),
        "NUMS_EX is optional"
    );
    let mut canvas = Bitmap::new(10, 10, 0);
    assert!(!skin.draw(&mut canvas, main::PLAY, 0, 0));
    assert_eq!(skin.draw_text(&mut canvas, "x", 0, 0), None);
    // Defaults for missing text files.
    assert_eq!(skin.vis_colors(), &amp_skin::config::DEFAULT_VIS_COLORS);
}

#[test]
fn png_named_bmp_and_8bit_palette_bmp_decode() {
    let mut z = SkinZip::new();
    z.add(
        "main.bmp",
        &encode(&sheet_image(Sheet::Main), image::ImageFormat::Png),
    );
    let gray = image::GrayImage::from_fn(155, 18, |x, _| image::Luma([x as u8]));
    let mut bytes = Cursor::new(Vec::new());
    gray.write_to(&mut bytes, image::ImageFormat::Bmp).unwrap();
    z.add("text.bmp", bytes.get_ref());
    let skin = Skin::from_wsz(&z.finish()).unwrap();
    assert_eq!(
        skin.sheet(Sheet::Main).unwrap().get(10, 5),
        Some(rgb_of(code(Sheet::Main, 10, 5)))
    );
    assert_eq!(
        skin.sheet(Sheet::Text).unwrap().get(7, 3),
        Some(rgb(7, 7, 7))
    );
}

#[test]
fn archive_without_images_is_rejected() {
    let mut z = SkinZip::new();
    z.add("readme.txt", b"hello");
    assert!(matches!(
        Skin::from_wsz(&z.finish()),
        Err(amp_skin::SkinError::Empty)
    ));
}

#[test]
fn directory_skin_loads() {
    let dir = tempfile::tempdir().unwrap();
    for sheet in [Sheet::Main, Sheet::Text] {
        std::fs::write(
            dir.path().join(sheet.file_name().to_uppercase()),
            encode(&sheet_image(sheet), image::ImageFormat::Bmp),
        )
        .unwrap();
    }
    let skin = Skin::load(dir.path()).unwrap();
    assert!(skin.sheet(Sheet::Text).is_some());
}
