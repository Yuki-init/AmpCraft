//! Sprite sheets and the coordinates of the main window's sprites inside them.
//!
//! Coordinates follow the classic skin format and were cross-checked against Webamp's
//! `skinSprites.ts` (MIT, Copyright Jordan Eldredge). Equalizer and playlist sprites are added
//! with those windows.

use crate::bitmap::Rect;

/// One image file of a classic skin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Sheet {
    Main,
    Cbuttons,
    Titlebar,
    Shufrep,
    Text,
    Numbers,
    NumsEx,
    Playpaus,
    Monoster,
    Posbar,
    Volume,
    Balance,
    EqMain,
    EqEx,
    Pledit,
    Gen,
    GenEx,
}

impl Sheet {
    pub const ALL: [Sheet; 17] = [
        Sheet::Main,
        Sheet::Cbuttons,
        Sheet::Titlebar,
        Sheet::Shufrep,
        Sheet::Text,
        Sheet::Numbers,
        Sheet::NumsEx,
        Sheet::Playpaus,
        Sheet::Monoster,
        Sheet::Posbar,
        Sheet::Volume,
        Sheet::Balance,
        Sheet::EqMain,
        Sheet::EqEx,
        Sheet::Pledit,
        Sheet::Gen,
        Sheet::GenEx,
    ];

    /// File name inside the skin archive (matched case-insensitively).
    pub const fn file_name(self) -> &'static str {
        match self {
            Sheet::Main => "main.bmp",
            Sheet::Cbuttons => "cbuttons.bmp",
            Sheet::Titlebar => "titlebar.bmp",
            Sheet::Shufrep => "shufrep.bmp",
            Sheet::Text => "text.bmp",
            Sheet::Numbers => "numbers.bmp",
            Sheet::NumsEx => "nums_ex.bmp",
            Sheet::Playpaus => "playpaus.bmp",
            Sheet::Monoster => "monoster.bmp",
            Sheet::Posbar => "posbar.bmp",
            Sheet::Volume => "volume.bmp",
            Sheet::Balance => "balance.bmp",
            Sheet::EqMain => "eqmain.bmp",
            Sheet::EqEx => "eq_ex.bmp",
            Sheet::Pledit => "pledit.bmp",
            Sheet::Gen => "gen.bmp",
            Sheet::GenEx => "genex.bmp",
        }
    }
}

/// A rectangle inside one sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sprite {
    pub sheet: Sheet,
    pub rect: Rect,
}

const fn s(sheet: Sheet, x: i32, y: i32, w: i32, h: i32) -> Sprite {
    Sprite {
        sheet,
        rect: Rect::new(x, y, w, h),
    }
}

/// Size of the main window at 1x.
pub const MAIN_WINDOW_W: i32 = 275;
pub const MAIN_WINDOW_H: i32 = 116;

/// Main window sprites.
pub mod main {
    use super::{Sheet::*, Sprite, s};

    pub const BACKGROUND: Sprite = s(Main, 0, 0, 275, 116);

    pub const PREVIOUS: Sprite = s(Cbuttons, 0, 0, 23, 18);
    pub const PREVIOUS_PRESSED: Sprite = s(Cbuttons, 0, 18, 23, 18);
    pub const PLAY: Sprite = s(Cbuttons, 23, 0, 23, 18);
    pub const PLAY_PRESSED: Sprite = s(Cbuttons, 23, 18, 23, 18);
    pub const PAUSE: Sprite = s(Cbuttons, 46, 0, 23, 18);
    pub const PAUSE_PRESSED: Sprite = s(Cbuttons, 46, 18, 23, 18);
    pub const STOP: Sprite = s(Cbuttons, 69, 0, 23, 18);
    pub const STOP_PRESSED: Sprite = s(Cbuttons, 69, 18, 23, 18);
    pub const NEXT: Sprite = s(Cbuttons, 92, 0, 22, 18);
    pub const NEXT_PRESSED: Sprite = s(Cbuttons, 92, 18, 22, 18);
    pub const EJECT: Sprite = s(Cbuttons, 114, 0, 22, 16);
    pub const EJECT_PRESSED: Sprite = s(Cbuttons, 114, 16, 22, 16);

