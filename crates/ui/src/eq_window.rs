//! The equalizer window: ON/AUTO toggles, preamp and ten band sliders, and the response
//! graph. Values are in dB, ±12.

use ampcraft_dock::Point;
use ampcraft_skin::sprites::eq as sp;
use ampcraft_skin::{Bitmap, Rect, Sheet, Skin, Sprite};

pub const WIDTH: i32 = 275;
pub const HEIGHT: i32 = 116;
const MAX_DB: f32 = 12.0;
const SLIDER_Y: i32 = 38;
const THUMB_TRAVEL: i32 = 51;
const THUMB_HALF: i32 = 5;
const GRAPH: Rect = Rect::new(86, 17, 113, 19);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqControl {
    Close,
    Shade,
    On,
    Auto,
    Presets,
    Preamp,
    Band(usize),
    /// The shade bar's volume and balance sliders.
    ShadeVolume,
    ShadeBalance,
}

impl EqControl {
    pub fn rect(self) -> Rect {
        match self {
            EqControl::Close => Rect::new(264, 3, 9, 9),
            EqControl::Shade => Rect::new(254, 3, 9, 9),
            EqControl::On => Rect::new(14, 18, 26, 12),
            EqControl::Auto => Rect::new(40, 18, 32, 12),
            EqControl::Presets => Rect::new(217, 18, 44, 12),
            EqControl::Preamp => Rect::new(21, SLIDER_Y, sp::SLIDER_W, sp::SLIDER_H),
            EqControl::Band(i) => {
                Rect::new(78 + 18 * i as i32, SLIDER_Y, sp::SLIDER_W, sp::SLIDER_H)
            }
            EqControl::ShadeVolume => Rect::new(61, 4, 97, 7),
            EqControl::ShadeBalance => Rect::new(164, 4, 43, 7),
        }
    }

    fn contains(self, p: Point) -> bool {
        let r = self.rect();
        p.x >= r.x && p.x < r.x + r.w && p.y >= r.y && p.y < r.y + r.h
    }

