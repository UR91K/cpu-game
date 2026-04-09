---

## Technical Design Document: `cpu-presenter`

### Overview

A drop-in replacement for the librashader-based `ShaderRenderer`. Takes a `PresentationRequest`, runs the NTSC composite simulation entirely on CPU, uploads the result to wgpu, and blits to a winit surface. The public API is intentionally identical to the existing crate.

---

### Crate structure

```
cpu-presenter/
├── Cargo.toml
├── src/
│   ├── lib.rs                  # re-exports ShaderRenderer
│   ├── renderer.rs             # ShaderRenderer — public API, wgpu setup
│   ├── composite/
│   │   ├── mod.rs              # CompositeProcessor
│   │   ├── params.rs           # CompositeParams (serializable)
│   │   ├── pass1.rs            # encode/demodulate per scanline
│   │   ├── pass2.rs            # FIR decode per scanline
│   │   ├── filters.rs          # LUMA_FILTER, CHROMA_FILTER constants
│   │   ├── lanczos.rs          # horizontal resampler 640 → 2560
│   │   └── colorspace.rs       # rgb2yiq, yiq2rgb, mix_mat_mul
│   └── blit/
│       ├── mod.rs              # BlitPipeline
│       └── blit.wgsl           # fullscreen triangle shader
```

---

### `CompositeParams`

All tunable values in one serializable struct. Rayon workers take a shared read reference each frame.

```rust
// composite/params.rs
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompositeParams {
    // mix_mat semantic controls
    pub brightness:  f32,   // default 1.0
    pub saturation:  f32,   // default 1.0
    pub artifacting: f32,   // default 1.0  (0 = S-Video)
    pub fringing:    f32,   // default 1.0

    // signal
    pub ntsc_field_rate: f64,  // default 29.97 Hz, drives dot crawl
    pub horizontal_scale: u32, // default 4  (640 * 4 = 2560)

    // gamma
    pub source_gamma:  f32,    // default 2.5 (CRT)
    pub target_gamma:  f32,    // default 2.0 (monitor)

    // noise floor
    pub noise_amplitude: f32,  // default 0.0 (add ~0.015 for capture card noise)
}

impl Default for CompositeParams {
    fn default() -> Self {
        Self {
            brightness: 1.0, saturation: 1.0,
            artifacting: 1.0, fringing: 1.0,
            ntsc_field_rate: 29.97,
            horizontal_scale: 4,
            source_gamma: 2.5, target_gamma: 2.0,
            noise_amplitude: 0.0,
        }
    }
}

impl CompositeParams {
    /// Build the 3×3 mix matrix from semantic params.
    /// Row-major, applied as: yiq = mix_mat * yiq
    pub fn mix_mat(&self) -> [[f32; 3]; 3] {
        [
            [self.brightness, self.fringing,    self.fringing   ],
            [self.artifacting, 2.0 * self.saturation, 0.0       ],
            [self.artifacting, 0.0,             2.0 * self.saturation],
        ]
    }

    pub fn gamma_exp(&self) -> f32 {
        self.source_gamma / self.target_gamma
    }
}
```

---

### Filter coefficients

Exact values from the libretro shader, as `const` arrays.

```rust
// composite/filters.rs
pub const TAPS: usize = 32;

pub const LUMA_FILTER: [f32; TAPS + 1] = [
    -0.000174844, -0.000205844, -0.000149453, -0.000051693,
     0.000000000, -0.000066171, -0.000245058, -0.000432928,
    -0.000472644, -0.000252236,  0.000198929,  0.000687058,
     0.000944112,  0.000803467,  0.000363199,  0.000013422,
     0.000253402,  0.001339461,  0.002932972,  0.003983485,
     0.003026683, -0.001102056, -0.008373026, -0.016897700,
    -0.022914480, -0.021642347, -0.008863273,  0.017271957,
     0.054921920,  0.098342579,  0.139044281,  0.168055832,
     0.178571429,
];

pub const CHROMA_FILTER: [f32; TAPS + 1] = [
    0.001384762, 0.001678312, 0.002021715, 0.002420562,
    0.002880460, 0.003406879, 0.004004985, 0.004679445,
    0.005434218, 0.006272332, 0.007195654, 0.008204665,
    0.009298238, 0.010473450, 0.011725413, 0.013047155,
    0.014429548, 0.015861306, 0.017329037, 0.018817382,
    0.020309220, 0.021785952, 0.023227857, 0.024614500,
    0.025925203, 0.027139546, 0.028237893, 0.029201910,
    0.030015081, 0.030663170, 0.031134640, 0.031420995,
    0.031517031,
];
```

