# Text Layer Design Document

## Overview

`text_layer.rs` introduces a structured, grid-based rendering layer for all HUD and overlay UI. Rather than writing pixels directly, subsystems place characters into a logical glyph grid; the layer handles pixel math, centering, and alpha compositing internally.

---

## Core Data Types

### `Cell`

Represents a single character slot in the grid.

```rust
pub struct Cell {
    pub glyph: char,
    pub fg: [u8; 4],  // RGBA foreground color
    pub bg: [u8; 4],  // RGBA background; alpha=0 means skip background
}
```

### `TextLayer`

Owns the glyph grid and renders it to a pixel buffer.

```rust
pub struct TextLayer {
    pub cols: usize,
    pub rows: usize,
    offset_x: usize,  // centering offset in pixels
    offset_y: usize,
    cells: Vec<Option<Cell>>,  // row-major flat array, len = cols * rows
}
```

`TextLayer::new(scene_width, scene_height)` computes:
- `cols = scene_width / GLYPH_W`
- `rows = scene_height / GLYPH_H`
- The pixel offsets that center the truncated grid within the scene

---

## Public API

| Method | Description |
|---|---|
| `set(&mut self, col, row, cell: Cell)` | Place a cell at a grid position |
| `clear(&mut self, col, row)` | Remove the cell at a grid position |
| `clear_all(&mut self)` | Reset every cell to `None` |
| `render_to_buf(&self, buf: &mut [u8], font: &Font)` | Rasterize the grid into a pixel buffer |

`render_to_buf` iterates every cell and alpha-blends first the background color, then foreground pixels (only on lit glyph pixels), into the buffer. `None` cells are skipped entirely, leaving the game world visible beneath.

**Bounds checking in `set` and `clear`:** out-of-bounds writes silently no-op. A bad offset in a HUD subsystem should not crash the game. `debug_assert!` can be added during development to catch layout bugs early, but the release behavior is silent ignore.

---

## Integration with `App`

`App` holds a `text_layer: TextLayer` field, reconstructed on resize alongside the renderer (both depend on scene dimensions).

Each frame, the overlay buffer is built as follows:

```rust
self.text_layer.clear_all();
self.build_hud(&scene);

let mut buf = vec![0u8; w * h * 4];
self.text_layer.render_to_buf(&mut buf, &self.font);
Some(buf)
```

`build_hud` is a method on `App` with access to all game state (`current_tick`, `anim_elapsed_ms`, etc.) and the assembled scene. Each UI subsystem — FPS counter, health bar, chat, kill feed, menu — is a separate method called from `build_hud`, each writing into a designated region of the grid.

---

## Alpha Blending

The pixel buffer starts as all zeros (fully transparent). For each cell, the blend order is:

1. Background color alpha-blended over the current pixel
2. Foreground color blended onto lit glyph pixels only

Cells that are `None`, or have both `fg.a == 0` and `bg.a == 0`, contribute nothing. The game world shows through wherever no cell is set.

```rust
fn blend(dst: &mut [u8; 4], src: [u8; 4]) {
    let a = src[3] as u32;
    for i in 0..3 {
        dst[i] = ((src[i] as u32 * a + dst[i] as u32 * (255 - a)) / 255) as u8;
    }
    dst[3] = dst[3].saturating_add(src[3]);
}
```

This is compatible with the existing overlay compositing pipeline.

---

## Layout System

All layout reduces to finding a top-left origin `(col, row)` and writing characters left-to-right from there.

### Index Conversions

```
index = row * cols + col    // 2D → flat index
col   = index % cols        // flat → column
row   = index / cols        // flat → row
```

### Single-Line Positioning

Given a string of `n` characters:

| Horizontal | Formula |
|---|---|
| Left | `col_origin = 0` |
| Center | `col_origin = (cols - text_len) / 2` |
| Right | `col_origin = cols - text_len` |

| Vertical | Formula |
|---|---|
| Top | `row_origin = 0` |
| Middle | `row_origin = (rows - line_count) / 2` |
| Bottom | `row_origin = rows - line_count` |

Integer division truncation means centering may be off by one pixel — a 5-char string in an 80-col grid leaves 37 columns to the left and 38 to the right. This matches standard terminal and bitmap UI behavior. Use ceiling division `(n + 1) / 2` to bias right instead.

### Multi-Line Text

A text block is a `&[&str]`, one string per line. Measure:
- `block_height = lines.len()`
- `block_width = lines.iter().map(|l| l.len()).max()`

