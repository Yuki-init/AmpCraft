//! The desktop's color scheme, for the "System" theme.
//!
//! - KDE Plasma: the full palette from `kdeglobals` (window, view, button and selection
//!   colors), so the player matches the Plasma color scheme in use.
//! - Other Linux desktops: light or dark and the accent color from the XDG desktop portal
//!   (`org.freedesktop.appearance`), painted onto neutral greys.
//! - Windows: light or dark (`AppsUseLightTheme`) and the accent color from the registry.
//!
//! `AMPCRAFT_COLOR_SCHEME=dark|light` overrides the desktop (neutral greys), for tests.
//! [`Watcher`] reports changes so the theme follows the desktop while the player runs.

use std::sync::mpsc::{self, Receiver};

use ampcraft_skin::theme::{Palette, Theme};

/// What the desktop says about its colors. Anything it does not say is `None`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Desktop {
    /// Prefers dark (`true`) or light (`false`).
    pub dark: Option<bool>,
    /// Accent color, `0xRRGGBB`.
    pub accent: Option<u32>,
    /// A full palette from the desktop's color scheme (KDE).
    pub palette: Option<Palette>,
}

impl Desktop {
    /// Reads the desktop's current colors. Never blocks for more than about half a second.
    pub fn read() -> Desktop {
        match std::env::var("AMPCRAFT_COLOR_SCHEME").as_deref() {
            Ok("dark") => Desktop {
                dark: Some(true),
                ..Desktop::default()
            },
            Ok("light") => Desktop {
                dark: Some(false),
                ..Desktop::default()
            },
            _ => sys::read(),
        }
    }

    /// The "System" theme for these colors: the desktop's palette when it has one, else
    /// neutral greys (dark when the desktop does not say) with its accent color.
    pub fn theme(&self) -> Theme {
        let palette = self
            .palette
            .unwrap_or_else(|| Palette::neutral(self.dark.unwrap_or(true), self.accent));
        Theme::system(palette)
    }
}

/// Builds a palette from a KDE `kdeglobals` file. `None` when it has no color scheme.
pub fn kde_palette(kdeglobals: &str) -> Option<Palette> {
    let get = |group: &str, key: &str| ini_color(kdeglobals, group, key);
    let window = get("Colors:Window", "BackgroundNormal")?;
    let view = get("Colors:View", "BackgroundNormal")?;
    let text = get("Colors:Window", "ForegroundNormal")
        .or_else(|| get("Colors:View", "ForegroundNormal"))?;
    let dark = luma(text) > luma(window);
    let fallback = Palette::neutral(dark, None);
    let accent = get("Colors:Selection", "BackgroundNormal")
        .or_else(|| get("Colors:View", "DecorationFocus"))
        .unwrap_or(fallback.accent);
    let orange = get("Colors:View", "ForegroundNeutral").unwrap_or(fallback.orange);
    Some(Palette {
        crust: view,
        base: window,
        surface: get("Colors:Button", "BackgroundNormal").unwrap_or(window),
        overlay: mix(window, text, 0.3),
        text,
        subtext: get("Colors:Window", "ForegroundInactive")
            .unwrap_or_else(|| mix(window, text, 0.65)),
        accent,
        selection: get("Colors:Selection", "BackgroundAlternate")
            .unwrap_or_else(|| mix(window, accent, 0.35)),
        red: get("Colors:View", "ForegroundNegative").unwrap_or(fallback.red),
        orange,
        yellow: mix(orange, fallback.yellow, 0.5),
        green: get("Colors:View", "ForegroundPositive").unwrap_or(fallback.green),
        cyan: get("Colors:View", "ForegroundLink").unwrap_or(fallback.cyan),
    })
}

/// An `R,G,B` (or `R,G,B,A`) value from `[group] key=...` in an INI-style file, as `0xAARRGGBB`.
fn ini_color(text: &str, group: &str, key: &str) -> Option<u32> {
    let mut in_group = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_group = name == group;
        } else if in_group
            && let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            let mut parts = v.split(',').map(|p| p.trim().parse::<u8>().ok());
            let (r, g, b) = (parts.next()??, parts.next()??, parts.next()??);
            return Some(0xFF00_0000 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b));
        }
    }
    None
}