---

### Colour space

```rust
// composite/colorspace.rs

#[inline(always)]
pub fn rgb2yiq(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    (
        0.2989 * r + 0.5870 * g + 0.1140 * b,
        0.5959 * r - 0.2744 * g - 0.3216 * b,
        0.2115 * r - 0.5229 * g + 0.3114 * b,
    )
}

#[inline(always)]
pub fn yiq2rgb(y: f32, i: f32, q: f32) -> (f32, f32, f32) {
    (
        y + 0.9560 * i + 0.6210 * q,
        y - 0.2720 * i - 0.6474 * q,
        y - 1.1060 * i + 1.7046 * q,
    )
}

#[inline(always)]
pub fn mix_mat_mul(y: f32, i: f32, q: f32, mat: &[[f32; 3]; 3]) -> (f32, f32, f32) {
    (
        mat[0][0] * y + mat[0][1] * i + mat[0][2] * q,
        mat[1][0] * y + mat[1][1] * i + mat[1][2] * q,
        mat[2][0] * y + mat[2][1] * i + mat[2][2] * q,
    )
}
```

---

### Pass 1 — encode/demodulate

```rust
// composite/pass1.rs
use std::f32::consts::PI;
use rayon::prelude::*;
use crate::composite::{colorspace::*, params::CompositeParams};

pub const CHROMA_MOD_FREQ: f32 = 4.0 * PI / 15.0;

/// encoded: YIQ f32, width * height, written in place
pub fn pass1(
    expanded: &[[f32; 3]],        // RGB f32, 2560 * height
    encoded:  &mut [[f32; 3]],    // YIQ f32 out, same size
    width: usize,
    height: usize,
    ntsc_field: u64,              // drives dot crawl, derived from wall clock
    params: &CompositeParams,
) {
    let mat = params.mix_mat();

    encoded
        .par_chunks_mut(width)
        .zip(expanded.par_chunks(width))
        .enumerate()
        .for_each(|(line_y, (out_line, in_line))| {
            // two-phase: phase alternates per line + per field
            let chroma_phase =
                PI * ((line_y % 2) as f32 + (ntsc_field % 2) as f32);

            for x in 0..width {
                let [r, g, b] = in_line[x];
                let (y, i, q) = rgb2yiq(r, g, b);

                let mod_phase = chroma_phase + x as f32 * CHROMA_MOD_FREQ;
                let (i_mod, q_mod) = (mod_phase.cos(), mod_phase.sin());

                // modulate
                let (y2, i2, q2) = (y, i * i_mod, q * q_mod);
                // cross-talk
                let (y3, i3, q3) = mix_mat_mul(y2, i2, q2, &mat);
                // demodulate
                out_line[x] = [y3, i3 * i_mod, q3 * q_mod];
            }
        });
}
```

The cos/sin calls are the only remaining cost here. If profiling shows them as a bottleneck, precompute a `Vec<(f32, f32)>` of `(cos, sin)` for all 2560 x-positions once per line-start — since `chroma_phase` is constant within a scanline and `CHROMA_MOD_FREQ` is constant, the table is fully deterministic and can be reused across all lines of the same parity.

---

### Pass 2 — FIR decode

```rust
// composite/pass2.rs
use rayon::prelude::*;
use crate::composite::{colorspace::yiq2rgb, filters::*};

pub fn pass2(
    encoded: &[[f32; 3]],
    decoded: &mut [[f32; 3]],
    width: usize,
    gamma_exp: f32,
) {
    decoded
        .par_chunks_mut(width)
        .zip(encoded.par_chunks(width))
        .for_each(|(out_line, in_line)| {
            for x in 0..width {
                let mut sig_y = 0.0f32;
                let mut sig_i = 0.0f32;
                let mut sig_q = 0.0f32;

                // symmetric FIR — pairs of taps cancel allocation
                for tap in 0..TAPS {
                    let lx = x.saturating_sub(TAPS - tap);
                    let rx = (x + TAPS - tap).min(width - 1);
                    let [ly, li, lq] = in_line[lx];
                    let [ry, ri, rq] = in_line[rx];

                    sig_y += (ly + ry) * LUMA_FILTER[tap];
                    sig_i += (li + ri) * CHROMA_FILTER[tap];
                    sig_q += (lq + rq) * CHROMA_FILTER[tap];
                }

                // centre tap
                let [cy, ci, cq] = in_line[x];
                sig_y += cy * LUMA_FILTER[TAPS];
                sig_i += ci * CHROMA_FILTER[TAPS];
                sig_q += cq * CHROMA_FILTER[TAPS];

                let (r, g, b) = yiq2rgb(sig_y, sig_i, sig_q);

                // gamma correction
                out_line[x] = [
                    r.max(0.0).powf(gamma_exp),
                    g.max(0.0).powf(gamma_exp),
                    b.max(0.0).powf(gamma_exp),
                ];
            }
        });
}
```

