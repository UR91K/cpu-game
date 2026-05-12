use crate::font::{Font, GLYPH_H, GLYPH_W};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub glyph: char,
    pub fg: [u8; 4],
    pub bg: [u8; 4],
}

#[derive(Clone, Debug)]
pub struct TextLayer {
    scene_width: usize,
    scene_height: usize,
    pub cols: usize,
    pub rows: usize,
    offset_x: usize,
    offset_y: usize,
    cells: Vec<Option<Cell>>,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

impl TextLayer {
    pub fn new(scene_width: u32, scene_height: u32) -> Self {
        let scene_width = scene_width as usize;
        let scene_height = scene_height as usize;
        let cols = scene_width / GLYPH_W;
        let rows = scene_height / GLYPH_H;
        let used_width = cols * GLYPH_W;
        let used_height = rows * GLYPH_H;

        Self {
            scene_width,
            scene_height,
            cols,
            rows,
            offset_x: scene_width.saturating_sub(used_width) / 2,
            offset_y: scene_height.saturating_sub(used_height) / 2,
            cells: vec![None; cols * rows],
        }
    }

    pub fn scene_size(&self) -> (usize, usize) {
        (self.scene_width, self.scene_height)
    }

    pub fn set(&mut self, col: usize, row: usize, cell: Cell) {
        if let Some(index) = self.index(col, row) {
            self.cells[index] = Some(cell);
        }
    }

    #[allow(dead_code)]
    pub fn clear(&mut self, col: usize, row: usize) {
        if let Some(index) = self.index(col, row) {
            self.cells[index] = None;
        }
    }

    pub fn clear_all(&mut self) {
        self.cells.fill(None);
    }

    pub fn render_to_buf(&self, buf: &mut [u8], font: &Font) {
        let expected_len = self.scene_width * self.scene_height * 4;
        debug_assert_eq!(buf.len(), expected_len);
        if buf.len() < expected_len {
            return;
        }

        for row in 0..self.rows {
            for col in 0..self.cols {
                let Some(cell) = self.cells[row * self.cols + col] else {
                    continue;
                };
                if cell.fg[3] == 0 && cell.bg[3] == 0 {
                    continue;
                }

                let glyph = font.glyph(cell.glyph);
                let base_x = self.offset_x + col * GLYPH_W;
                let base_y = self.offset_y + row * GLYPH_H;

                for glyph_y in 0..GLYPH_H {
                    let pixel_y = base_y + glyph_y;
                    if pixel_y >= self.scene_height {
                        break;
                    }
                    for glyph_x in 0..GLYPH_W {
                        let pixel_x = base_x + glyph_x;
                        if pixel_x >= self.scene_width {
                            break;
                        }
                        let index = (pixel_y * self.scene_width + pixel_x) * 4;
                        let pixel = &mut buf[index..index + 4];
                        let mut dst = [pixel[0], pixel[1], pixel[2], pixel[3]];
                        if cell.bg[3] > 0 {
                            blend(&mut dst, cell.bg);
                        }
                        if cell.fg[3] > 0 && glyph.pixel(glyph_x, glyph_y) {
                            blend(&mut dst, cell.fg);
                        }
                        pixel.copy_from_slice(&dst);
                    }
                }
            }
        }
    }

