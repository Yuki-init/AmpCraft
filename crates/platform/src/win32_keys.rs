//! Windows virtual-key codes to the X11 keysyms [`crate::Event::Key`] carries.
//!
//! Pure, so it is compiled and tested on every OS. Letters map to their lowercase keysym and
//! ignore Shift, matching the X11 backend, which reports the unshifted keysym. Punctuation
//! depends on the keyboard layout and is not mapped here; the backend will translate it with
//! `ToUnicode` once text input needs it.

/// Virtual-key codes (from `WinUser.h`), named here so this module needs no Windows crate.
mod vk {
    pub const BACK: u16 = 0x08;
    pub const TAB: u16 = 0x09;
    pub const RETURN: u16 = 0x0d;
    pub const ESCAPE: u16 = 0x1b;
    pub const SPACE: u16 = 0x20;
    pub const PRIOR: u16 = 0x21;
    pub const NEXT: u16 = 0x22;
    pub const END: u16 = 0x23;
    pub const HOME: u16 = 0x24;
    pub const LEFT: u16 = 0x25;
    pub const UP: u16 = 0x26;
    pub const RIGHT: u16 = 0x27;
    pub const DOWN: u16 = 0x28;
    pub const INSERT: u16 = 0x2d;
    pub const DELETE: u16 = 0x2e;
    pub const NUMPAD0: u16 = 0x60;
    pub const NUMPAD9: u16 = 0x69;
    pub const F1: u16 = 0x70;
    pub const F12: u16 = 0x7b;
}

/// The keysym for virtual-key `vk`, or None for keys the app does not use.
pub fn keysym(vk: u16) -> Option<u32> {
    let k = match vk {
        vk::BACK => 0xff08,
        vk::TAB => 0xff09,
        vk::RETURN => 0xff0d,
        vk::ESCAPE => 0xff1b,
        vk::SPACE => 0x0020,
        vk::PRIOR => 0xff55,
        vk::NEXT => 0xff56,
        vk::END => 0xff57,
        vk::HOME => 0xff50,
        vk::LEFT => 0xff51,
        vk::UP => 0xff52,
        vk::RIGHT => 0xff53,
        vk::DOWN => 0xff54,
        vk::INSERT => 0xff63,
        vk::DELETE => 0xffff,
        // Digits and letters share their ASCII codes with the keysyms.
        0x30..=0x39 => u32::from(vk),
        0x41..=0x5a => u32::from(vk) + 0x20,
        vk::NUMPAD0..=vk::NUMPAD9 => 0xffb0 + u32::from(vk - vk::NUMPAD0),
        vk::F1..=vk::F12 => 0xffbe + u32::from(vk - vk::F1),
        _ => return None,
    };
    Some(k)
}

#[cfg(test)]
mod tests {
    use super::keysym;
    use crate::keys;

    #[test]
    fn navigation_keys_match_the_shared_constants() {
        assert_eq!(keysym(0x1b), Some(keys::ESCAPE));
        assert_eq!(keysym(0x0d), Some(keys::RETURN));
        assert_eq!(keysym(0x20), Some(keys::SPACE));
        assert_eq!(keysym(0x25), Some(keys::LEFT));
        assert_eq!(keysym(0x26), Some(keys::UP));
        assert_eq!(keysym(0x27), Some(keys::RIGHT));
        assert_eq!(keysym(0x28), Some(keys::DOWN));
        assert_eq!(keysym(0x2e), Some(0xffff), "Delete");
        assert_eq!(keysym(0x08), Some(0xff08), "BackSpace");
        assert_eq!(keysym(0x24), Some(0xff50), "Home");
        assert_eq!(keysym(0x23), Some(0xff57), "End");
    }

    #[test]
    fn letters_are_lowercase_and_digits_ascii() {
        assert_eq!(keysym(u16::from(b'Z')), Some(u32::from(b'z')));
        assert_eq!(keysym(u16::from(b'A')), Some(u32::from(b'a')));
        assert_eq!(keysym(u16::from(b'0')), Some(u32::from(b'0')));
        assert_eq!(keysym(u16::from(b'9')), Some(u32::from(b'9')));
    }

    #[test]
    fn function_and_keypad_keys() {
        assert_eq!(keysym(0x70), Some(0xffbe), "F1");
        assert_eq!(keysym(0x7b), Some(0xffc9), "F12");
        assert_eq!(keysym(0x60), Some(0xffb0), "KP_0");
        assert_eq!(keysym(0x69), Some(0xffb9), "KP_9");
    }

    #[test]
    fn unmapped_keys() {
        // Shift, Ctrl, OEM punctuation and F13 are not keys the app handles.
        for vk in [0x10, 0x11, 0xba, 0x7c, 0x00, 0xff] {
            assert_eq!(keysym(vk), None, "{vk:#x}");
        }
    }
}
