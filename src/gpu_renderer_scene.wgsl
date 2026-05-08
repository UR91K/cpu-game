struct SceneUniforms {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var t_atlas: texture_2d<f32>;
@group(0) @binding(1) var s_atlas: sampler;
@group(0) @binding(2) var<uniform> uniforms: SceneUniforms;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv_times_w: vec2<f32>,
    @location(1) clip_w: f32,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.position = uniforms.view_proj * vec4(input.position, 1.0);
    out.uv_times_w = input.uv * out.position.w;
    out.clip_w = out.position.w;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let w_sign = select(-1.0, 1.0, input.clip_w >= 0.0);
    let w = w_sign * max(abs(input.clip_w), 1e-6);
    let uv = input.uv_times_w / w;
    let color = textureSample(t_atlas, s_atlas, uv);
    if (color.a < 0.05) {
        discard;
    }
    return color;
}