use bytemuck::{Pod, Zeroable};

pub const NTSC_PHASE_FLIP_HZ: f32 = 29.97002997;
pub const CHROMA_MOD_FREQ: f32 = 4.0 * std::f32::consts::PI / 15.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SceneVertex {
    pub(crate) position: [f32; 3],
    pub(crate) uv: [f32; 2],
}

impl SceneVertex {
    pub const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SceneUniforms {
    pub(crate) view_proj: [[f32; 4]; 4],
    pub(crate) affine_params: [f32; 4],
    pub(crate) snap_params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct NtscEncodeUniforms {
    source_size: [f32; 2],
    output_size: [f32; 2],
    frame_phase: f32,
    chroma_mod_freq: f32,
    _pad0: [f32; 2],
    mix_row0: [f32; 4],
    mix_row1: [f32; 4],
    mix_row2: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct NtscDecodeUniforms {
    source_size: [f32; 2],
    gamma_exp: f32,
    _pad0: f32,
}


pub fn build_encode_uniforms(
    scene_width: u32,
    scene_height: u32,
    anim_elapsed_ms: f64,
) -> NtscEncodeUniforms {
    let elapsed_seconds = (anim_elapsed_ms * 0.001) as f32;
    let frame_phase = (elapsed_seconds * NTSC_PHASE_FLIP_HZ)
        .floor()
        .rem_euclid(2.0);

    NtscEncodeUniforms {
        source_size: [scene_width as f32, scene_height as f32],
        output_size: [(scene_width * 4) as f32, scene_height as f32],
        frame_phase,
        chroma_mod_freq: CHROMA_MOD_FREQ,
        _pad0: [0.0; 2],
        mix_row0: [1.0, 1.0, 1.0, 0.0],
        mix_row1: [1.0, 2.0, 0.0, 0.0],
        mix_row2: [1.0, 0.0, 2.0, 0.0],
    }
}

pub fn build_decode_uniforms(composite_width: u32, composite_height: u32) -> NtscDecodeUniforms {
    NtscDecodeUniforms {
        source_size: [composite_width as f32, composite_height as f32],
        gamma_exp: 2.5 / 2.0,
        _pad0: 0.0,
    }
}
