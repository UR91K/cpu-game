use crate::renderer::mesh::{AtlasRect, push_quad, inset_atlas_rect_half_texel};
use crate::renderer::uniforms::SceneVertex;

/// A single triangle for collision — world-space, CCW winding (Y-up).
#[derive(Clone, Debug)]
pub struct MapTri {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub c: [f32; 3],
    /// Outward-facing surface normal (unit length).
    pub normal: [f32; 3],
}

impl MapTri {
    pub fn new(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Self {
        let ab = [b[0]-a[0], b[1]-a[1], b[2]-a[2]];
        let ac = [c[0]-a[0], c[1]-a[1], c[2]-a[2]];
        let nx = ab[1]*ac[2] - ab[2]*ac[1];
        let ny = ab[2]*ac[0] - ab[0]*ac[2];
        let nz = ab[0]*ac[1] - ab[1]*ac[0];
        let len = (nx*nx + ny*ny + nz*nz).sqrt().max(1e-10);
        Self { a, b, c, normal: [nx/len, ny/len, nz/len] }
    }
}

/// Combined render + collision representation of a map.
pub struct MapMesh {
    /// GPU vertex/index data — rebuilt when geometry changes.
    pub vertices: Vec<SceneVertex>,
    pub indices: Vec<u32>,
    /// Collision triangles — one per logical triangle.
    pub tris: Vec<MapTri>,
}

/// Builds the hardcoded test map.
///
/// Layout (X/Z plane, Y is up):
///   - Large floor plane 40×40
///   - Two solid boxes
///   - A ramp up to a raised platform
///
/// 1 unit ≈ 10cm (IVM scale). Human height ~18 units, doorway ~20.
pub fn build_test_map(atlas_rect: AtlasRect) -> MapMesh {
    let mut verts: Vec<SceneVertex> = Vec::new();
    let mut idxs: Vec<u32> = Vec::new();
    let mut tris: Vec<MapTri> = Vec::new();

    let rect = inset_atlas_rect_half_texel(atlas_rect);

    // Floor plane 40×40 centred at origin 
    let fw = 20.0_f32;
    emit_quad(&mut verts, &mut idxs, &mut tris, rect,
        [-fw, 0.0, -fw], [ fw, 0.0, -fw], [ fw, 0.0,  fw], [-fw, 0.0,  fw]);

    // Box A: 6w × 4h × 6d at (-8, 0, -8) 
    emit_box(&mut verts, &mut idxs, &mut tris, rect, -8.0, 0.0, -8.0, 6.0, 4.0, 6.0);

    // Box B: 4w × 3h × 4d at (6, 0, -6) 
    emit_box(&mut verts, &mut idxs, &mut tris, rect,  6.0, 0.0, -6.0, 4.0, 3.0, 4.0);

    // Ramp: rises y=0→3 from z=4→10, 5 units wide
    emit_quad(&mut verts, &mut idxs, &mut tris, rect,
        [-2.5, 0.0,  4.0], [ 2.5, 0.0,  4.0],
        [ 2.5, 3.0, 10.0], [-2.5, 3.0, 10.0]);
    // Side triangles
    emit_tri(&mut verts, &mut idxs, &mut tris, rect,
        [-2.5, 0.0, 4.0], [-2.5, 3.0, 10.0], [-2.5, 0.0, 10.0]);
    emit_tri(&mut verts, &mut idxs, &mut tris, rect,
        [ 2.5, 0.0, 4.0], [ 2.5, 0.0, 10.0], [ 2.5, 3.0, 10.0]);

    // Raised platform: 5w × 5d at y=3, z=10..15 
    emit_quad(&mut verts, &mut idxs, &mut tris, rect,
        [-2.5, 3.0, 10.0], [ 2.5, 3.0, 10.0],
        [ 2.5, 3.0, 15.0], [-2.5, 3.0, 15.0]);

    MapMesh { vertices: verts, indices: idxs, tris }
}

fn emit_quad(
    verts: &mut Vec<SceneVertex>, idxs: &mut Vec<u32>, tris: &mut Vec<MapTri>,
    rect: AtlasRect,
    p0: [f32;3], p1: [f32;3], p2: [f32;3], p3: [f32;3],
) {
    push_quad(verts, idxs, rect, false, p0, p1, p2, p3);
    tris.push(MapTri::new(p0, p1, p2));
    tris.push(MapTri::new(p0, p2, p3));
}

fn emit_tri(
    verts: &mut Vec<SceneVertex>, idxs: &mut Vec<u32>, tris: &mut Vec<MapTri>,
    rect: AtlasRect,
    p0: [f32;3], p1: [f32;3], p2: [f32;3],
) {
    let base = verts.len() as u32;
    // Planar UV projection onto XZ
    let uv = |p: [f32;3]| -> [f32;2] {
        let u = rect.u0 + (p[0] - p0[0]) * (rect.u1 - rect.u0);
        let v = rect.v0 + (p[2] - p0[2]) * (rect.v1 - rect.v0);
        [u, v]
    };
    verts.push(SceneVertex { position: p0, uv: uv(p0), color: SceneVertex::WHITE });
    verts.push(SceneVertex { position: p1, uv: uv(p1), color: SceneVertex::WHITE });
    verts.push(SceneVertex { position: p2, uv: uv(p2), color: SceneVertex::WHITE });
    idxs.extend_from_slice(&[base, base+1, base+2]);
    tris.push(MapTri::new(p0, p1, p2));
}

/// Emit 5 visible faces of a solid AABB box (no bottom face — sits on the floor).
fn emit_box(
    verts: &mut Vec<SceneVertex>, idxs: &mut Vec<u32>, tris: &mut Vec<MapTri>,
    rect: AtlasRect,
    x: f32, y: f32, z: f32, w: f32, h: f32, d: f32,
) {
    let (x0, x1) = (x, x + w);
    let (y0, y1) = (y, y + h);
    let (z0, z1) = (z, z + d);

    // Top
    emit_quad(verts, idxs, tris, rect, [x0,y1,z0],[x1,y1,z0],[x1,y1,z1],[x0,y1,z1]);
    // Front (+Z)
    emit_quad(verts, idxs, tris, rect, [x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1]);
    // Back (-Z)
    emit_quad(verts, idxs, tris, rect, [x1,y0,z0],[x0,y0,z0],[x0,y1,z0],[x1,y1,z0]);
    // Left (-X)
    emit_quad(verts, idxs, tris, rect, [x0,y0,z0],[x0,y0,z1],[x0,y1,z1],[x0,y1,z0]);
    // Right (+X)
    emit_quad(verts, idxs, tris, rect, [x1,y0,z1],[x1,y0,z0],[x1,y1,z0],[x1,y1,z1]);
}