    pub const TITLE_BAR: Sprite = s(Titlebar, 27, 15, 275, 14);
    pub const TITLE_BAR_ACTIVE: Sprite = s(Titlebar, 27, 0, 275, 14);
    pub const SHADE_BACKGROUND: Sprite = s(Titlebar, 27, 42, 275, 14);
    pub const SHADE_BACKGROUND_ACTIVE: Sprite = s(Titlebar, 27, 29, 275, 14);
    pub const OPTIONS: Sprite = s(Titlebar, 0, 0, 9, 9);
    pub const OPTIONS_PRESSED: Sprite = s(Titlebar, 0, 9, 9, 9);
    pub const MINIMIZE: Sprite = s(Titlebar, 9, 0, 9, 9);
    pub const MINIMIZE_PRESSED: Sprite = s(Titlebar, 9, 9, 9, 9);
    pub const CLOSE: Sprite = s(Titlebar, 18, 0, 9, 9);
    pub const CLOSE_PRESSED: Sprite = s(Titlebar, 18, 9, 9, 9);
    pub const SHADE: Sprite = s(Titlebar, 0, 18, 9, 9);
    pub const SHADE_PRESSED: Sprite = s(Titlebar, 9, 18, 9, 9);
    pub const UNSHADE: Sprite = s(Titlebar, 0, 27, 9, 9);
    pub const UNSHADE_PRESSED: Sprite = s(Titlebar, 9, 27, 9, 9);
    pub const CLUTTER_BAR: Sprite = s(Titlebar, 304, 0, 8, 43);
    pub const CLUTTER_BAR_DISABLED: Sprite = s(Titlebar, 312, 0, 8, 43);
    pub const SHADE_POSITION_BACKGROUND: Sprite = s(Titlebar, 0, 36, 17, 7);
    pub const SHADE_POSITION_THUMB: Sprite = s(Titlebar, 20, 36, 3, 7);
    pub const SHADE_POSITION_THUMB_LEFT: Sprite = s(Titlebar, 17, 36, 3, 7);
    pub const SHADE_POSITION_THUMB_RIGHT: Sprite = s(Titlebar, 23, 36, 3, 7);

    pub const REPEAT: Sprite = s(Shufrep, 0, 0, 28, 15);
    pub const REPEAT_PRESSED: Sprite = s(Shufrep, 0, 15, 28, 15);
    pub const REPEAT_ON: Sprite = s(Shufrep, 0, 30, 28, 15);
    pub const REPEAT_ON_PRESSED: Sprite = s(Shufrep, 0, 45, 28, 15);
    pub const SHUFFLE: Sprite = s(Shufrep, 28, 0, 47, 15);
    pub const SHUFFLE_PRESSED: Sprite = s(Shufrep, 28, 15, 47, 15);
    pub const SHUFFLE_ON: Sprite = s(Shufrep, 28, 30, 47, 15);
    pub const SHUFFLE_ON_PRESSED: Sprite = s(Shufrep, 28, 45, 47, 15);
    pub const EQ_BUTTON: Sprite = s(Shufrep, 0, 61, 23, 12);
    pub const EQ_BUTTON_ON: Sprite = s(Shufrep, 0, 73, 23, 12);
    pub const EQ_BUTTON_PRESSED: Sprite = s(Shufrep, 46, 61, 23, 12);
    pub const EQ_BUTTON_ON_PRESSED: Sprite = s(Shufrep, 46, 73, 23, 12);
    pub const PL_BUTTON: Sprite = s(Shufrep, 23, 61, 23, 12);
    pub const PL_BUTTON_ON: Sprite = s(Shufrep, 23, 73, 23, 12);
    pub const PL_BUTTON_PRESSED: Sprite = s(Shufrep, 69, 61, 23, 12);
    pub const PL_BUTTON_ON_PRESSED: Sprite = s(Shufrep, 69, 73, 23, 12);

