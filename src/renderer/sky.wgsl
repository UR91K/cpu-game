// Global constants
const cHashA4 = vec4<f32>(0.0, 1.0, 57.0, 58.0);
const cHashA3 = vec3<f32>(1.0, 57.0, 113.0);
const cHashM: f32 = 43758.54;

struct SkyUniforms {
    time_resolution: vec4<f32>,
    camera_origin: vec4<f32>,
    camera_forward: vec4<f32>,
    camera_right: vec4<f32>,
};

struct VertOut {
    @builtin(position) pos: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: SkyUniforms;

// Helper globals (scoped to the logic)
var<private> tCur: f32;
var<private> sunDir: vec3<f32>;
var<private> sunCol: vec3<f32>;

fn Hashv4f(p: f32) -> vec4<f32> {
    return fract(sin(p + cHashA4) * cHashM);
}

fn Noisefv2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    var f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    let t = Hashv4f(dot(i, cHashA3.xy));
    return mix(mix(t.x, t.y, f.x), mix(t.z, t.w, f.x), f.y);
}

fn Noisefv3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    var f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    let q = dot(i, cHashA3);
    let t1 = Hashv4f(q);
    let t2 = Hashv4f(q + cHashA3.z);
    return mix(
        mix(mix(t1.x, t1.y, f.x), mix(t1.z, t1.w, f.x), f.y),
        mix(mix(t2.x, t2.y, f.x), mix(t2.z, t2.w, f.x), f.y), 
        f.z
    );
}

fn Noisev3v2(p: vec2<f32>) -> vec3<f32> {
    let i = floor(p);
    let f = fract(p);
    let ff = f * f;
    let u = ff * (3.0 - 2.0 * f);
    let uu = 30.0 * ff * (ff - 2.0 * f + 1.0);
    let h = Hashv4f(dot(i, cHashA3.xy));
    
    let resX = h.x + (h.y - h.x) * u.x + (h.z - h.x) * u.y + (h.x - h.y - h.z + h.w) * u.x * u.y;
    let resYZ = uu * (vec2<f32>(h.y - h.x, h.z - h.x) + (h.x - h.y - h.z + h.w) * u.yx);
    
    return vec3<f32>(resX, resYZ.x, resYZ.y);
}

fn SkyBg(rd: vec3<f32>) -> vec3<f32> {
    const sbCol = vec3<f32>(0.15, 0.2, 0.65);
    return sbCol + 0.2 * sunCol * pow(1.0 - max(rd.y, 0.0), 5.0);
}

fn SkyCol(ro: vec3<f32>, rd: vec3<f32>) -> vec3<f32> {
    const skyHt: f32 = 200.0;
    var cloudFac: f32 = 0.0;
    var mutableRo = ro;

    if (rd.y > 0.0) {
        mutableRo.x += 0.5 * tCur;
        var p = 0.01 * (rd.xz * (skyHt - mutableRo.y) / rd.y + mutableRo.xz);
        var w: f32 = 0.65;
        var f: f32 = 0.0;
        for (var j = 0; j < 4; j++) {
            f += w * Noisefv2(p);
            w *= 0.5;
            p *= 2.3;
        }
        cloudFac = clamp(5.0 * (f - 0.5) * rd.y - 0.1, 0.0, 1.0);
    }

    let s = max(dot(rd, sunDir), 0.0);
    var col = SkyBg(rd) + sunCol * (0.35 * pow(s, 6.0) + 0.65 * min(pow(s, 256.0), 0.3));
    col = mix(col, vec3<f32>(0.85), cloudFac);
    return col;
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VertOut {
    var xy = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0),
    );

    var out: VertOut;
    out.pos = vec4(xy[i], 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(@builtin(position) fragCoord: vec4<f32>) -> @location(0) vec4<f32> {
    let iTime = uniforms.time_resolution.x;
    let iResolution = uniforms.time_resolution.yz;
    let pitch = uniforms.time_resolution.w;
    let ro = uniforms.camera_origin.xyz;
    let planeLen = uniforms.camera_origin.w;
    let aspect = iResolution.x / iResolution.y;
    let uv = vec2<f32>(
        (2.0 * fragCoord.x - iResolution.x) / iResolution.y,
        (iResolution.y - 2.0 * fragCoord.y) / iResolution.y,
    );

    // Initialize globals
    tCur = iTime * 2.0;
    sunDir = normalize(vec3<f32>(0.9, 1.0, 0.4));
    sunCol = vec3<f32>(1.0, 0.9, 0.8);

    let cp = cos(pitch);
    let sp = sin(pitch);
    let right = normalize(uniforms.camera_right.xyz);
    let forward = normalize(uniforms.camera_forward.xyz);
    let pitchedForward = normalize(forward * cp + vec3<f32>(0.0, sp, 0.0));
    let pitchedUp = normalize(vec3<f32>(0.0, cp, 0.0) - forward * sp);
    let verticalPlaneLen = planeLen / aspect;
    let rd = normalize(pitchedForward + uv.x * planeLen * right + uv.y * verticalPlaneLen * pitchedUp);

    var col = SkyCol(ro, rd);

    if (rd.y < 0.0) {
        col = vec3<f32>(0.1, 0.15, 0.2);
    }

    col = smoothstep(vec3<f32>(0.0), vec3<f32>(1.0), col);
    col = pow(col, vec3<f32>(0.4545));

    return vec4<f32>(col, 1.0);
}