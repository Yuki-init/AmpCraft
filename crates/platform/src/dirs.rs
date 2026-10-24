//! Where the app keeps its files and where the user's music is, following each OS's
//! conventions:
//!
//! | | config | data / cache | music |
//! |---|---|---|---|
//! | Linux | `$XDG_CONFIG_HOME/ampcraft` | XDG data / cache dirs | `XDG_MUSIC_DIR` from `user-dirs.dirs` |
//! | macOS | `~/Library/Application Support/AmpCraft` | same / `~/Library/Caches/AmpCraft` | `~/Music` |
//! | Windows | `%USERPROFILE%\.config\ampcraft` (owner's choice) | `%LOCALAPPDATA%\ampcraft` | Music known folder |
//!
//! `AMPCRAFT_CONFIG_DIR`, `AMPCRAFT_DATA_DIR`, `AMPCRAFT_CACHE_DIR` and `AMPCRAFT_MUSIC_DIR`
//! override the result (used by tests and `scripts/host-run.sh`).
//!
//! The decision logic is a pure function of [`Env`], so every OS is tested on any OS.

use std::path::{Path, PathBuf};

/// Directory name on Linux and Windows.
const APP: &str = "ampcraft";
/// Directory name on macOS, where folders are conventionally capitalized.
const APP_MAC: &str = "AmpCraft";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOs,
    Windows,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        }
    }
}

/// The facts the resolver needs, gathered from the real system by [`Env::system`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    pub home: Option<PathBuf>,
    /// Environment variables that matter (XDG_*, LOCALAPPDATA, AMPCRAFT_*).
    pub vars: Vec<(String, PathBuf)>,
    /// Contents of `user-dirs.dirs` (Linux).
    pub user_dirs: Option<String>,
    /// The Music known folder (Windows).
    pub known_music: Option<PathBuf>,
}

impl Env {
    fn var(&self, name: &str) -> Option<&Path> {
        self.vars
            .iter()
            .find(|(k, v)| k == name && !v.as_os_str().is_empty())
            .map(|(_, v)| v.as_path())
    }

    /// Reads the real environment.
    pub fn system() -> Self {
        let names = [
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "XDG_MUSIC_DIR",
            "LOCALAPPDATA",
            "USERPROFILE",
            "HOME",
            "AMPCRAFT_CONFIG_DIR",
            "AMPCRAFT_DATA_DIR",
            "AMPCRAFT_CACHE_DIR",
            "AMPCRAFT_MUSIC_DIR",
        ];
        let vars: Vec<(String, PathBuf)> = names
            .iter()
            .filter_map(|n| std::env::var_os(n).map(|v| ((*n).to_owned(), PathBuf::from(v))))
            .collect();
        let os = Os::current();
        let home = match os {
            Os::Windows => vars
                .iter()
                .find(|(k, _)| k == "USERPROFILE")
                .map(|(_, v)| v.clone()),
            _ => vars
                .iter()
                .find(|(k, _)| k == "HOME")
                .map(|(_, v)| v.clone()),
        };
        let user_dirs = if os == Os::Linux {
            let config = vars
                .iter()
                .find(|(k, _)| k == "XDG_CONFIG_HOME")
                .map(|(_, v)| v.clone())
                .or_else(|| home.as_ref().map(|h| h.join(".config")));
            config.and_then(|c| std::fs::read_to_string(c.join("user-dirs.dirs")).ok())
        } else {
            None
        };
        Self {
            home,
            vars,
            user_dirs,
            known_music: if os == Os::Windows {
                known_music_folder()
            } else {
                None
            },
        }
    }
}

/// Resolved locations. `music` is None when no music folder can be determined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirs {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
    pub music: Option<PathBuf>,
}

impl AppDirs {
    /// Locations for this machine.
    pub fn system() -> Self {
        resolve(Os::current(), &Env::system())
    }

    /// The main settings file.
    pub fn config_file(&self) -> PathBuf {
        self.config.join("config.toml")
    }
}

/// Parses `XDG_MUSIC_DIR` from a `user-dirs.dirs` file (shell syntax: `XDG_MUSIC_DIR="$HOME/Music"`).
fn xdg_music_from_user_dirs(text: &str, home: &Path) -> Option<PathBuf> {
    for line in text.lines() {
        let line = line.trim();
        let Some(value) = line.strip_prefix("XDG_MUSIC_DIR=") else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        let path = if let Some(rest) = value.strip_prefix("$HOME") {
            home.join(rest.trim_start_matches('/'))
        } else if value.starts_with('/') {
            PathBuf::from(value)
        } else {
            continue;
        };
        // xdg-user-dirs sets a disabled entry to $HOME itself.
        if path == home {
            return None;
        }
        return Some(path);
    }
    None
}

