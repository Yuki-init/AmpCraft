//! Opt-in checks against the owner's local copy of the stock base skin in `skins/`. That file
//! is not committed (no permissive license), so this test passes without doing anything when
//! it is absent.

// Integration tests are test code; panicking on setup failure is the point.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use amp_skin::sprites::{self, main};
use amp_skin::{Sheet, Skin};

fn local_skin() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skins/base-2.91.wsz");
    p.is_file().then_some(p)
}

#[test]
fn base_skin_matches_sprite_maps() {
    let Some(path) = local_skin() else {
        eprintln!("no local base skin, skipping");
        return;
    };
    let skin = Skin::load(&path).unwrap();
    eprintln!("warnings: {:?}", skin.warnings());
    for sheet in Sheet::ALL {
        if let Some(b) = skin.sheet(sheet) {
            eprintln!("{:<14} {}x{}", sheet.file_name(), b.width(), b.height());
        }
    }
    for s in main::ALL
        .iter()
        .chain(sprites::playlist::ALL)
        .chain(sprites::eq::ALL)
    {
        let sheet = skin.sheet(s.sheet).unwrap();
        assert!(sheet.contains_rect(s.rect), "{s:?} outside {sheet:?}");
    }
    assert_eq!(
        skin.sheet(Sheet::Main).map(|b| (b.width(), b.height())),
        Some((275, 116))
    );
    // TEXT.BMP is 155x18 in most skins; the stock base skin's is taller (155x74) with the
    // glyph grid in the top 18 rows.
    let text = skin.sheet(Sheet::Text).unwrap();
    assert!(text.width() >= 155 && text.height() >= 18);
    // Its region.txt is documentation only: every block is commented out.
    assert!(skin.regions().normal.is_empty());
}