Apply the same origin formulas, then iterate:

```rust
for (line_idx, line) in lines.iter().enumerate() {
    let row = row_origin + line_idx;
    // write each char at col_origin + char_idx
}
```

### Anchored Corners

For HUD elements that need a fixed corner position with a margin:

```
top-left corner:     (margin_cols, margin_rows)
bottom-right corner: (cols - width - margin_cols, rows - height - margin_rows)
```

All other anchor positions are combinations of the same formula.

### `place_text` Helper

`place_text` positions a single string relative to the full grid. Two limitations to be aware of:

**Vertical middle for single lines:** the `VAlign::Middle` formula uses `saturating_sub(1)`, which gives the correct center row for a single line of text. For multi-line blocks the formula generalises to `(rows - line_count) / 2` — this is handled correctly in the multi-line section above, but `place_text` only accepts a single string, so the two are consistent as written.

**No origin offset:** `place_text` always positions relative to `(0, 0)` of the full grid. It cannot, for example, place left-aligned text inside a chat box that starts at column 10. Once subsystems start occupying sub-regions (chat boxes, menus), add either a `col_offset`/`row_offset` parameter or a separate `place_text_in_region(col_origin, row_origin, width, height, ...)` variant.

```rust
pub enum HAlign { Left, Center, Right }
pub enum VAlign { Top, Middle, Bottom }

pub fn place_text(
    layer: &mut TextLayer,
    text: &str,
    halign: HAlign,
    valign: VAlign,
    fg: [u8; 4],
    bg: [u8; 4],
) {
    let col = match halign {
        HAlign::Left   => 0,
        HAlign::Center => (layer.cols.saturating_sub(text.len())) / 2,
        HAlign::Right  => layer.cols.saturating_sub(text.len()),
    };
    let row = match valign {
        VAlign::Top    => 0,
        VAlign::Middle => (layer.rows.saturating_sub(1)) / 2,
        VAlign::Bottom => layer.rows.saturating_sub(1),
    };
    for (i, ch) in text.chars().enumerate() {
        layer.set(col + i, row, Cell { glyph: ch, fg, bg });
    }
}
```

For complex elements (scrolling chat, centered menus), compute a `(col_origin, row_origin)` directly and offset from there. No special cases needed.

---

## Text Wrapping

Word wrap follows a standard greedy algorithm:

1. Split text into words.
2. Accumulate words onto the current line, tracking `current_line_width`.
3. When the next word would overflow, flush the current line and start a new one.
4. Words longer than `max_cols` are hard-broken at the column boundary.

> **UTF-8 note:** the hard-break path currently uses `as_bytes().chunks()`, which splits on byte boundaries. This is safe for the ASCII 32+ range the font covers, but would corrupt multibyte UTF-8 characters that straddle a chunk boundary. If non-ASCII input ever becomes possible, replace with char-based chunking:
> ```rust
> let chars: Vec<char> = word.chars().collect();
> for chunk in chars.chunks(max_cols) {
>     lines.push(chunk.iter().collect());
> }
> ```

```rust
pub fn wrap_text(text: &str, max_cols: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if word.len() > max_cols {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            for chunk in word.as_bytes().chunks(max_cols) {
                lines.push(String::from_utf8_lossy(chunk).into_owned());
            }
            continue;
        }

        let needed = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };

        if needed > max_cols {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        } else {
            if !current.is_empty() { current.push(' '); }
            current.push_str(word);
        }
    }

    if !current.is_empty() { lines.push(current); }
    lines
}
```

### Vertical Clipping (Chat / Kill Feed)

To implement scroll-from-bottom with no extra state, take the last `box_rows` lines after wrapping:

```rust
let wrapped = wrap_text(message, box_cols);
let visible = &wrapped[wrapped.len().saturating_sub(box_rows)..];
```

To support scroll-up-to-read, store a signed `scroll_offset: i64` and slice accordingly.

---

## Relationship to `font.rs`

`Font`, `Glyph`, and the glyph size constants remain in `font.rs` unchanged. The existing `draw_text` function can stay for now (it is used by the font test), but `render_to_buf` replaces it as the primary rendering path. `draw_text` can be removed once the font test is rewritten against `TextLayer`.

---

## Design Rationale

The core benefit of this system is that every HUD subsystem works purely in grid coordinates. There is no pixel math, no manual buffer slicing, and no per-subsystem offset tracking. Centering and pixel layout are computed once in `TextLayer::new()` and are invisible to callers at render time.