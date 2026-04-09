use rayon::prelude::*;

#[derive(Clone, Copy)]
struct Tap {
    src_x: u32,
    weight: f32,
}

#[derive(Clone, Copy)]
struct TapSpan {
    start: u32,
    len: u16,
    inv_norm_255: f32,
}

pub struct HorizontalPlan {
    src_w: usize,
    dst_w: usize,
    spans: Vec<TapSpan>,
    taps: Vec<Tap>,
}

pub fn build_plan(src_w: usize, dst_w: usize) -> HorizontalPlan {
    let scale = src_w as f32 / dst_w as f32;
    let mut spans = Vec::with_capacity(dst_w);
    let mut taps = Vec::with_capacity(dst_w);

    for dx in 0..dst_w {
        // Nearest-neighbor sample in source pixel-center space.
        let src_x = (dx as f32 + 0.5) * scale - 0.5;
        let nearest = src_x.round().clamp(0.0, (src_w - 1) as f32) as usize;
        let start = taps.len() as u32;
        taps.push(Tap {
            src_x: nearest as u32,
            weight: 1.0,
        });

        let len = (taps.len() as u32 - start) as u16;
        let inv_norm_255 = 1.0 / 255.0;

        spans.push(TapSpan {
            start,
            len,
            inv_norm_255,
        });
    }

    HorizontalPlan {
        src_w,
        dst_w,
        spans,
        taps,
    }
}

pub fn expand_horizontal(
    src: &[u8],
    src_w: usize,
    src_h: usize,
    dst: &mut [[f32; 3]],
    dst_w: usize,
    plan: &HorizontalPlan,
) {
    assert!(src_w == plan.src_w, "HorizontalPlan src_w mismatch");
    assert!(dst_w == plan.dst_w, "HorizontalPlan dst_w mismatch");

    dst.par_chunks_mut(dst_w)
        .enumerate()
        .take(src_h)
        .for_each(|(y, out_line)| {
            let src_row = &src[y * src_w * 4..(y + 1) * src_w * 4];
            for (dx, dst_px) in out_line.iter_mut().enumerate() {
                let span = plan.spans[dx];
                let mut r = 0.0f32;
                let mut g = 0.0f32;
                let mut b = 0.0f32;

                let start = span.start as usize;
                let end = start + span.len as usize;
                for tap in &plan.taps[start..end] {
                    let base = tap.src_x as usize * 4;
                    let weight = tap.weight;
                    r += src_row[base] as f32 * weight;
                    g += src_row[base + 1] as f32 * weight;
                    b += src_row[base + 2] as f32 * weight;
                }

                *dst_px = [r * span.inv_norm_255, g * span.inv_norm_255, b * span.inv_norm_255];
            }
        });
}