    pub const PLAYING: Sprite = s(Playpaus, 0, 0, 9, 9);
    pub const PAUSED: Sprite = s(Playpaus, 9, 0, 9, 9);
    pub const STOPPED: Sprite = s(Playpaus, 18, 0, 9, 9);
    /// 3 px strips left of the play state, showing whether decoding keeps up.
    pub const NOT_WORKING: Sprite = s(Playpaus, 36, 0, 3, 9);
    pub const WORKING: Sprite = s(Playpaus, 39, 0, 3, 9);

    pub const STEREO_ON: Sprite = s(Monoster, 0, 0, 29, 12);
    pub const STEREO: Sprite = s(Monoster, 0, 12, 29, 12);
    pub const MONO_ON: Sprite = s(Monoster, 29, 0, 27, 12);
    pub const MONO: Sprite = s(Monoster, 29, 12, 27, 12);

    pub const POSITION_BACKGROUND: Sprite = s(Posbar, 0, 0, 248, 10);
    pub const POSITION_THUMB: Sprite = s(Posbar, 248, 0, 29, 10);
    pub const POSITION_THUMB_PRESSED: Sprite = s(Posbar, 278, 0, 29, 10);

    pub const VOLUME_THUMB: Sprite = s(Volume, 15, 422, 14, 11);
    pub const VOLUME_THUMB_PRESSED: Sprite = s(Volume, 0, 422, 14, 11);
    pub const BALANCE_THUMB: Sprite = s(Balance, 15, 422, 14, 11);
    pub const BALANCE_THUMB_PRESSED: Sprite = s(Balance, 0, 422, 14, 11);

    /// Every fixed main-window sprite, for validation.
    pub const ALL: &[Sprite] = &[
        BACKGROUND,
        PREVIOUS,
        PREVIOUS_PRESSED,
        PLAY,
        PLAY_PRESSED,
        PAUSE,
        PAUSE_PRESSED,
        STOP,
        STOP_PRESSED,
        NEXT,
        NEXT_PRESSED,
        EJECT,
        EJECT_PRESSED,
        TITLE_BAR,
        TITLE_BAR_ACTIVE,
        SHADE_BACKGROUND,
        SHADE_BACKGROUND_ACTIVE,
        OPTIONS,
        OPTIONS_PRESSED,
        MINIMIZE,
        MINIMIZE_PRESSED,
        CLOSE,
        CLOSE_PRESSED,
        SHADE,
        SHADE_PRESSED,
        UNSHADE,
        UNSHADE_PRESSED,
        CLUTTER_BAR,
        CLUTTER_BAR_DISABLED,
        SHADE_POSITION_BACKGROUND,
        SHADE_POSITION_THUMB,
        SHADE_POSITION_THUMB_LEFT,
        SHADE_POSITION_THUMB_RIGHT,
        REPEAT,
        REPEAT_PRESSED,
        REPEAT_ON,
        REPEAT_ON_PRESSED,
        SHUFFLE,
        SHUFFLE_PRESSED,
        SHUFFLE_ON,
        SHUFFLE_ON_PRESSED,
        EQ_BUTTON,
        EQ_BUTTON_ON,
        EQ_BUTTON_PRESSED,
        EQ_BUTTON_ON_PRESSED,
        PL_BUTTON,
        PL_BUTTON_ON,
        PL_BUTTON_PRESSED,
        PL_BUTTON_ON_PRESSED,
        PLAYING,
        PAUSED,
        STOPPED,
        NOT_WORKING,
        WORKING,
        STEREO_ON,
        STEREO,
        MONO_ON,
        MONO,
        POSITION_BACKGROUND,
        POSITION_THUMB,
        POSITION_THUMB_PRESSED,
        VOLUME_THUMB,
        VOLUME_THUMB_PRESSED,
        BALANCE_THUMB,
        BALANCE_THUMB_PRESSED,
    ];
}

