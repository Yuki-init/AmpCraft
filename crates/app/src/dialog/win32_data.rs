//! The data the Win32 dialogs exchange with the system: UTF-16 strings, file filters, the
//! multi-select result and an in-memory dialog template. Pure, so it is tested on every OS.

use std::path::PathBuf;

/// A NUL-terminated UTF-16 string.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// A common-dialog filter: `name (*.a;*.b)`, `*.a;*.b`, then "All files", each NUL-ended,
/// with a final NUL.
pub fn filter(name: &str, extensions: &[&str]) -> Vec<u16> {
    let patterns: Vec<String> = extensions.iter().map(|e| format!("*.{e}")).collect();
    let patterns = patterns.join(";");
    let mut out = Vec::new();
    for part in [
        format!("{name} ({patterns})"),
        patterns,
        "All files (*.*)".to_owned(),
        "*.*".to_owned(),
    ] {
        out.extend(part.encode_utf16());
        out.push(0);
    }
    out.push(0);
    out
}

/// The file buffer a common dialog filled: one full path, or with multiple selection the
/// folder followed by the file names, NUL-separated, ending at a double NUL.
pub fn chosen_files(buffer: &[u16]) -> Vec<PathBuf> {
    let parts: Vec<String> = buffer
        .split(|&c| c == 0)
        .take_while(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect();
    match parts.as_slice() {
        [] => Vec::new(),
        [one] => vec![PathBuf::from(one)],
        [dir, names @ ..] => names.iter().map(|n| PathBuf::from(dir).join(n)).collect(),
    }
}

/// A screen rectangle as (left, top, right, bottom), right and bottom exclusive.
pub type Area = (i32, i32, i32, i32);

/// The smallest area holding all of `areas`, or None when there are none.
pub fn union(areas: impl IntoIterator<Item = Area>) -> Option<Area> {
    areas
        .into_iter()
        .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
}

/// The top-left corner that centres a `w` x `h` window over `over`, moved as needed to
/// lie inside `work` (the monitor's work area). A window bigger than `work` keeps its
/// top-left corner, and so its title bar, on screen.
pub fn centered(over: Area, w: i32, h: i32, work: Area) -> (i32, i32) {
    let fit = |centre: i32, len: i32, lo: i32, hi: i32| (centre - len / 2).min(hi - len).max(lo);
    (
        fit((over.0 + over.2) / 2, w, work.0, work.2),
        fit((over.1 + over.3) / 2, h, work.1, work.3),
    )
}

/// Control ids in the text prompt.
pub const ID_OK: u16 = 1;
pub const ID_CANCEL: u16 = 2;
pub const ID_EDIT: u16 = 100;

// Window and dialog styles (WinUser.h), named here so this module needs no Windows crate.
const WS_POPUP: u32 = 0x8000_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_CAPTION: u32 = 0x00C0_0000;
const WS_BORDER: u32 = 0x0080_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const DS_SETFONT: u32 = 0x40;
const DS_MODALFRAME: u32 = 0x80;
const DS_CENTER: u32 = 0x0800;
const ES_AUTOHSCROLL: u32 = 0x80;
const BS_DEFPUSHBUTTON: u32 = 0x1;
/// Predefined control classes, as ordinals.
const BUTTON: u16 = 0x0080;
const EDIT: u16 = 0x0081;
const STATIC: u16 = 0x0082;

/// A dialog template (`DLGTEMPLATE` with its items) for a one-line text prompt: a label, an
/// edit box and OK / Cancel. Sizes are dialog units. Returned as 32-bit words because the
/// template and every item must start on a 4-byte boundary.
pub fn prompt_template(title: &str, label: &str) -> Vec<u32> {
    let mut t: Vec<u16> = Vec::new();
    let push32 = |t: &mut Vec<u16>, v: u32| {
        t.push((v & 0xffff) as u16);
        t.push((v >> 16) as u16);
    };
    let text = |t: &mut Vec<u16>, s: &str| {
        t.extend(s.encode_utf16());
        t.push(0);
    };
    // DLGTEMPLATE: style, extended style, item count, x, y, cx, cy.
    push32(
        &mut t,
        WS_POPUP | WS_CAPTION | WS_SYSMENU | DS_MODALFRAME | DS_CENTER | DS_SETFONT,
    );
    push32(&mut t, 0);
    t.extend([4, 0, 0, 240, 64]);
    // No menu, the default class, the title, then the font (DS_SETFONT).
    t.extend([0, 0]);
    text(&mut t, title);
    t.push(9);
    text(&mut t, "Segoe UI");
    let items: [(u32, [u16; 4], u16, u16, &str); 4] = [
        (0, [7, 7, 226, 10], 0xffff, STATIC, label),
        (
            WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL,
            [7, 20, 226, 14],
            ID_EDIT,
            EDIT,
            "",
        ),
        (
            WS_TABSTOP | BS_DEFPUSHBUTTON,
            [129, 42, 50, 14],
            ID_OK,
            BUTTON,
            "OK",
        ),
        (WS_TABSTOP, [183, 42, 50, 14], ID_CANCEL, BUTTON, "Cancel"),
    ];
    for (style, [x, y, cx, cy], id, class, caption) in items {
        if t.len() % 2 == 1 {
            t.push(0);
        }
        // DLGITEMTEMPLATE: style, extended style, x, y, cx, cy, id; then the class as an
        // ordinal (0xFFFF, value), the caption and no creation data.
        push32(&mut t, style | WS_CHILD | WS_VISIBLE);
        push32(&mut t, 0);
        t.extend([x, y, cx, cy, id, 0xffff, class]);
        text(&mut t, caption);
        t.push(0);
    }
    if t.len() % 2 == 1 {
        t.push(0);
    }
    t.chunks(2)
        .map(|c| u32::from(c[0]) | (u32::from(c.get(1).copied().unwrap_or(0)) << 16))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(t: &[u32]) -> Vec<u16> {
        t.iter()
            .flat_map(|w| [(*w & 0xffff) as u16, (*w >> 16) as u16])
            .collect()
    }

    #[test]
    fn filters_are_double_nul_terminated() {
        let f = String::from_utf16(&filter("Skins", &["wsz", "zip"])).unwrap();
        assert_eq!(
            f,
            "Skins (*.wsz;*.zip)\0*.wsz;*.zip\0All files (*.*)\0*.*\0\0"
        );
    }

    #[test]
    fn single_and_multiple_selections() {
        let one = wide(r"C:\Skins\classic.wsz");
        assert_eq!(
            chosen_files(&one),
            vec![PathBuf::from(r"C:\Skins\classic.wsz")]
        );
        let mut many: Vec<u16> = Vec::new();
        for s in [r"C:\Music", "a.mp3", "b é.flac"] {
            many.extend(wide(s));
        }
        many.extend([0, 0, 0]);
        assert_eq!(
            chosen_files(&many),
            vec![
                PathBuf::from(r"C:\Music").join("a.mp3"),
                PathBuf::from(r"C:\Music").join("b é.flac"),
            ]
        );
        assert!(chosen_files(&[0, 0]).is_empty(), "cancelled");
    }

    #[test]
    fn dialogs_are_centred_over_the_player() {
        // The docked 2x stack from the third Windows report, on a 2560x1392 work area.
        let work = (0, 0, 2560, 1392);
        let player = union([
            (250, 0, 800, 232),
            (250, 232, 800, 464),
            (250, 464, 800, 928),
        ]);
        assert_eq!(player, Some((250, 0, 800, 928)));
        let player = player.unwrap();
        // A message box fits beside the player's centre.
        assert_eq!(centered(player, 400, 200, work), (325, 364));
        // The 1200x720 file dialog would start left of the screen: it stops at the edge.
        assert_eq!(centered(player, 1200, 720, work), (0, 104));
        // Near the bottom-right corner it is pulled back on screen.
        assert_eq!(
            centered((2400, 1300, 2560, 1392), 400, 200, work),
            (2160, 1192)
        );
        // Too big for the screen: the top-left corner stays visible.
        assert_eq!(centered(player, 3000, 2000, work), (0, 0));
        // A monitor left of the primary one.
        let left = (-2560, 0, 0, 1392);
        assert_eq!(centered((-700, 0, -150, 928), 400, 200, left), (-625, 364));
        assert_eq!(union([]), None);
    }

    #[test]
    fn prompt_template_layout() {
        let t = words(&prompt_template("Save preset", "Name:"));
        // Header: style, extended style (two words each), then 4 items.
        let style = u32::from(t[0]) | (u32::from(t[1]) << 16);
        assert_ne!(style & DS_SETFONT, 0);
        assert_ne!(style & WS_POPUP, 0);
        assert_eq!(t[4], 4, "item count");
        // Title follows the empty menu and class.
        assert_eq!((t[9], t[10]), (0, 0));
        let title: String = String::from_utf16(&t[11..11 + 11]).unwrap();
        assert_eq!(title, "Save preset");
        // Every item starts on a 4-byte boundary: find the edit control by its id and class.
        let edit = t
            .windows(3)
            .position(|w| w == [ID_EDIT, 0xffff, EDIT])
            .unwrap();
        // The id sits 16 bytes (8 words) into its item.
        assert_eq!((edit - 8) % 2, 0, "items are DWORD aligned");
        assert!(t.windows(3).any(|w| w == [ID_OK, 0xffff, BUTTON]));
        assert!(t.windows(3).any(|w| w == [ID_CANCEL, 0xffff, BUTTON]));
    }
}
