//! Renders a static main window from a skin to a PNG, for checking sprite maps by eye.
//!
//! `cargo run -p amp-skin --example preview -- SKIN.wsz OUT.png [SCALE]`
//!
//! Element positions are the classic main-window layout (cross-checked against Webamp's
//! main-window.css, MIT). They will move to the `ui` crate with the real main window.

use std::path::PathBuf;

use amp_skin::sprites::{self, main, numbers};
use amp_skin::{Bitmap, Skin};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let skin_path = PathBuf::from(args.next().ok_or("usage: preview SKIN OUT.png [SCALE]")?);
    let out = PathBuf::from(args.next().ok_or("missing OUT.png")?);
    let scale: u32 = args
        .next()
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(1)
        .clamp(1, 4);

    let skin = Skin::load(&skin_path)?;
    for w in skin.warnings() {
        eprintln!("warning: {w}");
    }

    let mut win = Bitmap::new(275, 116, 0xFF00_0000);
    let d = |win: &mut Bitmap, s, x, y| {
        skin.draw(win, s, x, y);
    };
    d(&mut win, main::BACKGROUND, 0, 0);
    d(&mut win, main::TITLE_BAR_ACTIVE, 0, 0);
    d(&mut win, main::CLUTTER_BAR, 10, 22);
    d(&mut win, main::PLAYING, 26, 28);
    d(&mut win, main::WORKING, 24, 28);

    if let Some(sheet) = skin.numbers_sheet() {
        // 01:23 elapsed.
        for (digit, x) in [(0u8, 48), (1, 60), (2, 78), (3, 90)] {
            d(&mut win, numbers::digit(sheet, digit), x, 26);
        }
    }
    skin.draw_text(&mut win, "1. OPENAMP PREVIEW - SKIN TEST (4:56)", 111, 27);
    skin.draw_text(&mut win, "320", 111, 43);
    skin.draw_text(&mut win, "44", 156, 43);
    d(&mut win, main::MONO, 212, 41);
    d(&mut win, main::STEREO_ON, 239, 41);

    d(&mut win, sprites::volume_frame(0.75), 107, 57);
    d(&mut win, main::VOLUME_THUMB, 107 + (0.75 * 51.0) as i32, 58);
    d(&mut win, sprites::balance_frame(0.0), 177, 57);
    d(&mut win, main::BALANCE_THUMB, 177 + 12, 58);
    d(&mut win, main::EQ_BUTTON_ON, 219, 58);
    d(&mut win, main::PL_BUTTON, 242, 58);

    d(&mut win, main::POSITION_BACKGROUND, 16, 72);
    d(&mut win, main::POSITION_THUMB, 16 + 60, 72);

    for (s, x) in [
        (main::PREVIOUS, 16),
        (main::PLAY_PRESSED, 39),
        (main::PAUSE, 62),
        (main::STOP, 85),
        (main::NEXT, 108),
    ] {
        d(&mut win, s, x, 88);
    }
    d(&mut win, main::EJECT, 136, 89);
    d(&mut win, main::SHUFFLE, 164, 89);
    d(&mut win, main::REPEAT_ON, 210, 89);

    // Integer scaling, nearest neighbor.
    let (w, h) = (win.width() * scale, win.height() * scale);
    let mut big = Bitmap::new(w, h, 0);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if let Some(p) = win.get(x / scale as i32, y / scale as i32) {
                big.set(x, y, p);
            }
        }
    }
    image::save_buffer(
        &out,
        &big.to_rgba_bytes(),
        w,
        h,
        image::ExtendedColorType::Rgba8,
    )?;
    eprintln!("wrote {}", out.display());
    Ok(())
}
