//! Drag, snap and dock demo on the native window backend.
//!
//! `ampcraft-dock-demo [--skin SKIN.wsz] [--scale 1|2|3]`
//!
//! Opens a main window, an equalizer and a playlist panel drawn from the skin (or plain
//! colors without one). Left-drag moves a window: it snaps to the others and to screen
//! edges, and dragging the main window carries everything docked to it. Escape cancels a
//! drag; right-click or closing a window quits.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ampcraft_dock::{Layout, PanelId, Rect};
use ampcraft_platform::NativePlatform;
use ampcraft_platform::{
    Event, MouseButton, Platform, Point, Size, WindowId, WindowKind, WindowSpec, keys,
};
use ampcraft_skin::sprites::main;
use ampcraft_skin::{Bitmap, Sheet, Skin, rgb};
use ampcraft_ui::{DockDrag, MoveTracker};
use anyhow::{Context, Result};
use clap::Parser;

#[derive(Parser)]
#[command(about = "Drag, snap and dock demo")]
struct Args {
    /// Skin file or directory. Defaults to skins/base-2.91.wsz when present.
    #[arg(long)]
    skin: Option<PathBuf>,
    /// Integer scale factor.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=3))]
    scale: u8,
}

struct Panel {
    id: PanelId,
    window: WindowId,
    frame: Bitmap,
}

/// Draws a panel at 1x: from the skin when it has the sheet, else a plain placeholder.
fn render(skin: Option<&Skin>, which: usize, w: i32, h: i32) -> Bitmap {
    let mut b = Bitmap::new(w as u32, h as u32, rgb(40, 40, 60));
    let placeholder_bar = [rgb(70, 90, 160), rgb(70, 140, 90), rgb(150, 100, 60)][which];
    let drawn = skin.is_some_and(|s| match which {
        0 => s.draw(&mut b, main::BACKGROUND, 0, 0) && s.draw(&mut b, main::TITLE_BAR_ACTIVE, 0, 0),
        1 => s.sheet(Sheet::EqMain).is_some_and(|eq| {
            b.blit(eq, ampcraft_skin::Rect::new(0, 0, 275, 116), 0, 0);
            true
        }),
        _ => s.sheet(Sheet::Pledit).is_some_and(|pl| {
            // Top-left corner, title and top-right corner of the playlist frame.
            b.blit(pl, ampcraft_skin::Rect::new(0, 0, 25, 20), 0, 0);
            for x in (25..w - 25).step_by(25) {
                b.blit(pl, ampcraft_skin::Rect::new(127, 0, 25, 20), x, 0);
            }
            b.blit(
                pl,
                ampcraft_skin::Rect::new(26, 0, 100, 20),
                (w - 100) / 2,
                0,
            );
            b.blit(pl, ampcraft_skin::Rect::new(153, 0, 25, 20), w - 25, 0);
            b.fill_rect(
                ampcraft_skin::Rect::new(12, 20, w - 32, h - 58),
                s.playlist_style().normal_bg,
            );
            true
        }),
    });
    if !drawn {
        b.fill_rect(ampcraft_skin::Rect::new(0, 0, w, 14), placeholder_bar);
    }
    if let Some(s) = skin {
        let label = ["MAIN WINDOW - DRAG ME", "EQUALIZER", "PLAYLIST"][which];
        let y = if which == 2 { 26 } else { 27 };
        let x = if which == 0 { 111 } else { 20 };
        let _ = s.draw_text(&mut b, label, x, y);
    }
    b
}

fn scaled(src: &Bitmap, scale: u32) -> Bitmap {
    if scale == 1 {
        return src.clone();
    }
    let mut out = Bitmap::new(src.width() * scale, src.height() * scale, 0);
    for y in 0..out.height() as i32 {
        for x in 0..out.width() as i32 {
            if let Some(p) = src.get(x / scale as i32, y / scale as i32) {
                out.set(x, y, p);
            }
        }
    }
    out
}

