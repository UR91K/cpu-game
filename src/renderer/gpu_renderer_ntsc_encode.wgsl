struct EncodeUniforms {
    source_size: vec2<f32>,
    output_size: vec2<f32>,
    frame_phase: f32,
    chroma_mod_freq: f32,
    _pad0: vec2<f32>,
    mix_row0: vec4<f32>,
    mix_row1: vec4<f32>,
    mix_row2: vec4<f32>,
};

@group(0) @binding(0) var t_source: texture_2d<f32>;
@group(0) @binding(1) var s_source: sampler;
@group(0) @binding(2) var<uniform> uniforms: EncodeUniforms;

struct VertOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

fn linear_channel_to_srgb(value: f32) -> f32 {
    if (value <= 0.0031308) {
        return 12.92 * value;
    }
    return 1.055 * pow(value, 1.0 / 2.4) - 0.055;
}

fn linear_to_srgb(color: vec3<f32>) -> vec3<f32> {
    let clamped = max(color, vec3<f32>(0.0));
    return vec3<f32>(
        linear_channel_to_srgb(clamped.r),
        linear_channel_to_srgb(clamped.g),
        linear_channel_to_srgb(clamped.b),
    );
}

fn rgb2yiq(col: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        0.2989 * col.r + 0.5870 * col.g + 0.1140 * col.b,
        0.5959 * col.r - 0.2744 * col.g - 0.3216 * col.b,
        0.2115 * col.r - 0.5229 * col.g + 0.3114 * col.b,
    );
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VertOut {
    var xy = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0),
    );
    var uv = array<vec2<f32>, 3>(
        vec2(0.0, 1.0),
        vec2(2.0, 1.0),
        vec2(0.0, -1.0),
    );

    var out: VertOut;
    out.pos = vec4(xy[i], 0.0, 1.0);
    out.uv = uv[i];
    return out;
}

@fragment
fn fs_main(input: VertOut) -> @location(0) vec4<f32> {
    let col = linear_to_srgb(textureSample(t_source, s_source, input.uv).rgb);
    var yiq = rgb2yiq(col);
    let pix_no = input.uv * uniforms.output_size;
    let chroma_phase = 3.14159265 * (f32(u32(pix_no.y) & 1u) + uniforms.frame_phase);
    let mod_phase = chroma_phase + pix_no.x * uniforms.chroma_mod_freq;
    let i_mod = cos(mod_phase);
    let q_mod = sin(mod_phase);

    yiq.y = yiq.y * i_mod;
    yiq.z = yiq.z * q_mod;
    let y2 = dot(yiq, uniforms.mix_row0.xyz);
    let i2 = dot(yiq, uniforms.mix_row1.xyz);
    let q2 = dot(yiq, uniforms.mix_row2.xyz);
    let demod = vec2<f32>(i2 * i_mod, q2 * q_mod);
    return vec4<f32>(y2, demod.x, demod.y, 1.0);
}