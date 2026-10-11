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
    pub const CLUTTER_O_ON: Sprite = s(Titlebar, 304, 47, 8, 8);
    pub const CLUTTER_A_ON: Sprite = s(Titlebar, 312, 55, 8, 7);
    pub const CLUTTER_I_ON: Sprite = s(Titlebar, 320, 62, 8, 7);
    pub const CLUTTER_D_ON: Sprite = s(Titlebar, 328, 69, 8, 8);
    pub const CLUTTER_V_ON: Sprite = s(Titlebar, 336, 77, 8, 7);
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
        CLUTTER_O_ON,
        CLUTTER_A_ON,
        CLUTTER_I_ON,
        CLUTTER_D_ON,
        CLUTTER_V_ON,
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

/// Playlist window sprites (PLEDIT.BMP). The frame is tiled to any size.
pub mod playlist {
    use super::{Sheet::Pledit, Sprite, s};

    pub const TOP_LEFT: Sprite = s(Pledit, 0, 21, 25, 20);
    pub const TOP_LEFT_ACTIVE: Sprite = s(Pledit, 0, 0, 25, 20);
    pub const TITLE: Sprite = s(Pledit, 26, 21, 100, 20);
    pub const TITLE_ACTIVE: Sprite = s(Pledit, 26, 0, 100, 20);
    pub const TOP_TILE: Sprite = s(Pledit, 127, 21, 25, 20);
    pub const TOP_TILE_ACTIVE: Sprite = s(Pledit, 127, 0, 25, 20);
    pub const TOP_RIGHT: Sprite = s(Pledit, 153, 21, 25, 20);
    pub const TOP_RIGHT_ACTIVE: Sprite = s(Pledit, 153, 0, 25, 20);
    pub const LEFT_TILE: Sprite = s(Pledit, 0, 42, 12, 29);
    pub const RIGHT_TILE: Sprite = s(Pledit, 31, 42, 20, 29);
    pub const BOTTOM_TILE: Sprite = s(Pledit, 179, 0, 25, 38);
    pub const BOTTOM_LEFT: Sprite = s(Pledit, 0, 72, 125, 38);
    pub const BOTTOM_RIGHT: Sprite = s(Pledit, 126, 72, 150, 38);
    pub const VISUALIZER_BACKGROUND: Sprite = s(Pledit, 205, 0, 75, 38);
    pub const SCROLL_HANDLE: Sprite = s(Pledit, 52, 53, 8, 18);
    pub const SCROLL_HANDLE_PRESSED: Sprite = s(Pledit, 61, 53, 8, 18);
    pub const CLOSE_PRESSED: Sprite = s(Pledit, 52, 42, 9, 9);
    pub const SHADE_PRESSED: Sprite = s(Pledit, 62, 42, 9, 9);
    // Bottom-bar button menus: each item normal and selected (23 px to the right), stacked
    // upward from the button, with a 3 px bar on their left.
    pub const ADD_URL: Sprite = s(Pledit, 0, 111, 22, 18);
    pub const ADD_URL_SELECTED: Sprite = s(Pledit, 23, 111, 22, 18);
    pub const ADD_DIR: Sprite = s(Pledit, 0, 130, 22, 18);
    pub const ADD_DIR_SELECTED: Sprite = s(Pledit, 23, 130, 22, 18);
    pub const ADD_FILE: Sprite = s(Pledit, 0, 149, 22, 18);
    pub const ADD_FILE_PRESSED: Sprite = s(Pledit, 23, 149, 22, 18);
    pub const ADD_BAR: Sprite = s(Pledit, 48, 111, 3, 54);
    pub const REMOVE_ALL: Sprite = s(Pledit, 54, 111, 22, 18);
    pub const REMOVE_ALL_SELECTED: Sprite = s(Pledit, 77, 111, 22, 18);
    pub const REMOVE_CROP: Sprite = s(Pledit, 54, 130, 22, 18);
    pub const REMOVE_CROP_SELECTED: Sprite = s(Pledit, 77, 130, 22, 18);
    pub const REMOVE_SELECTED: Sprite = s(Pledit, 54, 149, 22, 18);
    pub const REMOVE_SELECTED_PRESSED: Sprite = s(Pledit, 77, 149, 22, 18);
    pub const REMOVE_MISC: Sprite = s(Pledit, 54, 168, 22, 18);
    pub const REMOVE_MISC_SELECTED: Sprite = s(Pledit, 77, 168, 22, 18);
    pub const REMOVE_BAR: Sprite = s(Pledit, 100, 111, 3, 72);
    pub const SELECT_INVERT: Sprite = s(Pledit, 104, 111, 22, 18);
    pub const SELECT_INVERT_SELECTED: Sprite = s(Pledit, 127, 111, 22, 18);
    pub const SELECT_NONE: Sprite = s(Pledit, 104, 130, 22, 18);
    pub const SELECT_NONE_SELECTED: Sprite = s(Pledit, 127, 130, 22, 18);
    pub const SELECT_ALL: Sprite = s(Pledit, 104, 149, 22, 18);
    pub const SELECT_ALL_PRESSED: Sprite = s(Pledit, 127, 149, 22, 18);
    pub const SELECT_BAR: Sprite = s(Pledit, 150, 111, 3, 54);
    pub const MISC_SORT: Sprite = s(Pledit, 154, 111, 22, 18);
    pub const MISC_SORT_SELECTED: Sprite = s(Pledit, 177, 111, 22, 18);
    pub const MISC_INFO: Sprite = s(Pledit, 154, 130, 22, 18);
    pub const MISC_INFO_SELECTED: Sprite = s(Pledit, 177, 130, 22, 18);
    pub const MISC_OPTIONS: Sprite = s(Pledit, 154, 149, 22, 18);
    pub const MISC_OPTIONS_PRESSED: Sprite = s(Pledit, 177, 149, 22, 18);
    pub const MISC_BAR: Sprite = s(Pledit, 200, 111, 3, 54);
    pub const LIST_NEW: Sprite = s(Pledit, 204, 111, 22, 18);
    pub const LIST_NEW_SELECTED: Sprite = s(Pledit, 227, 111, 22, 18);
    pub const LIST_SAVE: Sprite = s(Pledit, 204, 130, 22, 18);
    pub const LIST_SAVE_SELECTED: Sprite = s(Pledit, 227, 130, 22, 18);
    pub const LOAD_LIST: Sprite = s(Pledit, 204, 149, 22, 18);
    pub const LOAD_LIST_PRESSED: Sprite = s(Pledit, 227, 149, 22, 18);
    pub const LIST_BAR: Sprite = s(Pledit, 250, 111, 3, 54);
    pub const SHADE_LEFT: Sprite = s(Pledit, 72, 42, 25, 14);
    pub const SHADE_TILE: Sprite = s(Pledit, 72, 57, 25, 14);
    pub const SHADE_RIGHT: Sprite = s(Pledit, 99, 57, 50, 14);
    pub const SHADE_RIGHT_ACTIVE: Sprite = s(Pledit, 99, 42, 50, 14);
    pub const UNSHADE_PRESSED: Sprite = s(Pledit, 150, 42, 9, 9);