fn main() -> Result<()> {
    let args = Args::parse();
    let scale = i32::from(args.scale);
    let skin_path = args.skin.or_else(|| {
        let p = PathBuf::from("skins/base-2.91.wsz");
        p.is_file().then_some(p)
    });
    let skin = match &skin_path {
        Some(p) => Some(Skin::load(p).with_context(|| format!("loading {}", p.display()))?),
        None => None,
    };

    let mut platform = NativePlatform::connect().context("connecting to the display")?;
    let screens = platform.screens()?;
    let origin = screens
        .first()
        .map_or(Point::new(0, 0), |s| Point::new(s.x + 100, s.y + 100));

    // Main and EQ docked, playlist off to the side to be dragged in.
    let specs = [
        ("AmpCraft", WindowKind::Main, Rect::new(0, 0, 275, 116)),
        ("Equalizer", WindowKind::Panel, Rect::new(0, 116, 275, 116)),
        ("Playlist", WindowKind::Panel, Rect::new(400, 0, 275, 232)),
    ];
    let mut layout = Layout::new();
    layout.set_snap_distance(10 * scale);
    layout.set_min_visible(ampcraft_dock::Size::new(28 * scale, 14 * scale));
    let mut panels = BTreeMap::new();
    for (i, (title, kind, r)) in specs.into_iter().enumerate() {
        let rect = Rect::new(
            origin.x + r.x * scale,
            origin.y + r.y * scale,
            r.w * scale,
            r.h * scale,
        );
        let window = platform.create_window(&WindowSpec {
            title: title.into(),
            rect,
            kind,
            owner: None,
        })?;
        let id = PanelId(i as u32 + 1);
        layout.insert(id, rect);
        let frame = scaled(&render(skin.as_ref(), i, r.w, r.h), scale as u32);
        panels.insert(window, Panel { id, window, frame });
    }
    layout.set_leader(Some(PanelId(1)));
    layout.set_screens(screens);
    let mut tracker = MoveTracker::new();
    for p in panels.values() {
        if let Some(r) = layout.rect(p.id) {
            tracker.requested(p.window, Point::new(r.x, r.y));
            platform.move_window(p.window, Point::new(r.x, r.y))?;
        }
        platform.set_visible(p.window, true)?;
    }
    platform.flush()?;

    let window_of = |id: PanelId, panels: &BTreeMap<WindowId, Panel>| {
        panels.values().find(|p| p.id == id).map(|p| p.window)
    };
    let mut drag = DockDrag::new();
    eprintln!("drag windows with the left button; Esc cancels a drag; right-click quits");
    loop {
        let Some(event) = platform.next_event(None)? else {
            continue;
        };
        let mut moves = match event {
            Event::Expose { window } => {
                if let Some(p) = panels.get(&window) {
                    let size = Size::new(p.frame.width() as i32, p.frame.height() as i32);
                    platform.present(window, size, p.frame.pixels())?;
                }
                Vec::new()
            }
            Event::ButtonDown {
                window,
                button: MouseButton::Left,
                root,
                ..
            } => {
                if let Some(p) = panels.get(&window) {
                    drag.press(&layout, p.id, root);
                }
                Vec::new()
            }
            Event::Motion { .. } if drag.is_dragging() => {
                drag.motion(&mut layout, platform.pointer()?)
            }
            Event::ButtonUp {
                button: MouseButton::Left,
                root,
                ..
            } => drag.release(&mut layout, root),
            Event::Key {
                keysym: keys::ESCAPE,
                pressed: true,
                ..
            } => drag.cancel(&mut layout),
            Event::Configured { window, rect } => {
                tracker.observed(window, Point::new(rect.x, rect.y));
                Vec::new()
            }
            Event::ButtonDown {
                button: MouseButton::Right,
                ..
            }
            | Event::CloseRequested { .. } => {
                break;
            }
            _ => Vec::new(),
        };
        // Moves the window manager made on its own (for example keeping a window on screen):
        // once no drag is in progress, shift each docked group to match so it stays docked.
        if !drag.is_dragging() {
            for (window, origin) in tracker.take_overrides() {
                if let Some(p) = panels.get(&window) {
                    moves.extend(layout.moved_externally(p.id, origin));
                }
            }
        }
        for (id, r) in moves {
            if let Some(w) = window_of(id, &panels) {
                tracker.requested(w, Point::new(r.x, r.y));
                platform.move_window(w, Point::new(r.x, r.y))?;
            }
        }
        platform.flush()?;
    }
    Ok(())
}
