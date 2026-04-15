const TAPS: i32 = 32;
const LUMA_FILTER: array<f32, 33> = array<f32, 33>(
    -0.000174844, -0.000205844, -0.000149453, -0.000051693, 0.000000000,
    -0.000066171, -0.000245058, -0.000432928, -0.000472644, -0.000252236,
    0.000198929, 0.000687058, 0.000944112, 0.000803467, 0.000363199,
    0.000013422, 0.000253402, 0.001339461, 0.002932972, 0.003983485,
    0.003026683, -0.001102056, -0.008373026, -0.016897700, -0.022914480,
    -0.021642347, -0.008863273, 0.017271957, 0.054921920, 0.098342579,
    0.139044281, 0.168055832, 0.178571429
);
const CHROMA_FILTER: array<f32, 33> = array<f32, 33>(
    0.001384762, 0.001678312, 0.002021715, 0.002420562, 0.002880460,
    0.003406879, 0.004004985, 0.004679445, 0.005434218, 0.006272332,
    0.007195654, 0.008204665, 0.009298238, 0.010473450, 0.011725413,
    0.013047155, 0.014429548, 0.015861306, 0.017329037, 0.018817382,
    0.020309220, 0.021785952, 0.023227857, 0.024614500, 0.025925203,
    0.027139546, 0.028237893, 0.029201910, 0.030015081, 0.030663170,
    0.031134640, 0.031420995, 0.031517031
);

struct DecodeUniforms {
    source_size: vec2<f32>,
    gamma_exp: f32,
    _pad0: f32,
};

@group(0) @binding(0) var t_source: texture_2d<f32>;
@group(0) @binding(1) var s_source: sampler;
@group(0) @binding(2) var<uniform> uniforms: DecodeUniforms;

struct VertOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

fn srgb_channel_to_linear(value: f32) -> f32 {
    if (value <= 0.04045) {
        return value / 12.92;
    }
    return pow((value + 0.055) / 1.055, 2.4);
}

fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
    let clamped = max(color, vec3<f32>(0.0));
    return vec3<f32>(
        srgb_channel_to_linear(clamped.r),
        srgb_channel_to_linear(clamped.g),
        srgb_channel_to_linear(clamped.b),
    );
}

fn yiq2rgb(yiq: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        yiq.x + 0.9560 * yiq.y + 0.6210 * yiq.z,
        yiq.x - 0.2720 * yiq.y - 0.6474 * yiq.z,
        yiq.x - 1.1060 * yiq.y + 1.7046 * yiq.z,
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
    out.uv = uv[i] - vec2(0.5 / uniforms.source_size.x, 0.0);
    return out;
}

@fragment
fn fs_main(input: VertOut) -> @location(0) vec4<f32> {
    let one_x = 1.0 / uniforms.source_size.x;
    var signal = vec3<f32>(0.0);

    for (var i: i32 = 0; i < TAPS; i = i + 1) {
        let offset = f32(i);
        let sums =
            textureSample(t_source, s_source, input.uv + vec2((offset - f32(TAPS)) * one_x, 0.0)).xyz +
            textureSample(t_source, s_source, input.uv + vec2((f32(TAPS) - offset) * one_x, 0.0)).xyz;
        signal += sums * vec3<f32>(LUMA_FILTER[i], CHROMA_FILTER[i], CHROMA_FILTER[i]);
    }

    signal += textureSample(t_source, s_source, input.uv).xyz *
        vec3<f32>(LUMA_FILTER[TAPS], CHROMA_FILTER[TAPS], CHROMA_FILTER[TAPS]);

    let rgb = max(yiq2rgb(signal), vec3<f32>(0.0));
    let display_rgb = pow(rgb, vec3<f32>(uniforms.gamma_exp));
    return vec4<f32>(srgb_to_linear(display_rgb), 1.0);
}