    fn all() -> impl Iterator<Item = EqControl> {
        [
            EqControl::Close,
            EqControl::Shade,
            EqControl::On,
            EqControl::Auto,
            EqControl::Presets,
            EqControl::Preamp,
        ]
        .into_iter()
        .chain((0..10).map(EqControl::Band))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EqAction {
    SetEnabled(bool),
    SetAuto(bool),
    SetPreamp(f32),
    SetBand(usize, f32),
    /// The shade bar's sliders: volume 0.0 to 1.0, balance -1.0 (left) to 1.0.
    SetVolume(f32),
    SetBalance(f32),
    Presets,
    Close,
    ToggleShade,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqDown {
    Control(EqControl),
    DragWindow,
}

#[derive(Debug, Default)]
pub struct EqWindow {
    pub enabled: bool,
    pub auto: bool,
    pub preamp_db: f32,
    pub bands_db: [f32; 10],
    pub focused: bool,
    pub shaded: bool,
    /// Volume and balance, shown on the shade bar; the app keeps them current.
    pub volume: f32,
    pub balance: f32,
    pressed: Option<(EqControl, bool)>,
    /// Slider being dragged.
    slider: Option<EqControl>,
}

/// 0.0 at -12 dB, 1.0 at +12 dB.
fn level(db: f32) -> f32 {
    ((db + MAX_DB) / (2.0 * MAX_DB)).clamp(0.0, 1.0)
}

impl EqWindow {
    pub fn new() -> Self {
        Self::default()
    }

    /// Unscaled height for the current mode.
    pub fn height(&self) -> i32 {
        if self.shaded { 14 } else { HEIGHT }
    }

    fn hit(&self, p: Point) -> Option<EqControl> {
        if self.shaded {
            return [
                EqControl::Close,
                EqControl::Shade,
                EqControl::ShadeVolume,
                EqControl::ShadeBalance,
            ]
            .into_iter()
            .find(|c| c.contains(p));
        }
        EqControl::all().find(|c| c.contains(p))
    }

    /// Changes whenever the rendered image would change.
    pub fn render_hash(&self, scale: u32) -> u64 {
        crate::render::hash_of(&(
            scale,
            self.enabled,
            self.auto,
            self.preamp_db.to_bits(),
            self.bands_db.map(f32::to_bits),
            self.focused,
            self.shaded,
            self.pressed.map(|(c, inside)| (format!("{c:?}"), inside)),
            self.slider.map(|c| format!("{c:?}")),
            (self.volume.to_bits(), self.balance.to_bits()),
        ))
    }

    pub fn is_interacting(&self) -> bool {
        self.pressed.is_some() || self.slider.is_some()
    }

    fn db_at(y: i32) -> f32 {
        let t = (y - SLIDER_Y - THUMB_HALF) as f32 / THUMB_TRAVEL as f32;
        let db = (1.0 - t.clamp(0.0, 1.0)) * 2.0 * MAX_DB - MAX_DB;
        // A little stickiness at 0 dB makes it easy to flatten a band.
        if db.abs() < 1.0 { 0.0 } else { db }
    }

    fn slide(&mut self, control: EqControl, p: Point) -> Vec<EqAction> {
        // Shade sliders: the thumb (3 px) centered on the pointer.
        let along = |c: EqControl| {
            let r = c.rect();
            ((p.x - r.x - 1) as f32 / (r.w - 3) as f32).clamp(0.0, 1.0)
        };
        match control {
            EqControl::ShadeVolume => {
                self.volume = along(control);
                return vec![EqAction::SetVolume(self.volume)];
            }
            EqControl::ShadeBalance => {
                let b = along(control) * 2.0 - 1.0;
                // Sticky center, as on the main window.
                self.balance = if b.abs() < 0.08 { 0.0 } else { b };
                return vec![EqAction::SetBalance(self.balance)];
            }
            _ => {}
        }
        let db = Self::db_at(p.y);
        match control {
            EqControl::Preamp => {
                self.preamp_db = db;
                vec![EqAction::SetPreamp(db)]
            }
            EqControl::Band(i) if i < 10 => {
                self.bands_db[i] = db;
                vec![EqAction::SetBand(i, db)]
            }
            _ => Vec::new(),
        }
    }

    pub fn pointer_down(&mut self, p: Point) -> (EqDown, Vec<EqAction>) {
        let Some(control) = self.hit(p) else {
            return (EqDown::DragWindow, Vec::new());
        };
        match control {
            EqControl::Preamp
            | EqControl::Band(_)
            | EqControl::ShadeVolume
            | EqControl::ShadeBalance => {
                self.slider = Some(control);
                (EqDown::Control(control), self.slide(control, p))
            }
            _ => {
                self.pressed = Some((control, true));
                (EqDown::Control(control), Vec::new())
            }
        }
    }

    pub fn pointer_move(&mut self, p: Point) -> Vec<EqAction> {
        if let Some(c) = self.slider {
            return self.slide(c, p);
        }
        if let Some((c, _)) = self.pressed {
            self.pressed = Some((c, c.contains(p)));
        }
        Vec::new()
    }

    pub fn pointer_up(&mut self, p: Point) -> Vec<EqAction> {
        if let Some(c) = self.slider.take() {
            return self.slide(c, p);
        }
        let Some((c, _)) = self.pressed.take() else {
            return Vec::new();
        };
        if !c.contains(p) {
            return Vec::new();
        }
        match c {
            EqControl::On => {
                self.enabled = !self.enabled;
                vec![EqAction::SetEnabled(self.enabled)]
            }
            EqControl::Auto => {
                self.auto = !self.auto;
                vec![EqAction::SetAuto(self.auto)]
            }
            EqControl::Presets => vec![EqAction::Presets],
            EqControl::Shade => vec![EqAction::ToggleShade],
            EqControl::Close => vec![EqAction::Close],
            _ => Vec::new(),
        }
    }

    fn pressed_on(&self, c: EqControl) -> bool {
        self.pressed == Some((c, true))
    }

    /// Draws the window at `scale` (pixel doubling; the equalizer has no text).
    pub fn render(&self, skin: &Skin, scale: u32) -> Bitmap {
        if self.shaded {
            let mut b = Bitmap::new(WIDTH as u32, 14, 0xFF00_0000);
            let bg = if self.focused {
                sp::SHADE_BACKGROUND_ACTIVE
            } else {
                sp::SHADE_BACKGROUND
            };
            skin.draw(&mut b, bg, 0, 0);
            if self.pressed_on(EqControl::Shade) {
                skin.draw(&mut b, sp::SHADE_UNSHADE_PRESSED, 254, 3);
            }
            let close = if self.pressed_on(EqControl::Close) {
                sp::SHADE_CLOSE_PRESSED
            } else {
                sp::SHADE_CLOSE
            };
            skin.draw(&mut b, close, 264, 3);
            // Volume and balance thumbs: the sprite follows the value (low, middle, high).
            let v = self.volume.clamp(0.0, 1.0);
            let vol = EqControl::ShadeVolume.rect();
            let vi = if v < 1.0 / 3.0 {
                0
            } else if v < 2.0 / 3.0 {
                1
            } else {
                2
            };
            let vx = vol.x + (v * (vol.w - 3) as f32).round() as i32;
            skin.draw(&mut b, sp::SHADE_VOLUME_THUMBS[vi], vx, vol.y);
            let bal = self.balance.clamp(-1.0, 1.0);
            let br = EqControl::ShadeBalance.rect();
            let bi = if bal < -0.25 {
                0
            } else if bal > 0.25 {
                2
            } else {
                1
            };
            let bx = br.x + ((bal + 1.0) / 2.0 * (br.w - 3) as f32).round() as i32;
            skin.draw(&mut b, sp::SHADE_BALANCE_THUMBS[bi], bx, br.y);
            return crate::render::scale_nearest(&b, scale);
        }
        let mut b = Bitmap::new(WIDTH as u32, HEIGHT as u32, 0xFF00_0000);
        let draw = |b: &mut Bitmap, s: Sprite, x: i32, y: i32| {
            skin.draw(b, s, x, y);
        };
        draw(&mut b, sp::BACKGROUND, 0, 0);
        draw(
            &mut b,
            if self.focused {
                sp::TITLE_BAR_ACTIVE
            } else {
                sp::TITLE_BAR
            },
            0,
            0,
        );
        if self.pressed_on(EqControl::Close) {
            draw(&mut b, sp::CLOSE_PRESSED, 264, 3);
        }
        let on = match (self.enabled, self.pressed_on(EqControl::On)) {
            (false, false) => sp::ON,
            (false, true) => sp::ON_PRESSED,
            (true, false) => sp::ON_ACTIVE,
            (true, true) => sp::ON_ACTIVE_PRESSED,
        };
        draw(&mut b, on, 14, 18);
        let auto = match (self.auto, self.pressed_on(EqControl::Auto)) {
            (false, false) => sp::AUTO,
            (false, true) => sp::AUTO_PRESSED,
            (true, false) => sp::AUTO_ACTIVE,
            (true, true) => sp::AUTO_ACTIVE_PRESSED,
        };
        draw(&mut b, auto, 40, 18);
        let presets = if self.pressed_on(EqControl::Presets) {
            sp::PRESETS_PRESSED
        } else {
            sp::PRESETS
        };
        draw(&mut b, presets, 217, 18);

        // Response graph: preamp line, then the band curve colored by height.
        draw(&mut b, sp::GRAPH_BACKGROUND, GRAPH.x, GRAPH.y);
        let max_y = GRAPH.h - 1;
        let to_y = |db: f32| ((1.0 - level(db)) * max_y as f32).round() as i32;
        draw(
            &mut b,
            sp::PREAMP_LINE,
            GRAPH.x,
            GRAPH.y + to_y(self.preamp_db),
        );
        let line_colors = skin.sheet(Sheet::EqMain);
        let color_at = |y: i32| {
            line_colors
                .and_then(|s| {
                    s.get(
                        sp::GRAPH_LINE_COLORS.rect.x,
                        sp::GRAPH_LINE_COLORS.rect.y + y,
                    )
                })
                .unwrap_or(0xFF00_FF00)
        };
        let mut prev: Option<i32> = None;
        for x in 0..GRAPH.w {
            // Linear interpolation between band points spread across the graph.
            let pos = x as f32 * 9.0 / (GRAPH.w - 1) as f32;
            let i = (pos.floor() as usize).min(8);
            let t = pos - i as f32;
            let db = self.bands_db[i] * (1.0 - t) + self.bands_db[i + 1] * t;
            let y = to_y(db);
            let (lo, hi) = match prev {
                Some(p) if (p - y).abs() > 1 => (p.min(y), p.max(y)),
                _ => (y, y),
            };
            for yy in lo..=hi {
                b.set(GRAPH.x + x, GRAPH.y + yy, color_at(yy));
            }
            prev = Some(y);
        }

        // Sliders.
        for c in std::iter::once(EqControl::Preamp).chain((0..10).map(EqControl::Band)) {
            let db = match c {
                EqControl::Band(i) => self.bands_db[i],
                _ => self.preamp_db,
            };
            let r = c.rect();
            let l = level(db);
            draw(&mut b, sp::slider_frame(l), r.x, r.y);
            let thumb = if self.slider == Some(c) {
                sp::SLIDER_THUMB_PRESSED
            } else {
                sp::SLIDER_THUMB
            };
            draw(
                &mut b,
                thumb,
                r.x + 1,
                r.y + ((1.0 - l) * THUMB_TRAVEL as f32).round() as i32,
            );
        }
        crate::render::scale_nearest(&b, scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_skin::{code, coded_skin};

    fn center(c: EqControl) -> Point {
        let r = c.rect();
        Point::new(r.x + r.w / 2, r.y + r.h / 2)
    }

    #[test]
    fn shade_bar_volume_and_balance() {
        let skin = coded_skin();
        let mut w = EqWindow::new();
        w.shaded = true;
        w.volume = 1.0;
        let b = w.render(&skin, 1);
        // Full volume: the "high" thumb at the right end of the groove.
        assert_eq!(b.get(61 + 94, 4), Some(code(Sheet::EqEx, 7, 30)));
        // Centered balance: the middle thumb in the middle.
        assert_eq!(b.get(164 + 20, 4), Some(code(Sheet::EqEx, 14, 30)));
        // Dragging sets them.
        let (_, a) = w.pointer_down(Point::new(61 + 1, 7));
        assert_eq!(a, vec![EqAction::SetVolume(0.0)]);
        assert_eq!(
            w.pointer_move(Point::new(300, 7)),
            vec![EqAction::SetVolume(1.0)]
        );
        w.pointer_up(Point::new(300, 7));
        let (_, a) = w.pointer_down(Point::new(164 + 1, 6));
        assert_eq!(a, vec![EqAction::SetBalance(-1.0)]);
        assert_eq!(
            w.pointer_move(Point::new(164 + 22, 6)),
            vec![EqAction::SetBalance(0.0)],
            "sticky center"
        );
    }

    #[test]
    fn presets_button_is_always_drawn() {
        let skin = coded_skin();
        let mut w = EqWindow::new();
        let b = w.render(&skin, 1);
        assert_eq!(b.get(217, 18), Some(code(Sheet::EqMain, 224, 164)));
        w.pointer_down(center(EqControl::Presets));
        let b = w.render(&skin, 1);
        assert_eq!(b.get(217, 18), Some(code(Sheet::EqMain, 224, 176)));
    }

    #[test]
    fn controls_hit_at_center() {
        for c in EqControl::all() {
            assert_eq!(EqControl::all().find(|x| x.contains(center(c))), Some(c));
        }
        let mut w = EqWindow::new();
        assert_eq!(w.pointer_down(Point::new(120, 6)).0, EqDown::DragWindow);
    }

    #[test]
    fn on_toggles_and_close_reports() {
        let mut w = EqWindow::new();
        w.pointer_down(center(EqControl::On));
        assert_eq!(
            w.pointer_up(center(EqControl::On)),
            vec![EqAction::SetEnabled(true)]
        );
        w.pointer_down(center(EqControl::Close));
        assert_eq!(
            w.pointer_up(center(EqControl::Close)),
            vec![EqAction::Close]
        );
        w.pointer_down(center(EqControl::Auto));
        assert!(
            w.pointer_up(Point::new(0, 0)).is_empty(),
            "released outside"
        );
    }

    #[test]
    fn sliders_map_top_to_plus_twelve() {
        let mut w = EqWindow::new();
        let r = EqControl::Band(3).rect();
        let (_, a) = w.pointer_down(Point::new(r.x + 5, SLIDER_Y));
        assert_eq!(a, vec![EqAction::SetBand(3, 12.0)]);
        assert_eq!(
            w.pointer_move(Point::new(r.x + 5, 500)),
            vec![EqAction::SetBand(3, -12.0)]
        );
        // Middle snaps to flat.
        let mid = SLIDER_Y + THUMB_HALF + THUMB_TRAVEL / 2;
        assert_eq!(
            w.pointer_up(Point::new(r.x + 5, mid + 1)),
            vec![EqAction::SetBand(3, 0.0)]
        );
        let (_, a) = w.pointer_down(center(EqControl::Preamp));
        assert!(matches!(a[..], [EqAction::SetPreamp(_)]));
    }

    #[test]
    fn shade_toggles_and_keeps_only_bar_controls() {
        let skin = coded_skin();
        let mut w = EqWindow::new();
        w.pointer_down(center(EqControl::Shade));
        assert_eq!(
            w.pointer_up(center(EqControl::Shade)),
            vec![EqAction::ToggleShade]
        );
        w.shaded = true;
        assert_eq!(w.height(), 14);
        assert_eq!(
            w.pointer_down(center(EqControl::On)).0,
            EqDown::DragWindow,
            "no sliders in shade"
        );
        w.pointer_down(center(EqControl::Close));
        assert_eq!(
            w.pointer_up(center(EqControl::Close)),
            vec![EqAction::Close]
        );
        let b = w.render(&skin, 2);
        assert_eq!((b.width(), b.height()), (550, 28));
    }

    #[test]
    fn render_states_and_thumbs() {
        let skin = coded_skin();
        let mut w = EqWindow::new();
        w.enabled = true;
        w.bands_db[0] = 12.0;
        let b = w.render(&skin, 1);
        assert_eq!(b.get(14, 18), Some(code(Sheet::EqMain, 69, 119)), "ON lit");
        let r = EqControl::Band(0).rect();
        assert_eq!(
            b.get(r.x + 1, r.y),
            Some(code(Sheet::EqMain, 0, 164)),
            "thumb at top"
        );
        let preamp = EqControl::Preamp.rect();
        assert_eq!(
            b.get(preamp.x + 1, preamp.y + 26),
            Some(code(Sheet::EqMain, 0, 164)),
            "flat preamp thumb in the middle"
        );
        // Top frame for +12 dB: frame 27 is in the second row, column 13.
        assert_eq!(
            b.get(r.x + 13, r.y + 20),
            Some(code(Sheet::EqMain, 13 + 13 * 15 + 13, 164 + 65 + 20))
        );
    }
}