---

### CompositeProcessor

The top-level coordinator. Owns all working buffers, allocated once.

```rust
// composite/mod.rs
use std::time::Instant;
use crate::composite::{params::CompositeParams, lanczos, pass1, pass2};

pub struct CompositeProcessor {
    pub params: CompositeParams,
    start_time: Instant,

    // working buffers — allocated once at startup, never reallocated
    expanded:   Vec<[f32; 3]>,   // after lanczos:  2560 * h
    encoded:    Vec<[f32; 3]>,   // after pass1 YIQ: 2560 * h
    decoded:    Vec<[f32; 3]>,   // after pass2 RGB: 2560 * h
    pub rgba_out: Vec<u8>,       // u8 RGBA for GPU upload: 2560 * h * 4
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
            expanded:  vec![[0.0; 3]; n],
            encoded:   vec![[0.0; 3]; n],
            decoded:   vec![[0.0; 3]; n],
            rgba_out:  vec![0u8; n * 4],
            output_width: out_w,
            output_height: src_height,
        }
    }

    /// Run the full pipeline. Returns a slice into rgba_out.
    pub fn process(&mut self, pixel_data: &[u8], src_w: usize, src_h: usize) -> &[u8] {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let ntsc_field = (elapsed * self.params.ntsc_field_rate) as u64;

        lanczos::expand_horizontal(pixel_data, src_w, src_h,
            &mut self.expanded, self.output_width);

        pass1::pass1(&self.expanded, &mut self.encoded,
            self.output_width, src_h, ntsc_field, &self.params);

        pass2::pass2(&self.encoded, &mut self.decoded,
            self.output_width, self.params.gamma_exp());

        self.finalize_rgba();
        &self.rgba_out
    }

    fn finalize_rgba(&mut self) {
        for (src, dst) in self.decoded.iter()
            .zip(self.rgba_out.chunks_exact_mut(4))
        {
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
```

---

### Lanczos horizontal resampler

```rust
// composite/lanczos.rs

const LANCZOS_A: f32 = 3.0; // 3-lobe

#[inline]
fn lanczos_kernel(x: f32) -> f32 {
    if x.abs() < f32::EPSILON { return 1.0; }
    if x.abs() >= LANCZOS_A   { return 0.0; }
    let px = std::f32::consts::PI * x;
    (px.sin() / px) * ((px / LANCZOS_A).sin() / (px / LANCZOS_A))
}

/// Expand each scanline from src_w to dst_w using Lanczos-3.
/// Input is u8 RGBA packed, output is f32 RGB planar-ish ([f32;3]).
pub fn expand_horizontal(
    src: &[u8],
    src_w: usize, src_h: usize,
    dst: &mut [[f32; 3]],
    dst_w: usize,
) {
    use rayon::prelude::*;
    let scale = src_w as f32 / dst_w as f32; // < 1.0 for upscale

    dst.par_chunks_mut(dst_w)
        .enumerate()
        .for_each(|(y, out_line)| {
            let src_row = &src[y * src_w * 4..(y + 1) * src_w * 4];
            for dx in 0..dst_w {
                let src_x = (dx as f32 + 0.5) * scale - 0.5;
                let x0 = (src_x - LANCZOS_A).ceil() as i32;
                let x1 = (src_x + LANCZOS_A).floor() as i32;
                let (mut r, mut g, mut b, mut w) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                for sx in x0..=x1 {
                    let clamped = sx.clamp(0, src_w as i32 - 1) as usize;
                    let weight = lanczos_kernel(src_x - sx as f32);
                    let base = clamped * 4;
                    r += src_row[base    ] as f32 * weight;
                    g += src_row[base + 1] as f32 * weight;
                    b += src_row[base + 2] as f32 * weight;
                    w += weight;
                }
                out_line[dx] = [r / (w * 255.0), g / (w * 255.0), b / (w * 255.0)];
            }
        });
}
```

---

### Blit shader

