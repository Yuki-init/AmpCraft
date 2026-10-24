//! Which skin the player starts with, and cycling through the built-in themes.

use std::path::{Path, PathBuf};

use ampcraft_skin::theme::{SYSTEM_ID, THEMES, find};

/// The local copy of the stock skin that development checkouts may have (never shipped).
pub const LOCAL_BASE_SKIN: &str = "skins/base-2.91.wsz";

/// The skin to use when none was chosen: the local base skin when there is one, else the
/// System theme (the desktop's colors). Always a value, so the app knows what is showing.
pub fn default_skin(local_base_exists: bool) -> PathBuf {
    if local_base_exists {
        PathBuf::from(LOCAL_BASE_SKIN)
    } else {
        PathBuf::from(SYSTEM_ID)
    }
}

/// Whether `skin` is the System theme, which follows the desktop's colors.
pub fn is_system(skin: &Path) -> bool {
    !skin.exists()
        && skin
            .to_str()
            .is_some_and(|s| s.trim().eq_ignore_ascii_case(SYSTEM_ID))
}

/// The built-in theme `skin` names, if it is one (a path that exists is a file, not a theme).
pub fn theme_index(skin: &Path) -> Option<usize> {
    if skin.exists() {
        return None;
    }
    let theme = skin.to_str().and_then(find)?;
    THEMES.iter().position(|t| t.id == theme.id)
}

/// The theme Alt+S switches to from `current`: System, then each built-in theme in turn,
/// then System again. `None` is System. From a skin file it goes to System.
pub fn next_theme(current: Option<&Path>) -> Option<usize> {
    match current {
        Some(p) if is_system(p) => Some(0),
        Some(p) => theme_index(p).map(|i| i + 1).filter(|&i| i < THEMES.len()),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_system_and_alt_s_moves_off_it() {
        // Regression: with no skin chosen the first Alt+S must change what is showing.
        let start = default_skin(false);
        assert!(is_system(&start));
        assert_eq!(theme_index(&start), None);
        assert_eq!(next_theme(Some(&start)), Some(0));
    }

    #[test]
    fn cycling_wraps_through_system_and_skin_files_go_to_system() {
        let first = PathBuf::from(THEMES[0].id);
        assert_eq!(next_theme(Some(&first)), Some(1));
        let last = PathBuf::from(THEMES[THEMES.len() - 1].id);
        assert_eq!(next_theme(Some(&last)), None);
        assert_eq!(next_theme(Some(Path::new("/skins/classic.wsz"))), None);
        assert_eq!(next_theme(None), None);
        assert_eq!(theme_index(Path::new(LOCAL_BASE_SKIN)), None);
        assert!(is_system(Path::new("System")));
    }
}