/// Volume and balance backgrounds have 28 frames, 15 px apart, each 13 px tall.
pub const SLIDER_FRAMES: i32 = 28;

/// Volume background for `level` in 0.0..=1.0.
pub fn volume_frame(level: f32) -> Sprite {
    s(Sheet::Volume, 0, frame_index(level) * 15, 68, 13)
}

/// Balance background for `balance` in -1.0..=1.0. Both directions use the same frames,
/// counting up from the center.
pub fn balance_frame(balance: f32) -> Sprite {
    s(Sheet::Balance, 9, frame_index(balance.abs()) * 15, 38, 13)
}

fn frame_index(v: f32) -> i32 {
    let v = if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((v * (SLIDER_FRAMES - 1) as f32).round() as i32).clamp(0, SLIDER_FRAMES - 1)
}

/// Big time-display digits. `NUMS_EX.BMP` adds a minus sign; plain `NUMBERS.BMP` has a 5x1
/// minus drawn inside the "2" cell.
pub mod numbers {
    use super::{Sheet, Sprite, s};

    pub const DIGIT_W: i32 = 9;
    pub const DIGIT_H: i32 = 13;

    pub fn digit(sheet: Sheet, d: u8) -> Sprite {
        s(sheet, i32::from(d.min(9)) * DIGIT_W, 0, DIGIT_W, DIGIT_H)
    }

    /// Blank cell (no minus) for NUMS_EX, or the 5x1 bar location for NUMBERS.
    pub fn no_minus(sheet: Sheet) -> Sprite {
        match sheet {
            Sheet::NumsEx => s(sheet, 90, 0, 9, 13),
            _ => s(sheet, 9, 6, 5, 1),
        }
    }

    pub fn minus(sheet: Sheet) -> Sprite {
        match sheet {
            Sheet::NumsEx => s(sheet, 99, 0, 9, 13),
            _ => s(sheet, 20, 6, 5, 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_unique_lowercase_bmp() {
        let mut names: Vec<_> = Sheet::ALL.iter().map(|s| s.file_name()).collect();
        assert!(
            names
                .iter()
                .all(|n| n.ends_with(".bmp") && *n == n.to_lowercase())
        );
        names.sort();
        names.dedup();
        assert_eq!(names.len(), Sheet::ALL.len());
    }

    #[test]
    fn transport_buttons_tile_the_sheet_row() {
        let row = [
            main::PREVIOUS,
            main::PLAY,
            main::PAUSE,
            main::STOP,
            main::NEXT,
            main::EJECT,
        ];
        for pair in row.windows(2) {
            assert_eq!(pair[0].rect.x + pair[0].rect.w, pair[1].rect.x);
        }
        assert_eq!(main::EJECT.rect.x + main::EJECT.rect.w, 136);
    }

    #[test]
    fn sprites_have_positive_size() {
        assert!(
            main::ALL
                .iter()
                .all(|s| s.rect.w > 0 && s.rect.h > 0 && s.rect.x >= 0 && s.rect.y >= 0)
        );
    }

    #[test]
    fn slider_frames() {
        assert_eq!(volume_frame(0.0).rect.y, 0);
        assert_eq!(volume_frame(1.0).rect.y, 27 * 15);
        assert_eq!(volume_frame(0.5).rect.y, 14 * 15);
        assert_eq!(volume_frame(f32::NAN).rect.y, 0);
        assert_eq!(balance_frame(-1.0), balance_frame(1.0));
        assert_eq!(balance_frame(0.0).rect, Rect::new(9, 0, 38, 13));
    }

    #[test]
    fn digits() {
        assert_eq!(
            numbers::digit(Sheet::Numbers, 7).rect,
            Rect::new(63, 0, 9, 13)
        );
        assert_eq!(
            numbers::digit(Sheet::Numbers, 12).rect.x,
            81,
            "clamped to 9"
        );
        assert_eq!(numbers::minus(Sheet::NumsEx).rect.x, 99);
        assert_eq!(numbers::minus(Sheet::Numbers).rect, Rect::new(20, 6, 5, 1));
    }
}
