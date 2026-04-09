@group(0) @binding(0) var t_composite: texture_2d<f32>;
@group(0) @binding(1) var s_composite: sampler;

struct VertOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
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
fn fs_main(in: VertOut) -> @location(0) vec4<f32> {
    return textureSample(t_composite, s_composite, in.uv);
}
