use std::f32::consts::PI;

use rayon::prelude::*;

use crate::composite::{colorspace::*, params::CompositeParams};

pub const CHROMA_MOD_FREQ: f32 = 4.0 * PI / 15.0;

pub fn build_modulation_table(width: usize) -> Vec<[f32; 2]> {
    let mut table = Vec::with_capacity(width);
    for x in 0..width {
        let phase = x as f32 * CHROMA_MOD_FREQ;
        table.push([phase.cos(), phase.sin()]);
    }
    table
}

pub fn pass1(
    expanded: &[[f32; 3]],
    encoded: &mut [[f32; 3]],
    width: usize,
    height: usize,
    ntsc_field: u64,
    params: &CompositeParams,
    modulation_table: &[[f32; 2]],
) {
    let mat = params.mix_mat();

    encoded
        .par_chunks_mut(width)
        .zip(expanded.par_chunks(width))
        .enumerate()
        .take(height)
        .for_each(|(line_y, (out_line, in_line))| {
            let sign = if (((line_y as u64) + ntsc_field) & 1) == 0 {
                1.0f32
            } else {
                -1.0f32
            };

            for x in 0..width {
                let [r, g, b] = in_line[x];
                let (y, i, q) = rgb2yiq(r, g, b);

                let base = modulation_table[x];
                let i_mod = base[0] * sign;
                let q_mod = base[1] * sign;

                let (y2, i2, q2) = (y, i * i_mod, q * q_mod);
                let (y3, i3, q3) = mix_mat_mul(y2, i2, q2, &mat);
                out_line[x] = [y3, i3 * i_mod, q3 * q_mod];
            }
        });
}
