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

const colTop = vec3<f32>(0.12, 0.20, 0.90);
const colBottom = vec3<f32>(0.75, 0.85, 0.95);
const cloudCol = vec3<f32>(1.0, 1.0, 1.0);

const skyHt: f32 = 200.0;       // cloud plane height
const cloudFadeDist: f32 = 1500.0; // distance at which clouds fully dissolve into dome

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

fn SkyCol(rd: vec3<f32>) -> vec3<f32> {
    let t = clamp(rd.y * 0.5 + 0.5, 0.0, 1.0);
    return mix(colBottom, colTop, t);
}

fn Sky(ro: vec3<f32>, rd: vec3<f32>, tCur: f32) -> vec3<f32> {
    var col = SkyCol(rd);

    if (rd.y > 0.001) {
        // distance along ray to the cloud plane
        let dist = (skyHt - ro.y) / rd.y;
        let hit = ro + rd * dist;

        // animate + sample noise on the plane
        var p = 0.01 * (hit.xz + vec2<f32>(0.5 * tCur, 0.0));
        var w: f32 = 0.65;
        var f: f32 = 0.0;
        for (var j = 0; j < 4; j++) {
            f += w * Noisefv2(p);
            w *= 0.5;
            p *= 2.0;
        }

        var cloudFac = clamp(8.0 * (0.4 - f), 0.0, 1.0);

        // fade clouds out as the hit point gets far away
        let distFade = 1.0 - clamp(dist / cloudFadeDist, 0.0, 1.0);
        cloudFac *= distFade;

        col = mix(col, cloudCol, cloudFac);
    }

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
    let ro = uniforms.camera_origin.xyz;
    let planeLen = uniforms.camera_origin.w;
    let aspect = iResolution.x / iResolution.y;
    let uv = vec2<f32>(
        (2.0 * fragCoord.x - iResolution.x) / iResolution.x,
        (iResolution.y - 2.0 * fragCoord.y) / iResolution.y,
    );

    let right = normalize(uniforms.camera_right.xyz);
    let forward = normalize(uniforms.camera_forward.xyz);
    let up = normalize(cross(forward, right));
    let verticalPlaneLen = planeLen / aspect;
    let rd = normalize(
        forward
        + uv.x * planeLen * right
        + uv.y * verticalPlaneLen * up
    );

    let tCur = iTime * 2.0;
    let col = Sky(ro, rd, tCur);
    return vec4<f32>(col, 1.0);
}