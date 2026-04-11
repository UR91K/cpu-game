use rayon::prelude::*;

use crate::composite::{colorspace::yiq_to_rgb, filters::*};

pub struct FirPlan {
    input_width: usize,
    output_width: usize,
    center: Vec<u32>,
    pairs: Vec<[u32; TAPS * 2]>,
}

pub fn build_plan(input_width: usize, output_width: usize) -> FirPlan {
    let mut center = Vec::with_capacity(output_width);
    let mut pairs = Vec::with_capacity(output_width);

    for x in 0..output_width {
        // Match shader pass2 decimation by 0.5 with half-texel compensation:
        // the base sample lands on even source texels.
        let cx = (x * 2).min(input_width - 1) as u32;
        center.push(cx);
        let mut row = [0u32; TAPS * 2];
        for tap in 0..TAPS {
            let o = (TAPS - tap) as u32;
            let lx = cx.saturating_sub(o);
            let rx = (cx + o).min((input_width - 1) as u32);
            row[tap * 2] = lx;
            row[tap * 2 + 1] = rx;
        }
        pairs.push(row);
    }

    FirPlan {
        input_width,
        output_width,
        center,
        pairs,
    }
}

pub fn pass2(
    encoded: &[[f32; 3]],
    decoded: &mut [[f32; 3]],
    input_width: usize,
    output_width: usize,
    gamma_exp: f32,
    plan: &FirPlan,
) {
    assert!(input_width == plan.input_width, "FirPlan input_width mismatch");
    assert!(output_width == plan.output_width, "FirPlan output_width mismatch");

    decoded
        .par_chunks_mut(output_width)
        .zip(encoded.par_chunks(input_width))
        .for_each(|(out_line, in_line)| {
            for x in 0..output_width {
                let mut sig_y = 0.0f32;
                let mut sig_i = 0.0f32;
                let mut sig_q = 0.0f32;

                let idx = &plan.pairs[x];
                for tap in 0..TAPS {
                    let lx = idx[tap * 2] as usize;
                    let rx = idx[tap * 2 + 1] as usize;
                    let [ly, li, lq] = in_line[lx];
                    let [ry, ri, rq] = in_line[rx];

                    sig_y += (ly + ry) * LUMA_FILTER[tap];
                    sig_i += (li + ri) * CHROMA_FILTER[tap];
                    sig_q += (lq + rq) * CHROMA_FILTER[tap];
                }

                let [cy, ci, cq] = in_line[plan.center[x] as usize];
                sig_y += cy * LUMA_FILTER[TAPS];
                sig_i += ci * CHROMA_FILTER[TAPS];
                sig_q += cq * CHROMA_FILTER[TAPS];

                let (r, g, b) = yiq_to_rgb(sig_y, sig_i, sig_q);
                out_line[x] = [
                    r.max(0.0).powf(gamma_exp),
                    g.max(0.0).powf(gamma_exp),
                    b.max(0.0).powf(gamma_exp),
                ];
            }
        });
}