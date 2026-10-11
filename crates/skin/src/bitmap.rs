//! 32-bit pixel buffers and sprite blitting.

use crate::SkinError;

/// A rectangle in pixels. Used for sprite coordinates inside a sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
}

/// An image as `0xAARRGGBB` pixels, row by row.
#[derive(Clone, PartialEq, Eq)]
pub struct Bitmap {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
}

impl std::fmt::Debug for Bitmap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bitmap({}x{})", self.width, self.height)
    }
}

/// Opaque color from red, green and blue.
pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    0xFF00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

impl Bitmap {
    /// A bitmap filled with one color.
    pub fn new(width: u32, height: u32, fill: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![fill; width as usize * height as usize],
        }
    }

    /// Decodes BMP or PNG data. The format is detected from the content, since some skins
    /// ship PNGs named `.bmp`.
    pub fn decode(bytes: &[u8]) -> Result<Self, SkinError> {
        let img = image::load_from_memory(bytes)?.to_rgba8();
        let (width, height) = img.dimensions();
        let pixels = img
            .pixels()
            .map(|p| {
                let [r, g, b, a] = p.0;
                u32::from(a) << 24 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
            })
            .collect();
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    pub fn pixels_mut(&mut self) -> &mut [u32] {
        &mut self.pixels
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
        (x < self.width && y < self.height).then(|| y as usize * self.width as usize + x as usize)
    }

    pub fn get(&self, x: i32, y: i32) -> Option<u32> {
        self.index(x, y).map(|i| self.pixels[i])
    }

    pub fn set(&mut self, x: i32, y: i32, color: u32) {
        if let Some(i) = self.index(x, y) {
            self.pixels[i] = color;
        }
    }

    /// Whether `r` lies fully inside this bitmap.
    pub fn contains_rect(&self, r: Rect) -> bool {
        r.x >= 0
            && r.y >= 0
            && r.w >= 0
            && r.h >= 0
            && (r.x + r.w) as u32 <= self.width
            && (r.y + r.h) as u32 <= self.height
    }

    pub fn fill_rect(&mut self, r: Rect, color: u32) {
        for y in r.y.max(0)..(r.y + r.h).min(self.height as i32) {
            for x in r.x.max(0)..(r.x + r.w).min(self.width as i32) {
                self.set(x, y, color);
            }
        }
    }

    /// Copies `src_rect` of `src` to (`dx`, `dy`) in this bitmap, clipping on both sides.
    /// Skins are opaque, so pixels are copied, not blended.
    pub fn blit(&mut self, src: &Bitmap, src_rect: Rect, dx: i32, dy: i32) {
        // Clip the source rect to the source bitmap, shifting the destination to match.
        let mut sx = src_rect.x;
        let mut sy = src_rect.y;
        let mut w = src_rect.w;
        let mut h = src_rect.h;
        let (mut dx, mut dy) = (dx, dy);
        if sx < 0 {
            w += sx;
            dx -= sx;
            sx = 0;
        }
        if sy < 0 {
            h += sy;
            dy -= sy;
            sy = 0;
        }
        w = w.min(src.width as i32 - sx);
        h = h.min(src.height as i32 - sy);
        // Then clip against the destination.
        if dx < 0 {
            w += dx;
            sx -= dx;
            dx = 0;
        }
        if dy < 0 {
            h += dy;
            sy -= dy;
            dy = 0;
        }
        w = w.min(self.width as i32 - dx);
        h = h.min(self.height as i32 - dy);
        if w <= 0 || h <= 0 {
            return;
        }
        let (w, h) = (w as usize, h as usize);
        for row in 0..h {
            let s = (sy as usize + row) * src.width as usize + sx as usize;
            let d = (dy as usize + row) * self.width as usize + dx as usize;
            self.pixels[d..d + w].copy_from_slice(&src.pixels[s..s + w]);
        }
    }

    /// Copies a region out into a new bitmap. Parts outside the source are transparent.
    pub fn crop(&self, r: Rect) -> Bitmap {
        let mut out = Bitmap::new(r.w.max(0) as u32, r.h.max(0) as u32, 0);
        out.blit(self, r, 0, 0);
        out
    }

    /// Pixels as RGBA bytes, for encoding or uploading.
    pub fn to_rgba_bytes(&self) -> Vec<u8> {
        self.pixels
            .iter()
            .flat_map(|p| {
                let [a, r, g, b] = p.to_be_bytes();
                [r, g, b, a]
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: u32 = rgb(255, 0, 0);
    const BLUE: u32 = rgb(0, 0, 255);

    fn numbered(w: u32, h: u32) -> Bitmap {
        let mut b = Bitmap::new(w, h, 0);
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                b.set(x, y, (y * 100 + x) as u32);
            }
        }
        b
    }

    #[test]
    fn get_set_and_bounds() {
        let mut b = Bitmap::new(4, 3, BLUE);
        b.set(3, 2, RED);
        assert_eq!(b.get(3, 2), Some(RED));
        assert_eq!(b.get(4, 2), None);
        assert_eq!(b.get(-1, 0), None);
        b.set(9, 9, RED);
        assert_eq!(b.pixels().iter().filter(|p| **p == RED).count(), 1);
    }

    #[test]
    fn blit_copies_region() {
        let src = numbered(10, 10);
        let mut dst = Bitmap::new(5, 5, 0);
        dst.blit(&src, Rect::new(2, 3, 2, 2), 1, 1);
        assert_eq!(dst.get(1, 1), Some(302));
        assert_eq!(dst.get(2, 2), Some(403));
        assert_eq!(dst.get(0, 0), Some(0));
        assert_eq!(dst.get(3, 3), Some(0));
    }

    #[test]
    fn blit_clips_destination_edges() {
        let src = numbered(10, 10);
        let mut dst = Bitmap::new(4, 4, 0);
        dst.blit(&src, Rect::new(0, 0, 3, 3), -1, -1);
        assert_eq!(dst.get(0, 0), Some(101));
        dst.blit(&src, Rect::new(0, 0, 3, 3), 3, 3);
        assert_eq!(dst.get(3, 3), Some(0));
    }

    #[test]
    fn blit_clips_source_edges() {
        let src = numbered(4, 4);
        let mut dst = Bitmap::new(6, 6, 7);
        // Source rect hangs off the bottom-right and the top-left of the sheet.
        dst.blit(&src, Rect::new(2, 2, 5, 5), 0, 0);
        assert_eq!(dst.get(1, 1), Some(303));
        assert_eq!(
            dst.get(2, 2),
            Some(7),
            "outside the sheet is left untouched"
        );
        dst.blit(&src, Rect::new(-2, -2, 3, 3), 0, 0);
        assert_eq!(dst.get(2, 2), Some(0));
        assert_eq!(
            dst.get(0, 0),
            Some(202),
            "nothing drawn for the off-sheet part"
        );
    }

    #[test]
    fn crop_and_contains() {
        let src = numbered(10, 10);
        assert!(src.contains_rect(Rect::new(0, 0, 10, 10)));
        assert!(!src.contains_rect(Rect::new(1, 0, 10, 10)));
        let c = src.crop(Rect::new(8, 8, 4, 4));
        assert_eq!(c.get(1, 1), Some(909));
        assert_eq!(c.get(3, 3), Some(0));
    }

    #[test]
    fn rgba_bytes_order() {
        let b = Bitmap::new(1, 1, 0x80_11_22_33);
        assert_eq!(b.to_rgba_bytes(), vec![0x11, 0x22, 0x33, 0x80]);
    }

    #[test]
    fn decode_bmp_and_png_by_content() {
        let mut img = image::RgbImage::new(3, 2);
        img.put_pixel(2, 1, image::Rgb([10, 20, 30]));
        for format in [image::ImageFormat::Bmp, image::ImageFormat::Png] {
            let mut bytes = std::io::Cursor::new(Vec::new());
            img.write_to(&mut bytes, format).unwrap();
            let b = Bitmap::decode(bytes.get_ref()).unwrap();
            assert_eq!((b.width(), b.height()), (3, 2));
            assert_eq!(b.get(2, 1), Some(rgb(10, 20, 30)));
            assert_eq!(b.get(0, 0), Some(rgb(0, 0, 0)));
        }
    }

    #[test]
    fn decode_garbage_is_an_error() {
        assert!(Bitmap::decode(b"nope").is_err());
    }
}
