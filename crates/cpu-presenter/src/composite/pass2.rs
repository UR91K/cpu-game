use crate::composite::{colorspace::yiq_to_rgb, filters::*};

pub struct FirPlan {
    input_width: usize,
    output_width: usize,
    center: Vec<u32>,
}

pub fn build_plan(input_width: usize, output_width: usize) -> FirPlan {
    let center = (0..output_width)
        .map(|x| (x * 2).min(input_width - 1) as u32)
        .collect();

    FirPlan {
        input_width,
        output_width,
        center,
    }
}

pub fn pass2_row(
    in_line: &[[f32; 3]],
    out_line: &mut [[f32; 3]],
    output_width: usize,
    gamma_exp: f32,
    plan: &FirPlan,
) {
    // Determine gamma variant once per row rather than branching per pixel.
    enum Gamma { Identity, Sqrt, Pow125, Square, General(f32) }
    let gv = if (gamma_exp - 1.0).abs() < 1e-4 { Gamma::Identity }
        else if (gamma_exp - 0.5).abs() < 1e-4 { Gamma::Sqrt }
        else if (gamma_exp - 1.25).abs() < 1e-4 { Gamma::Pow125 }
        else if (gamma_exp - 2.0).abs() < 1e-4 { Gamma::Square }
        else { Gamma::General(gamma_exp) };

    #[inline(always)]
    fn g(x: f32, gv: &Gamma) -> f32 {
        match gv {
            Gamma::Identity => x,
            Gamma::Sqrt => x.sqrt(),
            Gamma::Pow125 => { let s = x.sqrt(); x * s.sqrt() },
            Gamma::Square => x * x,
            Gamma::General(e) => x.powf(*e),
        }
    }

    let input_width_m1 = plan.input_width as u32 - 1;
    for x in 0..output_width {
        let mut sig_y = 0.0f32;
        let mut sig_i = 0.0f32;
        let mut sig_q = 0.0f32;

        let cx = plan.center[x];
        for tap in 0..TAPS {
            let o = (TAPS - tap) as u32;
            let lx = cx.saturating_sub(o) as usize;
            let rx = (cx + o).min(input_width_m1) as usize;
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

        let (r, g_c, b) = yiq_to_rgb(sig_y, sig_i, sig_q);
        out_line[x] = [
            g(r.max(0.0), &gv),
            g(g_c.max(0.0), &gv),
            g(b.max(0.0), &gv),
        ];
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
        .chunks_mut(output_width)
        .zip(encoded.chunks(input_width))
        .for_each(|(out_line, in_line)| {
            let iw_m1 = plan.input_width as u32 - 1;
            for x in 0..output_width {
                let mut sig_y = 0.0f32;
                let mut sig_i = 0.0f32;
                let mut sig_q = 0.0f32;

                let cx = plan.center[x];
                for tap in 0..TAPS {
                    let o = (TAPS - tap) as u32;
                    let lx = cx.saturating_sub(o) as usize;
                    let rx = (cx + o).min(iw_m1) as usize;
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