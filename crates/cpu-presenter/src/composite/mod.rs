pub mod colorspace;
pub mod filters;
pub mod lanczos;
pub mod params;
pub mod pass1;
pub mod pass2;

use std::time::Instant;

use params::CompositeParams;

pub struct CompositeProcessor {
    pub params: CompositeParams,
    start_time: Instant,
    expanded: Vec<[f32; 3]>,
    encoded: Vec<[f32; 3]>,
    decoded: Vec<[f32; 3]>,
    pub rgba_out: Vec<u8>,
    output_width: usize,
    output_height: usize,
}

impl CompositeProcessor {
    pub fn new(params: CompositeParams, src_width: usize, src_height: usize) -> Self {
        let out_w = src_width * params.horizontal_scale as usize;
        let n = out_w * src_height;
        Self {
            params,
            start_time: Instant::now(),
            expanded: vec![[0.0; 3]; n],
            encoded: vec![[0.0; 3]; n],
            decoded: vec![[0.0; 3]; n],
            rgba_out: vec![0u8; n * 4],
            output_width: out_w,
            output_height: src_height,
        }
    }

    fn ensure_size(&mut self, src_w: usize, src_h: usize) {
        let desired_w = src_w * self.params.horizontal_scale as usize;
        if desired_w == self.output_width && src_h == self.output_height {
            return;
        }

        let n = desired_w * src_h;
        self.output_width = desired_w;
        self.output_height = src_h;
        self.expanded.resize(n, [0.0; 3]);
        self.encoded.resize(n, [0.0; 3]);
        self.decoded.resize(n, [0.0; 3]);
        self.rgba_out.resize(n * 4, 0u8);
    }

    pub fn process(&mut self, pixel_data: &[u8], src_w: usize, src_h: usize) -> &[u8] {
        self.ensure_size(src_w, src_h);

        let elapsed = self.start_time.elapsed().as_secs_f64();
        let ntsc_field = (elapsed * self.params.ntsc_field_rate) as u64;

        lanczos::expand_horizontal(
            pixel_data,
            src_w,
            src_h,
            &mut self.expanded,
            self.output_width,
        );

        pass1::pass1(
            &self.expanded,
            &mut self.encoded,
            self.output_width,
            src_h,
            ntsc_field,
            &self.params,
        );

        pass2::pass2(
            &self.encoded,
            &mut self.decoded,
            self.output_width,
            self.params.gamma_exp(),
        );

        self.finalize_rgba();
        &self.rgba_out
    }

    fn finalize_rgba(&mut self) {
        for (src, dst) in self.decoded.iter().zip(self.rgba_out.chunks_exact_mut(4)) {
            dst[0] = (src[0].clamp(0.0, 1.0) * 255.0) as u8;
            dst[1] = (src[1].clamp(0.0, 1.0) * 255.0) as u8;
            dst[2] = (src[2].clamp(0.0, 1.0) * 255.0) as u8;
            dst[3] = 255;
        }
    }

    pub fn output_size(&self) -> (u32, u32) {
        (self.output_width as u32, self.output_height as u32)
    }
}
