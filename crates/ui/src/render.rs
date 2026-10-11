//! Software rendering helpers.

use amp_skin::Bitmap;

/// Integer nearest-neighbor upscale, for double and triple size.
pub fn scale_nearest(src: &Bitmap, factor: u32) -> Bitmap {
    let factor = factor.max(1);
    if factor == 1 {
        return src.clone();
    }
    let (w, h) = (src.width(), src.height());
    let mut out = Bitmap::new(w * factor, h * factor, 0);
    let (sw, dw) = (w as usize, (w * factor) as usize);
    let f = factor as usize;
    let dst = out.pixels_mut();
    for y in 0..h as usize {
        let row = &src.pixels()[y * sw..(y + 1) * sw];
        let first = y * f * dw;
        for (x, p) in row.iter().enumerate() {
            dst[first + x * f..first + (x + 1) * f].fill(*p);
        }
        for k in 1..f {
            dst.copy_within(first..first + dw, first + k * dw);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_each_pixel() {
        let mut src = Bitmap::new(2, 1, 0);
        src.set(0, 0, 1);
        src.set(1, 0, 2);
        let out = scale_nearest(&src, 2);
        assert_eq!(out.pixels(), &[1, 1, 2, 2, 1, 1, 2, 2]);
        assert_eq!(scale_nearest(&src, 1), src);
        assert_eq!(scale_nearest(&src, 3).width(), 6);
    }
}
