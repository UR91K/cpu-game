# cpu-presenter

CPU implementation of the two-pass NTSC composite pipeline used by the ntsc libretro shader.

Mapping:
- Pass 1 encode/demodulate: src/composite/pass1.rs
- Pass 2 FIR decode + gamma: src/composite/pass2.rs
- FIR coefficients: src/composite/filters.rs
- Colorspace conversion: src/composite/colorspace.rs
- Horizontal expansion: src/composite/lanczos.rs
