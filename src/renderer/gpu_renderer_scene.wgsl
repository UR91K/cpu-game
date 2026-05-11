struct SceneUniforms {
    view_proj: mat4x4<f32>,
    affine_params: vec4<f32>,
    snap_params: vec4<f32>,
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
    @location(0) uv: vec2<f32>,
    @location(1) affine_uv: vec3<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    let clip = uniforms.view_proj * vec4(input.position, 1.0);

    let clip_w_sign = select(-1.0, 1.0, clip.w >= 0.0);
    let clip_w = clip_w_sign * max(abs(clip.w), 1e-6);
    let ndc = clip.xy / clip_w;
    let actual_res = max(uniforms.affine_params.yz, vec2(1.0, 1.0));
    let virtual_res = max(uniforms.snap_params.xy, vec2(1.0, 1.0));
    let screen = (ndc * 0.5 + vec2(0.5, 0.5)) * actual_res;
    let scale = actual_res / virtual_res;
    let snapped = floor(screen / scale + vec2(0.5, 0.5)) * scale;
    let snapped_ndc = (snapped / actual_res) * 2.0 - vec2(1.0, 1.0);
    out.position = vec4(snapped_ndc * clip_w, clip.z, clip.w);

    out.uv = input.uv;
    out.affine_uv = vec3(input.uv * clip.w, clip.w);
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let w_sign = select(-1.0, 1.0, input.affine_uv.z >= 0.0);
    let w = w_sign * max(abs(input.affine_uv.z), 1e-6);
    let affine = input.affine_uv.xy / w;
    let uv = mix(input.uv, affine, uniforms.affine_params.x);
    let color = textureSample(t_atlas, s_atlas, uv);
    if (color.a < 0.05) {
        discard;
    }
    return color;
}