```wgsl
// blit/blit.wgsl

@group(0) @binding(0) var t_composite: texture_2d<f32>;
@group(0) @binding(1) var s_composite: sampler;

struct VertOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VertOut {
    // fullscreen triangle, no vertex buffer
    var xy = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0),
    );
    var uv = array<vec2<f32>, 3>(
        vec2(0.0, 1.0),
        vec2(2.0, 1.0),
        vec2(0.0, -1.0),
    );
    var out: VertOut;
    out.pos = vec4(xy[i], 0.0, 1.0);
    out.uv  = uv[i];
    return out;
}

@fragment
fn fs_main(in: VertOut) -> @location(0) vec4<f32> {
    return textureSample(t_composite, s_composite, in.uv);
}
```

---

### ShaderRenderer — public API

This is what the existing `App` code calls. The surface is identical to the librashader version.

```rust
// renderer.rs
use std::sync::Arc;
use std::time::Instant;
use anyhow::{anyhow, Result};
use wgpu::{Device, Queue};
use engine_core::PresentationRequest;
use crate::composite::{CompositeProcessor, params::CompositeParams};
use crate::blit::BlitPipeline;

pub struct ShaderRenderer {
    pub device: Arc<Device>,
    pub queue:  Arc<Queue>,
    composite:  CompositeProcessor,
    blit:       BlitPipeline,
    start_time: Instant,
    frame_count: usize,
}

impl ShaderRenderer {
    pub fn new(device: Arc<Device>, queue: Arc<Queue>) -> Self {
        let params = CompositeParams::default();
        // pre-allocate for 640×480 source
        let composite = CompositeProcessor::new(params, 640, 480);
        let blit = BlitPipeline::new(&device);
        Self { device, queue, composite, blit, start_time: Instant::now(), frame_count: 0 }
    }

    pub fn load_default_preset(&mut self) -> Result<()> {
        Ok(()) // params are already set to composite defaults
    }

    /// Runs CPU composite and stages result for GPU upload.
    pub fn load_presentation(&mut self, request: &PresentationRequest) {
        assert!(request.is_valid());
        let rgba = self.composite.process(
            &request.pixel_data,
            request.width as usize,
            request.height as usize,
        );
        let (ow, oh) = self.composite.output_size();
        self.blit.upload(&self.queue, rgba, ow, oh);
    }

    pub fn render_frame_to_viewport(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        output_size: (u32, u32),
        output_format: wgpu::TextureFormat,
        viewport_x: u32,
        viewport_y: u32,
    ) -> Result<()> {
        self.blit.render(encoder, output_view, output_size,
            output_format, viewport_x, viewport_y)?;
        self.frame_count += 1;
        Ok(())
    }

    pub fn calculate_aspect_preserving_viewport(
        window_width: u32, window_height: u32,
        content_width: u32, content_height: u32,
    ) -> (u32, u32, u32, u32) {
        let wa = window_width as f32 / window_height as f32;
        let ca = content_width as f32 / content_height as f32;
        if wa > ca {
            let sw = window_height as f32 * ca;
            let x = (window_width as f32 - sw) / 2.0;
            (x as u32, 0, sw as u32, window_height)
        } else {
            let sh = window_width as f32 / ca;
            let y = (window_height as f32 - sh) / 2.0;
            (0, y as u32, window_width, sh as u32)
        }
    }

    pub fn frame_count(&self) -> usize { self.frame_count }
    pub fn reset_frame_count(&mut self) { self.frame_count = 0; }
    pub fn reset_animation_time(&mut self) { self.start_time = Instant::now(); }
    pub fn has_input(&self) -> bool { true }
}
```

---

### `Cargo.toml` dependencies

```toml
[package]
name = "cpu-presenter"
version = "0.1.0"
edition = "2021"

[dependencies]
engine-core    = { path = "../engine-core" }
wgpu           = "22"
winit          = "0.30"
rayon          = "1"
anyhow         = "1"
serde          = { version = "1", features = ["derive"] }
pollster       = "0.3"

# optional: for egui parameter panel later
# egui          = "0.27"
# egui-wgpu     = "0.27"
# egui-winit    = "0.27"
```

---

### What is not yet in this document

These are the natural next pieces but intentionally left out of the initial build scope:

- `BlitPipeline` internals (wgpu pipeline creation, bind group layout, texture management) — straightforward wgpu boilerplate
- SIMD optimisation of the FIR inner loop — profile first, then reach for `wide` or `std::arch`
- The egui parameter panel — add after the baseline is working and profiled
- Preset save/load via `serde` + `toml` — trivial once `CompositeParams` is stable
- Three-phase mode (SNES/other consoles) — a `#[derive]` enum on `CompositeParams` and a branch in `pass1`