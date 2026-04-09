use rayon::prelude::*;

use crate::composite::{colorspace::yiq2rgb, filters::*};

pub fn pass2(encoded: &[[f32; 3]], decoded: &mut [[f32; 3]], width: usize, gamma_exp: f32) {
    decoded
        .par_chunks_mut(width)
        .zip(encoded.par_chunks(width))
        .for_each(|(out_line, in_line)| {
            for x in 0..width {
                let mut sig_y = 0.0f32;
                let mut sig_i = 0.0f32;
                let mut sig_q = 0.0f32;

                for tap in 0..TAPS {
                    let lx = x.saturating_sub(TAPS - tap);
                    let rx = (x + TAPS - tap).min(width - 1);
                    let [ly, li, lq] = in_line[lx];
                    let [ry, ri, rq] = in_line[rx];

                    sig_y += (ly + ry) * LUMA_FILTER[tap];
                    sig_i += (li + ri) * CHROMA_FILTER[tap];
                    sig_q += (lq + rq) * CHROMA_FILTER[tap];
                }

                let [cy, ci, cq] = in_line[x];
                sig_y += cy * LUMA_FILTER[TAPS];
                sig_i += ci * CHROMA_FILTER[TAPS];
                sig_q += cq * CHROMA_FILTER[TAPS];

                let (r, g, b) = yiq2rgb(sig_y, sig_i, sig_q);
                out_line[x] = [
                    r.max(0.0).powf(gamma_exp),
                    g.max(0.0).powf(gamma_exp),
                    b.max(0.0).powf(gamma_exp),
                ];
            }
        });
}
