//! Software renderer, widgets, hit testing and UI state machine.

pub mod dock_drag;
pub mod eq_window;
pub mod gen_frame;
pub mod grip;
pub mod library_window;
pub mod main_window;
pub mod menu;
pub mod move_tracker;
pub mod playlist_window;
pub mod prefs_window;
pub mod render;
pub mod text;
pub mod vis;

pub use dock_drag::{DockDrag, Moves};
pub use eq_window::{EqAction, EqDown, EqWindow};
pub use grip::Grip;
pub use library_window::{
    AlbumRow, ArtistRow, FolderRow, LibAction, LibDown, LibView, LibraryRow, LibraryWindow,
};
pub use main_window::{Action, Control, MainWindow, NowPlaying, PlayState, PointerDown};
pub use menu::{Mark, MenuColors, MenuEntry, MenuEvent, MenuStack};
pub use move_tracker::MoveTracker;
pub use playlist_window::{PlDown, PlaylistAction, PlaylistEntry, PlaylistWindow, SelectMods};
pub use prefs_window::{Page, PrefsAction, PrefsDown, PrefsState, PrefsWindow};
pub use render::scale_nearest;
pub use text::{Mask, NoFallback, TextRenderer};
pub use vis::{VisMode, Visualizer};

/// Generated skins for tests: every pixel encodes its sheet and coordinates.
#[cfg(test)]
pub(crate) mod test_skin {
    use ampcraft_skin::{Bitmap, Sheet, Skin, rgb};

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
            Sheet::Balance => (68, 433),
            Sheet::EqMain => (275, 315),
            Sheet::EqEx => (275, 82),
            Sheet::Pledit => (280, 186),
            Sheet::Gen => (194, 109),
            Sheet::GenEx => (98, 92),
        }
    }

    /// The pixel value at (x, y) of `sheet` in [`coded_skin`]. y is taken modulo 128.
    pub fn code(sheet: Sheet, x: i32, y: i32) -> u32 {
        let idx = Sheet::ALL.iter().position(|s| *s == sheet).unwrap_or(0) as u8;
        rgb(
            idx * 8 + (x >> 8) as u8 * 2 + ((y >> 7) & 1) as u8,
            x as u8,
            y as u8,
        )
    }

    pub fn coded_skin() -> Skin {
        skin_from(
            &Sheet::ALL
                .iter()
                .map(|s| {
                    let (w, h) = size(*s);
                    let mut b = Bitmap::new(w, h, 0);
                    for y in 0..h as i32 {
                        for x in 0..w as i32 {
                            b.set(x, y, code(*s, x, y));
                        }
                    }
                    (*s, b)
                })
                .collect::<Vec<_>>(),
        )
    }

    pub fn skin_from(sheets: &[(Sheet, Bitmap)]) -> Skin {
        Skin::from_sheets(sheets.iter().cloned())
    }
}