/// Picks the directories for `os` from `env`. Pure: no I/O.
pub fn resolve(os: Os, env: &Env) -> AppDirs {
    // Without a home directory, fall back to something writable and obvious.
    let home = env
        .home
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join(APP));
    let (config, data, cache, music) = match os {
        Os::Linux => {
            let config_home = env
                .var("XDG_CONFIG_HOME")
                .map_or_else(|| home.join(".config"), Path::to_path_buf);
            let data_home = env
                .var("XDG_DATA_HOME")
                .map_or_else(|| home.join(".local/share"), Path::to_path_buf);
            let cache_home = env
                .var("XDG_CACHE_HOME")
                .map_or_else(|| home.join(".cache"), Path::to_path_buf);
            let music = env
                .var("XDG_MUSIC_DIR")
                .map(Path::to_path_buf)
                .or_else(|| {
                    env.user_dirs
                        .as_deref()
                        .and_then(|t| xdg_music_from_user_dirs(t, &home))
                })
                .unwrap_or_else(|| home.join("Music"));
            (
                config_home.join(APP),
                data_home.join(APP),
                cache_home.join(APP),
                Some(music),
            )
        }
        Os::MacOs => {
            let support = home.join("Library/Application Support").join(APP_MAC);
            (
                support.clone(),
                support,
                home.join("Library/Caches").join(APP_MAC),
                Some(home.join("Music")),
            )
        }
        Os::Windows => {
            let local = env
                .var("LOCALAPPDATA")
                .map_or_else(|| home.join("AppData").join("Local"), Path::to_path_buf)
                .join(APP);
            (
                home.join(".config").join(APP),
                local.clone(),
                local.join("cache"),
                env.known_music.clone().or_else(|| Some(home.join("Music"))),
            )
        }
    };
    let pick = |var: &str, default: PathBuf| env.var(var).map_or(default, Path::to_path_buf);
    AppDirs {
        config: pick("AMPCRAFT_CONFIG_DIR", config),
        data: pick("AMPCRAFT_DATA_DIR", data),
        cache: pick("AMPCRAFT_CACHE_DIR", cache),
        music: env
            .var("AMPCRAFT_MUSIC_DIR")
            .map(Path::to_path_buf)
            .or(music),
    }
}

/// The Music known folder, which follows the user if they moved it (for example to OneDrive).
#[cfg(windows)]
// Win32 FFI is the one place unsafe is allowed until the egui port replaces it (docs/gaps.md).
#[allow(unsafe_code)]
fn known_music_folder() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Music, SHGetKnownFolderPath};

    let mut raw: windows_sys::core::PWSTR = std::ptr::null_mut();
    // SAFETY: FOLDERID_Music is a valid GUID that outlives the call, a null token means the
    // current user, and `raw` is a valid out-pointer. On success the shell allocates a
    // NUL-terminated wide string that we must free with CoTaskMemFree, which we do below
    // whether or not the call succeeded (freeing null is allowed).
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_Music, 0, std::ptr::null_mut(), &mut raw) };
    let path = if hr >= 0 && !raw.is_null() {
        // SAFETY: `raw` is a NUL-terminated UTF-16 string from the shell; we read up to the
        // terminator only.
        let wide = unsafe {
            let mut len = 0;
            while *raw.add(len) != 0 {
                len += 1;
            }
            std::slice::from_raw_parts(raw, len)
        };
        Some(PathBuf::from(std::ffi::OsString::from_wide(wide)))
    } else {
        None
    };
    // SAFETY: `raw` is null or was allocated by SHGetKnownFolderPath.
    unsafe { CoTaskMemFree(raw as *const core::ffi::c_void) };
    path
}