    fn index(&self, col: usize, row: usize) -> Option<usize> {
        if col < self.cols && row < self.rows {
            Some(row * self.cols + col)
        } else {
            None
        }
    }
}

pub fn place_text(
    layer: &mut TextLayer,
    text: &str,
    halign: HAlign,
    valign: VAlign,
    fg: [u8; 4],
    bg: [u8; 4],
) {
    let text_len = text.chars().count();
    let col = match halign {
        HAlign::Left => 0,
        HAlign::Center => layer.cols.saturating_sub(text_len) / 2,
        HAlign::Right => layer.cols.saturating_sub(text_len),
    };
    let row = match valign {
        VAlign::Top => 0,
        VAlign::Middle => layer.rows.saturating_sub(1) / 2,
        VAlign::Bottom => layer.rows.saturating_sub(1),
    };

    for (index, glyph) in text.chars().enumerate() {
        layer.set(col + index, row, Cell { glyph, fg, bg });
    }
}

#[allow(dead_code)]
pub fn wrap_text(text: &str, max_cols: usize) -> Vec<String> {
    if max_cols == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for word in text.split_whitespace() {
        let word_width = word.chars().count();
        if word_width > max_cols {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                current_width = 0;
            }

            let chars: Vec<char> = word.chars().collect();
            for chunk in chars.chunks(max_cols) {
                lines.push(chunk.iter().collect());
            }
            continue;
        }

        let needed = if current.is_empty() {
            word_width
        } else {
            current_width + 1 + word_width
        };

        if needed > max_cols {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
            current_width = word_width;
        } else {
            if !current.is_empty() {
                current.push(' ');
                current_width += 1;
            }
            current.push_str(word);
            current_width += word_width;
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    lines
}

fn blend(dst: &mut [u8; 4], src: [u8; 4]) {
    let alpha = src[3] as u32;
    for channel in 0..3 {
        dst[channel] = ((src[channel] as u32 * alpha
            + dst[channel] as u32 * (255 - alpha))
            / 255) as u8;
    }
    dst[3] = dst[3].saturating_add(src[3]);
}

#[cfg(test)]
mod tests {
    use super::{Cell, HAlign, TextLayer, VAlign, place_text, wrap_text};
    use crate::font::{Font, GLYPH_H, GLYPH_W};

    #[test]
    fn computes_centered_grid_size() {
        let layer = TextLayer::new(18, 10);

        assert_eq!(layer.cols, 2);
        assert_eq!(layer.rows, 1);
        assert_eq!(layer.offset_x, 1);
        assert_eq!(layer.offset_y, 1);
        assert_eq!(layer.scene_size(), (18, 10));
    }

    #[test]
    fn ignores_out_of_bounds_writes() {
        let mut layer = TextLayer::new(GLYPH_W as u32, GLYPH_H as u32);

        layer.set(
            4,
            4,
            Cell {
                glyph: 'A',
                fg: [255, 255, 255, 255],
                bg: [0, 0, 0, 0],
            },
        );
        layer.clear(8, 8);

        assert!(layer.cells.iter().all(Option::is_none));
    }

    #[test]
    fn blends_background_before_foreground_pixels() {
        let font = Font::load();
        let mut layer = TextLayer::new(GLYPH_W as u32, GLYPH_H as u32);
        layer.set(
            0,
            0,
            Cell {
                glyph: 'A',
                fg: [240, 10, 20, 255],
                bg: [10, 20, 240, 128],
            },
        );

        let mut buf = vec![0u8; GLYPH_W * GLYPH_H * 4];
        layer.render_to_buf(&mut buf, &font);

        let glyph = font.glyph('A');
        let mut lit = None;
        let mut unlit = None;
        for y in 0..GLYPH_H {
            for x in 0..GLYPH_W {
                if glyph.pixel(x, y) && lit.is_none() {
                    lit = Some((x, y));
                }
                if !glyph.pixel(x, y) && unlit.is_none() {
                    unlit = Some((x, y));
                }
            }
        }

        let (lit_x, lit_y) = lit.expect("expected lit pixel in glyph");
        let (unlit_x, unlit_y) = unlit.expect("expected unlit pixel in glyph");

        let lit_index = (lit_y * GLYPH_W + lit_x) * 4;
        assert_eq!(
            &buf[lit_index..lit_index + 4],
            &[240, 10, 20, 255],
        );

        let unlit_index = (unlit_y * GLYPH_W + unlit_x) * 4;
        assert_eq!(
            &buf[unlit_index..unlit_index + 4],
            &[5, 10, 120, 128],
        );
    }

    #[test]
    fn places_single_line_text_with_alignment() {
        let mut layer = TextLayer::new((GLYPH_W * 4) as u32, (GLYPH_H * 3) as u32);

        place_text(
            &mut layer,
            "HI",
            HAlign::Center,
            VAlign::Bottom,
            [255, 255, 255, 255],
            [0, 0, 0, 0],
        );

        let row = layer.rows - 1;
        let col = (layer.cols - 2) / 2;
        assert_eq!(layer.cells[row * layer.cols + col].unwrap().glyph, 'H');
        assert_eq!(layer.cells[row * layer.cols + col + 1].unwrap().glyph, 'I');
    }

    #[test]
    fn wraps_long_words_and_lines() {
        let lines = wrap_text("alpha beta12345 gamma", 5);

        assert_eq!(lines, vec!["alpha", "beta1", "2345", "gamma"]);
    }
}