fn mix(a: u32, b: u32, t: f32) -> u32 {
    let ch = |s: u32| {
        let (x, y) = (((a >> s) & 0xFF) as f32, ((b >> s) & 0xFF) as f32);
        ((x + (y - x) * t).round().clamp(0.0, 255.0) as u32) << s
    };
    0xFF00_0000 | ch(16) | ch(8) | ch(0)
}

/// Rough perceived brightness, 0 to 1 (enough to tell light from dark).
fn luma(c: u32) -> f32 {
    let ch = |s: u32| ((c >> s) & 0xFF) as f32 / 255.0;
    0.2126 * ch(16) + 0.7152 * ch(8) + 0.0722 * ch(0)
}

/// Watches the desktop's colors in the background.
#[derive(Debug)]
pub struct Watcher {
    rx: Receiver<Desktop>,
}

impl Watcher {
    /// Starts watching. Where the platform has no way to watch, it never reports anything.
    pub fn start() -> Watcher {
        let (tx, rx) = mpsc::channel();
        if std::env::var_os("AMPCRAFT_COLOR_SCHEME").is_none() {
            sys::watch(tx);
        }
        Watcher { rx }
    }

    /// The desktop's colors, if they changed since the last call.
    pub fn changed(&self) -> Option<Desktop> {
        self.rx.try_iter().last()
    }
}

/// Runs `poke`-driven re-reads: after each burst of pokes settles, reads the desktop and
/// sends it when it differs from the last reading.
#[cfg(any(target_os = "linux", windows))]
fn spawn_reader(pokes: Receiver<()>, tx: mpsc::Sender<Desktop>) {
    use std::time::Duration;
    let run = move || {
        let mut last = sys::read();
        while pokes.recv().is_ok() {
            // Desktops change several settings at once; wait for the burst to end.
            while pokes.recv_timeout(Duration::from_millis(300)).is_ok() {}
            let now = sys::read();
            if now != last {
                last = now;
                if tx.send(now).is_err() {
                    return;
                }
            }
        }
    };
    if let Err(e) = std::thread::Builder::new()
        .name("appearance".into())
        .spawn(run)
    {
        log::warn!("cannot watch the desktop colors: {e}");
    }
}

#[cfg(target_os = "linux")]
mod sys {
    use std::path::PathBuf;
    use std::sync::mpsc::{self, Sender};
    use std::time::{Duration, SystemTime};

    use zbus::blocking::{Connection, MessageIterator};
    use zbus::zvariant::{OwnedValue, Value};

    use super::{Desktop, kde_palette};

    const PORTAL: &str = "org.freedesktop.portal.Desktop";
    const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
    const SETTINGS: &str = "org.freedesktop.portal.Settings";
    const APPEARANCE: &str = "org.freedesktop.appearance";

    pub(super) fn read() -> Desktop {
        let mut d = Desktop::default();
        // The portal may need starting, or hang; never hold the player up for it.
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("appearance-read".into())
            .spawn(move || {
                let _ = tx.send(read_portal());
            });
        if spawned.is_ok()
            && let Ok(Some((dark, accent))) = rx.recv_timeout(Duration::from_millis(500))
        {
            d.dark = dark;
            d.accent = accent;
        }
        if is_kde() {
            d.palette = kdeglobals()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .and_then(|t| kde_palette(&t));
            if let Some(p) = d.palette {
                d.dark = Some(p.is_dark());
            }
        }
        d
    }

    fn read_portal() -> Option<(Option<bool>, Option<u32>)> {
        let conn = Connection::session().ok()?;
        // color-scheme: 0 no preference, 1 dark, 2 light.
        let dark = setting(&conn, "color-scheme").and_then(|v| match u32::try_from(v) {
            Ok(1) => Some(true),
            Ok(2) => Some(false),
            _ => None,
        });
        let accent = setting(&conn, "accent-color").and_then(|v| accent(&v));
        Some((dark, accent))
    }