    pub const ALL: &[Sprite] = &[
        TOP_LEFT,
        TOP_LEFT_ACTIVE,
        TITLE,
        TITLE_ACTIVE,
        TOP_TILE,
        TOP_TILE_ACTIVE,
        TOP_RIGHT,
        TOP_RIGHT_ACTIVE,
        LEFT_TILE,
        RIGHT_TILE,
        BOTTOM_TILE,
        BOTTOM_LEFT,
        BOTTOM_RIGHT,
        VISUALIZER_BACKGROUND,
        SCROLL_HANDLE,
        SCROLL_HANDLE_PRESSED,
        CLOSE_PRESSED,
        SHADE_PRESSED,
        ADD_URL,
        ADD_URL_SELECTED,
        ADD_DIR,
        ADD_DIR_SELECTED,
        ADD_FILE,
        ADD_FILE_PRESSED,
        ADD_BAR,
        REMOVE_ALL,
        REMOVE_ALL_SELECTED,
        REMOVE_CROP,
        REMOVE_CROP_SELECTED,
        REMOVE_SELECTED,
        REMOVE_SELECTED_PRESSED,
        REMOVE_MISC,
        REMOVE_MISC_SELECTED,
        REMOVE_BAR,
        SELECT_INVERT,
        SELECT_INVERT_SELECTED,
        SELECT_NONE,
        SELECT_NONE_SELECTED,
        SELECT_ALL,
        SELECT_ALL_PRESSED,
        SELECT_BAR,
        MISC_SORT,
        MISC_SORT_SELECTED,
        MISC_INFO,
        MISC_INFO_SELECTED,
        MISC_OPTIONS,
        MISC_OPTIONS_PRESSED,
        MISC_BAR,
        LIST_NEW,
        LIST_NEW_SELECTED,
        LIST_SAVE,
        LIST_SAVE_SELECTED,
        LOAD_LIST,
        LOAD_LIST_PRESSED,
        LIST_BAR,
    ];
}

/// Equalizer window sprites (EQMAIN.BMP).
pub mod eq {
    use super::{Sheet::EqMain, Sprite, s};

