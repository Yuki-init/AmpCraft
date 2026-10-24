//! Renders every built-in theme's main window, equalizer and playlist with sample content,
//! one PPM image per theme, for checking the themes by eye. Also renders the System theme
//! on neutral light and dark greys, and from a KDE color scheme when
//! `AMPCRAFT_GALLERY_KDEGLOBALS` names a `kdeglobals` file.
//!
//! `cargo run -p ampcraft --example theme_gallery -- OUT_DIR [SCALE]`

use std::time::Duration;

use ampcraft::system_font::SystemFont;
use ampcraft_skin::Bitmap;
use ampcraft_skin::theme::{Palette, THEMES, Theme};
use ampcraft_ui::{EqWindow, MainWindow, NowPlaying, PlayState, PlaylistEntry, PlaylistWindow};

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let out = std::path::PathBuf::from(args.next().unwrap_or_else(|| ".".into()));
    let scale: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    std::fs::create_dir_all(&out)?;
    let mut font = SystemFont::load();
    let mut themes: Vec<(String, Theme)> = THEMES.iter().map(|t| (t.id.to_owned(), *t)).collect();
    themes.push((
        "system-light".into(),
        Theme::system(Palette::neutral(false, Some(0x3daee9))),
    ));
    themes.push((
        "system-dark".into(),
        Theme::system(Palette::neutral(true, None)),
    ));
    if let Some(kde) = std::env::var_os("AMPCRAFT_GALLERY_KDEGLOBALS")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| ampcraft::appearance::kde_palette(&t))
    {
        themes.push(("system-kde".into(), Theme::system(kde)));
    }
    for (id, theme) in &themes {
        let skin = theme.skin();
        let mut main = MainWindow::new();
        main.now = NowPlaying {
            state: PlayState::Playing,
            position: Duration::from_secs(83),
            duration: Some(Duration::from_secs(225)),
            title: "Ma15 - ハレ晴レユカイ".into(),
            track_number: Some(2),
            kbps: Some(1143),
            khz: Some(44),
            channels: 2,
        };
        main.set_volume(0.7);
        main.set_equalizer_visible(true);
        main.set_playlist_visible(true);
        let levels: [f32; 19] =
            std::array::from_fn(|i| 0.9 - (i as f32 * 0.04) + if i % 3 == 0 { 0.05 } else { 0.0 });
        main.vis.update(&levels, &[], 33.0);
        let mut eq = EqWindow::new();
        eq.enabled = true;
        eq.focused = true;
        eq.bands_db = [6.0, 4.0, 2.0, 0.0, -2.0, -3.0, 0.0, 3.0, 5.0, 7.0];
        eq.preamp_db = 2.0;
        let mut pl = PlaylistWindow::new();
        pl.set_entries(
            [
                ("KEIKO - God Knows...", 230),
                ("Ma15 - ハレ晴レユカイ", 225),
                ("chum - 創聖のアクエリオン", 260),
                ("NINE - Butter-fly", 248),
                ("YURiE - ゆずれない願い", 239),
            ]
            .iter()
            .enumerate()
            .map(|(i, (t, s))| PlaylistEntry {
                id: i as u64 + 1,
                title: (*t).into(),
                duration: Some(Duration::from_secs(*s)),
            })
            .collect(),
        );
        pl.set_current(Some(2));
        pl.now = main.now.clone();

        let parts = [
            main.render(&skin, &mut font, 0, scale),
            eq.render(&skin, scale),
            pl.render(&skin, &mut font, 0, scale),
        ];
        let w = parts.iter().map(Bitmap::width).max().unwrap_or(0);
        let h: u32 = parts.iter().map(Bitmap::height).sum();
        let mut sheet = Bitmap::new(w, h, 0xFF00_0000);
        let mut y = 0;
        for p in &parts {
            sheet.blit(
                p,
                ampcraft_skin::Rect::new(0, 0, p.width() as i32, p.height() as i32),
                0,
                y,
            );
            y += p.height() as i32;
        }
        let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
        for px in sheet.pixels() {
            ppm.extend([(px >> 16) as u8, (px >> 8) as u8, *px as u8]);
        }
        std::fs::write(out.join(format!("{id}.ppm")), ppm)?;
    }
    Ok(())
}