    fn setting(conn: &Connection, key: &str) -> Option<OwnedValue> {
        let reply = conn
            .call_method(
                Some(PORTAL),
                PORTAL_PATH,
                Some(SETTINGS),
                "ReadOne",
                &(APPEARANCE, key),
            )
            .or_else(|_| {
                conn.call_method(
                    Some(PORTAL),
                    PORTAL_PATH,
                    Some(SETTINGS),
                    "Read",
                    &(APPEARANCE, key),
                )
            })
            .ok()?;
        let value: OwnedValue = reply.body().deserialize().ok()?;
        unwrap_variant(value)
    }

    /// Older portals answer `Read` with a variant inside a variant.
    fn unwrap_variant(v: OwnedValue) -> Option<OwnedValue> {
        match &*v {
            Value::Value(inner) => OwnedValue::try_from(inner.as_ref().try_clone().ok()?).ok(),
            _ => Some(v),
        }
    }

    /// accent-color is `(ddd)`, each 0 to 1; anything outside that means "not set".
    fn accent(v: &Value<'_>) -> Option<u32> {
        let Value::Structure(s) = v else { return None };
        let mut out = 0u32;
        let fields = s.fields();
        if fields.len() != 3 {
            return None;
        }
        for f in fields {
            let Value::F64(x) = f else { return None };
            if !(0.0..=1.0).contains(x) {
                return None;
            }
            out = out << 8 | (x * 255.0).round() as u32;
        }
        Some(out)
    }

    fn is_kde() -> bool {
        std::env::var("XDG_CURRENT_DESKTOP")
            .is_ok_and(|d| d.split(':').any(|p| p.eq_ignore_ascii_case("KDE")))
    }

    fn kdeglobals() -> Option<PathBuf> {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(config.join("kdeglobals"))
    }

    pub(super) fn watch(tx: Sender<Desktop>) {
        let (poke, pokes) = mpsc::channel();
        super::spawn_reader(pokes, tx);
        // Portal signals: light/dark and accent on every desktop, color schemes on KDE.
        let signals = poke.clone();
        let _ = std::thread::Builder::new()
            .name("appearance-portal".into())
            .spawn(move || {
                let Ok(conn) = Connection::session() else {
                    return;
                };
                let rule = format!("type='signal',interface='{SETTINGS}',member='SettingChanged'");
                let Ok(messages) = MessageIterator::for_match_rule(rule.as_str(), &conn, Some(64))
                else {
                    return;
                };
                for msg in messages.flatten() {
                    let ns: Option<(String, String, OwnedValue)> = msg.body().deserialize().ok();
                    if ns.is_some_and(|(ns, _, _)| {
                        ns == APPEARANCE || ns.starts_with("org.kde.kdeglobals")
                    }) && signals.send(()).is_err()
                    {
                        return;
                    }
                }
            });
        // KDE also rewrites kdeglobals; watch its modification time as a fallback.
        if is_kde()
            && let Some(path) = kdeglobals()
        {
            let _ = std::thread::Builder::new()
                .name("appearance-kde".into())
                .spawn(move || {
                    let modified = || std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                    let mut last: Option<SystemTime> = modified();
                    loop {
                        std::thread::sleep(Duration::from_secs(2));
                        let now = modified();
                        if now != last {
                            last = now;
                            if poke.send(()).is_err() {
                                return;
                            }
                        }
                    }
                });
        }
    }
}