    pub const BACKGROUND: Sprite = s(EqMain, 0, 0, 275, 116);
    pub const TITLE_BAR: Sprite = s(EqMain, 0, 149, 275, 14);
    pub const TITLE_BAR_ACTIVE: Sprite = s(EqMain, 0, 134, 275, 14);
    pub const CLOSE: Sprite = s(EqMain, 0, 116, 9, 9);
    pub const CLOSE_PRESSED: Sprite = s(EqMain, 0, 125, 9, 9);
    pub const ON: Sprite = s(EqMain, 10, 119, 26, 12);
    pub const ON_PRESSED: Sprite = s(EqMain, 128, 119, 26, 12);
    pub const ON_ACTIVE: Sprite = s(EqMain, 69, 119, 26, 12);
    pub const ON_ACTIVE_PRESSED: Sprite = s(EqMain, 187, 119, 26, 12);
    pub const AUTO: Sprite = s(EqMain, 36, 119, 32, 12);
    pub const AUTO_PRESSED: Sprite = s(EqMain, 154, 119, 32, 12);
    pub const AUTO_ACTIVE: Sprite = s(EqMain, 95, 119, 32, 12);
    pub const AUTO_ACTIVE_PRESSED: Sprite = s(EqMain, 213, 119, 32, 12);
    pub const PRESETS: Sprite = s(EqMain, 224, 164, 44, 12);
    pub const PRESETS_PRESSED: Sprite = s(EqMain, 224, 176, 44, 12);
    pub const SLIDER_THUMB: Sprite = s(EqMain, 0, 164, 11, 11);
    pub const SLIDER_THUMB_PRESSED: Sprite = s(EqMain, 0, 176, 11, 11);
    pub const GRAPH_BACKGROUND: Sprite = s(EqMain, 0, 294, 113, 19);
    pub const GRAPH_LINE_COLORS: Sprite = s(EqMain, 115, 294, 1, 19);
    pub const PREAMP_LINE: Sprite = s(EqMain, 0, 314, 113, 1);

    /// Window-shade bar (EQ_EX.BMP).
    pub const SHADE_BACKGROUND_ACTIVE: Sprite = s(super::Sheet::EqEx, 0, 0, 275, 14);
    pub const SHADE_BACKGROUND: Sprite = s(super::Sheet::EqEx, 0, 15, 275, 14);
    pub const SHADE_UNSHADE_PRESSED: Sprite = s(super::Sheet::EqEx, 1, 38, 9, 9);
    pub const SHADE_CLOSE: Sprite = s(super::Sheet::EqEx, 11, 38, 9, 9);
    pub const SHADE_CLOSE_PRESSED: Sprite = s(super::Sheet::EqEx, 11, 47, 9, 9);
    /// Shade-bar slider thumbs for low, middle and high values.
    pub const SHADE_VOLUME_THUMBS: [Sprite; 3] = [
        s(super::Sheet::EqEx, 1, 30, 3, 7),
        s(super::Sheet::EqEx, 4, 30, 3, 7),
        s(super::Sheet::EqEx, 7, 30, 3, 7),
    ];
    pub const SHADE_BALANCE_THUMBS: [Sprite; 3] = [
        s(super::Sheet::EqEx, 11, 30, 3, 7),
        s(super::Sheet::EqEx, 14, 30, 3, 7),
        s(super::Sheet::EqEx, 17, 30, 3, 7),
    ];

    pub const SLIDER_W: i32 = 14;
    pub const SLIDER_H: i32 = 63;

    /// Slider background for `level` in 0.0 (bottom, -12 dB) ..= 1.0 (top, +12 dB): 28
    /// frames in two rows of 14, 15 px apart horizontally and 65 px vertically.
    pub fn slider_frame(level: f32) -> Sprite {
        let n = super::frame_index(level);
        s(
            EqMain,
            13 + (n % 14) * 15,
            164 + (n / 14) * 65,
            SLIDER_W,
            SLIDER_H,
        )
    }

    pub const ALL: &[Sprite] = &[
        BACKGROUND,
        TITLE_BAR,
        TITLE_BAR_ACTIVE,
        CLOSE,
        CLOSE_PRESSED,
        ON,
        ON_PRESSED,
        ON_ACTIVE,
        ON_ACTIVE_PRESSED,
        AUTO,
        AUTO_PRESSED,
        AUTO_ACTIVE,
        AUTO_ACTIVE_PRESSED,
        PRESETS,
        PRESETS_PRESSED,
        SLIDER_THUMB,
        SLIDER_THUMB_PRESSED,
        GRAPH_BACKGROUND,
        GRAPH_LINE_COLORS,
        PREAMP_LINE,
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
