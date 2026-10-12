//! Which skin the player starts with, and cycling through the built-in themes.

use std::path::{Path, PathBuf};

use amp_skin::theme::{THEMES, find};

/// The local copy of the stock skin that development checkouts may have (never shipped).
pub const LOCAL_BASE_SKIN: &str = "skins/base-2.91.wsz";

/// The skin to use when none was chosen: the local base skin when there is one, else the
/// first built-in theme, by name. Always a value, so the app knows what is showing.
pub fn default_skin(local_base_exists: bool) -> PathBuf {
    if local_base_exists {
        PathBuf::from(LOCAL_BASE_SKIN)
    } else {
        PathBuf::from(THEMES[0].id)
    }
}

/// The built-in theme `skin` names, if it is one (a path that exists is a file, not a theme).
pub fn theme_index(skin: &Path) -> Option<usize> {
    if skin.exists() {
        return None;
    }
    let theme = skin.to_str().and_then(find)?;
    THEMES.iter().position(|t| t.id == theme.id)
}

/// The theme Alt+S switches to from `current`: the next one, or the first from a skin file.
pub fn next_theme(current: Option<&Path>) -> usize {
    current
        .and_then(theme_index)
        .map_or(0, |i| (i + 1) % THEMES.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_theme_from_the_default_is_the_second() {
        // Regression: with no skin chosen the first theme showed, but the first Alt+S
        // "switched" to it again because the current skin was unknown.
        let start = default_skin(false);
        assert_eq!(theme_index(&start), Some(0));
        assert_eq!(next_theme(Some(&start)), 1);
    }

    #[test]
    fn cycling_wraps_and_skin_files_go_to_the_first_theme() {
        let last = PathBuf::from(THEMES[THEMES.len() - 1].id);
        assert_eq!(next_theme(Some(&last)), 0);
        assert_eq!(next_theme(Some(Path::new("/skins/classic.wsz"))), 0);
        assert_eq!(next_theme(None), 0);
        assert_eq!(theme_index(Path::new(LOCAL_BASE_SKIN)), None);
    }
}