#[cfg(not(windows))]
fn known_music_folder() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(home: &str, vars: &[(&str, &str)]) -> Env {
        Env {
            home: Some(PathBuf::from(home)),
            vars: vars
                .iter()
                .map(|(k, v)| ((*k).to_owned(), PathBuf::from(v)))
                .collect(),
            user_dirs: None,
            known_music: None,
        }
    }

    #[test]
    fn linux_defaults() {
        let d = resolve(Os::Linux, &env("/home/u", &[]));
        assert_eq!(d.config, PathBuf::from("/home/u/.config/ampcraft"));
        assert_eq!(d.data, PathBuf::from("/home/u/.local/share/ampcraft"));
        assert_eq!(d.cache, PathBuf::from("/home/u/.cache/ampcraft"));
        assert_eq!(d.music, Some(PathBuf::from("/home/u/Music")));
        assert_eq!(
            d.config_file(),
            PathBuf::from("/home/u/.config/ampcraft/config.toml")
        );
    }

    #[test]
    fn linux_xdg_variables_win() {
        let d = resolve(
            Os::Linux,
            &env(
                "/home/u",
                &[
                    ("XDG_CONFIG_HOME", "/cfg"),
                    ("XDG_DATA_HOME", "/data"),
                    ("XDG_CACHE_HOME", ""),
                ],
            ),
        );
        assert_eq!(d.config, PathBuf::from("/cfg/ampcraft"));
        assert_eq!(d.data, PathBuf::from("/data/ampcraft"));
        assert_eq!(
            d.cache,
            PathBuf::from("/home/u/.cache/ampcraft"),
            "empty means unset"
        );
    }

    #[test]
    fn linux_music_from_user_dirs() {
        let mut e = env("/home/u", &[]);
        e.user_dirs = Some(
            "# comment\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_MUSIC_DIR=\"$HOME/Musik\"\n".into(),
        );
        assert_eq!(
            resolve(Os::Linux, &e).music,
            Some(PathBuf::from("/home/u/Musik"))
        );
        e.user_dirs = Some("XDG_MUSIC_DIR=\"/mnt/audio\"\n".into());
        assert_eq!(
            resolve(Os::Linux, &e).music,
            Some(PathBuf::from("/mnt/audio"))
        );
        // Disabled entries point at $HOME; fall back to ~/Music.
        e.user_dirs = Some("XDG_MUSIC_DIR=\"$HOME/\"\n".into());
        assert_eq!(
            resolve(Os::Linux, &e).music,
            Some(PathBuf::from("/home/u/Music"))
        );
    }

    #[test]
    fn macos_library_folders() {
        let d = resolve(Os::MacOs, &env("/Users/u", &[]));
        assert_eq!(
            d.config,
            PathBuf::from("/Users/u/Library/Application Support/AmpCraft")
        );
        assert_eq!(d.data, d.config);
        assert_eq!(d.cache, PathBuf::from("/Users/u/Library/Caches/AmpCraft"));
        assert_eq!(d.music, Some(PathBuf::from("/Users/u/Music")));
    }

    #[test]
    fn windows_config_in_dot_config_and_known_music() {
        let mut e = env(
            "C:/Users/u",
            &[("LOCALAPPDATA", "C:/Users/u/AppData/Local")],
        );
        e.known_music = Some(PathBuf::from("D:/OneDrive/Music"));
        let d = resolve(Os::Windows, &e);
        assert_eq!(d.config, PathBuf::from("C:/Users/u/.config/ampcraft"));
        assert_eq!(d.data, PathBuf::from("C:/Users/u/AppData/Local/ampcraft"));
        assert_eq!(
            d.cache,
            PathBuf::from("C:/Users/u/AppData/Local/ampcraft/cache")
        );
        assert_eq!(d.music, Some(PathBuf::from("D:/OneDrive/Music")));
        e.known_music = None;
        assert_eq!(
            resolve(Os::Windows, &e).music,
            Some(PathBuf::from("C:/Users/u/Music"))
        );
    }

    #[test]
    fn overrides_apply_on_every_os() {
        for os in [Os::Linux, Os::MacOs, Os::Windows] {
            let d = resolve(
                os,
                &env(
                    "/h",
                    &[
                        ("AMPCRAFT_CONFIG_DIR", "/o/c"),
                        ("AMPCRAFT_DATA_DIR", "/o/d"),
                        ("AMPCRAFT_CACHE_DIR", "/o/x"),
                        ("AMPCRAFT_MUSIC_DIR", "/o/m"),
                    ],
                ),
            );
            assert_eq!(
                d,
                AppDirs {
                    config: "/o/c".into(),
                    data: "/o/d".into(),
                    cache: "/o/x".into(),
                    music: Some("/o/m".into()),
                },
                "{os:?}"
            );
        }
    }

    #[test]
    fn missing_home_does_not_panic() {
        let d = resolve(Os::Linux, &Env::default());
        assert!(d.config.ends_with("ampcraft/ampcraft") || d.config.ends_with("ampcraft"));
    }
}
