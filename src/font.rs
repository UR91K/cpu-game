const FONT_BMP: &[u8] = include_bytes!("../textures/FONT.bmp");

pub const GLYPH_W: usize = 8;
pub const GLYPH_H: usize = 8;
pub const FONT_COLS: usize = 16;
pub const FONT_ROWS: usize = 6;
pub const FIRST_ASCII: u8 = 32;

/// An 8×8 glyph stored as 8 bytes, one per row.
/// In each byte, bit 7 (MSB) = leftmost pixel, bit 0 (LSB) = rightmost pixel.
pub struct Glyph {
    pub rows: [u8; GLYPH_H],
}

impl Glyph {
    #[inline]
    pub fn pixel(&self, x: usize, y: usize) -> bool {
        self.rows[y] & (0x80 >> x) != 0
    }
}

/// Bitmap font parsed from FONT.bmp.
/// Characters start at ASCII 32 (space) and increase left-to-right, top-to-bottom.
pub struct Font {
    glyphs: Box<[Glyph]>,
}

impl Font {
    pub fn load() -> Self {
        let img = image::load_from_memory(FONT_BMP)
            .expect("FONT.bmp failed to decode")
            .to_luma8();

        let mut glyphs = Vec::with_capacity(FONT_COLS * FONT_ROWS);
        for row in 0..FONT_ROWS {
            for col in 0..FONT_COLS {
                let mut rows = [0u8; GLYPH_H];
                for gy in 0..GLYPH_H {
                    let mut bits = 0u8;
                    for gx in 0..GLYPH_W {
                        let px = (col * GLYPH_W + gx) as u32;
                        let py = (row * GLYPH_H + gy) as u32;
                        if img.get_pixel(px, py).0[0] > 128 {
                            bits |= 0x80 >> gx;
                        }
                    }
                    rows[gy] = bits;
                }
                glyphs.push(Glyph { rows });
            }
        }

        Self { glyphs: glyphs.into_boxed_slice() }
    }

    /// Returns the glyph for `ch`, or `None` if outside the font's character range.
    pub fn glyph(&self, ch: char) -> Option<&Glyph> {
        let code = ch as u32;
        if code < FIRST_ASCII as u32 {
            return None;
        }
        self.glyphs.get((code - FIRST_ASCII as u32) as usize)
    }

    /// Draw `text` into an RGBA byte buffer (row-major, 4 bytes/pixel) at pixel
    /// position (`x`, `y`). Lit pixels are written in `color`; dark pixels are skipped.
    pub fn draw_text(
        &self,
        buf: &mut [u8],
        buf_width: usize,
        buf_height: usize,
        text: &str,
        mut x: usize,
        y: usize,
        color: [u8; 3],
    ) {
        for ch in text.chars() {
            if x + GLYPH_W > buf_width {
                break;
            }
            if let Some(glyph) = self.glyph(ch) {
                for gy in 0..GLYPH_H {
                    let py = y + gy;
                    if py >= buf_height {
                        break;
                    }
                    for gx in 0..GLYPH_W {
                        if glyph.pixel(gx, gy) {
                            let idx = (py * buf_width + x + gx) * 4;
                            buf[idx] = color[0];
                            buf[idx + 1] = color[1];
                            buf[idx + 2] = color[2];
                            buf[idx + 3] = 255;
                        }
                    }
                }
            }
            x += GLYPH_W;
        }
    }
}