#[cfg(windows)]
// Registry reads are Win32 FFI, the one place unsafe is allowed (docs/gaps.md).
#[allow(unsafe_code)]
mod sys {
    use std::sync::mpsc::{self, Sender};
    use std::time::Duration;

    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};

    use super::Desktop;

    const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
    const DWM: &str = r"Software\Microsoft\Windows\DWM";

    pub(super) fn read() -> Desktop {
        Desktop {
            dark: dword(PERSONALIZE, "AppsUseLightTheme").map(|light| light == 0),
            // AccentColor is 0xAABBGGRR.
            accent: dword(DWM, "AccentColor")
                .map(|c| (c & 0xFF) << 16 | (c & 0xFF00) | (c >> 16 & 0xFF)),
            palette: None,
        }
    }

    fn dword(key: &str, value: &str) -> Option<u32> {
        let key: Vec<u16> = key.encode_utf16().chain([0]).collect();
        let value: Vec<u16> = value.encode_utf16().chain([0]).collect();
        let mut data = 0u32;
        let mut size = 4u32;
        // SAFETY: both names are NUL-terminated UTF-16 that outlive the call, `data` is a
        // writable 4-byte buffer whose size is passed in `size`, and the type out-pointer
        // may be null.
        let rc = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&raw mut data).cast(),
                &raw mut size,
            )
        };
        (rc == 0).then_some(data)
    }

    pub(super) fn watch(tx: Sender<Desktop>) {
        let (poke, pokes) = mpsc::channel();
        super::spawn_reader(pokes, tx);
        // The registry values are cheap to read; poll them.
        let _ = std::thread::Builder::new()
            .name("appearance-poll".into())
            .spawn(move || {
                let mut last = read();
                loop {
                    std::thread::sleep(Duration::from_secs(2));
                    let now = read();
                    if now != last {
                        last = now;
                        if poke.send(()).is_err() {
                            return;
                        }
                    }
                }
            });
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod sys {
    use std::sync::mpsc::Sender;

    use super::Desktop;

    pub(super) fn read() -> Desktop {
        Desktop::default()
    }

    pub(super) fn watch(_tx: Sender<Desktop>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from KDE's Breeze Light scheme.
    const BREEZE_LIGHT: &str = "[General]\nColorScheme=BreezeLight\n\n\
        [Colors:Button]\nBackgroundNormal=252,252,252\n\n\
        [Colors:Selection]\nBackgroundAlternate=163,212,250\nBackgroundNormal=61,174,233\n\n\
        [Colors:View]\nBackgroundNormal=255,255,255\nForegroundNormal=35,38,41\n\
        ForegroundNegative=218,68,83\nForegroundNeutral=246,116,0\nForegroundPositive=39,174,96\n\
        ForegroundLink=41,128,185\n\n\
        [Colors:Window]\nBackgroundNormal=239,240,241\nForegroundInactive=112,125,138\n\
        ForegroundNormal=35,38,41\n";

    #[test]
    fn breeze_light_becomes_a_light_palette_with_its_own_colors() {
        let p = kde_palette(BREEZE_LIGHT).unwrap();
        assert!(!p.is_dark());
        assert_eq!(p.base, 0xFFEF_F0F1);
        assert_eq!(p.crust, 0xFFFF_FFFF);
        assert_eq!(p.surface, 0xFFFC_FCFC);
        assert_eq!(p.text, 0xFF23_2629);
        assert_eq!(p.selection, 0xFFA3_D4FA);
        assert_eq!(p.red, 0xFFDA_4453);
        assert_eq!(p.green, 0xFF27_AE60);
        // The display color starts as the selection blue, darkened to be readable on white.
        let t = Desktop {
            palette: Some(p),
            ..Desktop::default()
        }
        .theme();
        assert!(t.display_contrast() >= 4.5);
        assert_ne!(t.palette.accent, p.accent);
    }

    #[test]
    fn dark_schemes_are_dark_and_missing_groups_give_nothing() {
        let dark = "[Colors:Window]\nBackgroundNormal=32,35,38\nForegroundNormal=252,252,252\n\
            [Colors:View]\nBackgroundNormal=20,22,24\n";
        let p = kde_palette(dark).unwrap();
        assert!(p.is_dark());
        assert_eq!(p.surface, p.base);
        assert_eq!(kde_palette("[General]\nName=x\n"), None);
        assert_eq!(
            kde_palette("[Colors:Window]\nBackgroundNormal=junk\n"),
            None
        );
    }

    #[test]
    fn without_a_palette_the_theme_follows_light_dark_and_accent() {
        let light = Desktop {
            dark: Some(false),
            accent: Some(0x3daee9),
            palette: None,
        };
        let t = light.theme();
        assert!(!t.palette.is_dark());
        assert!(t.display_contrast() >= 4.5);
        let unknown = Desktop::default().theme();
        assert!(
            unknown.palette.is_dark(),
            "no preference keeps the old dark default"
        );
    }
}
