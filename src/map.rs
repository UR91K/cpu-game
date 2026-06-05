use crate::model::MapTri;
use crate::renderer::mesh::{AtlasRect, push_quad, inset_atlas_rect_half_texel};
use crate::renderer::uniforms::SceneVertex;

/// Combined render + collision representation of a map.
pub struct MapMesh {
    /// GPU vertex/index data — rebuilt when geometry changes.
    pub vertices: Vec<SceneVertex>,
    pub indices: Vec<u32>,
    /// Collision triangles — one per logical triangle.
    pub tris: Vec<MapTri>,
}

/// Returns collision triangles for the test map geometry (no GPU data needed).
pub fn build_test_map_tris() -> Vec<MapTri> {
    let mut tris: Vec<MapTri> = Vec::new();
    emit_test_map_tris(&mut tris);
    tris
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

    emit_test_map_mesh(&mut verts, &mut idxs, &mut tris, rect);

    MapMesh { vertices: verts, indices: idxs, tris }
}

fn emit_test_map_mesh(
    verts: &mut Vec<SceneVertex>,
    idxs: &mut Vec<u32>,
    tris: &mut Vec<MapTri>,
    rect: AtlasRect,
) {

    // Floor plane 40×40 centred at origin
    let fw = 20.0_f32;
    emit_top_quad(verts, idxs, tris, rect,
        [-fw, 0.0, -fw], [ fw, 0.0, -fw], [ fw, 0.0,  fw], [-fw, 0.0,  fw]);

    // Box A: 6w × 4h × 6d at (-8, 0, -8)
    emit_box(verts, idxs, tris, rect, -8.0, 0.0, -8.0, 6.0, 4.0, 6.0);

    // Box B: 4w × 3h × 4d at (6, 0, -6)
    emit_box(verts, idxs, tris, rect,  6.0, 0.0, -6.0, 4.0, 3.0, 4.0);

    // Ramp: rises y=0→3 from z=4→10, 5 units wide
    emit_top_quad(verts, idxs, tris, rect,
        [-2.5, 0.0,  4.0], [ 2.5, 0.0,  4.0],
        [ 2.5, 3.0, 10.0], [-2.5, 3.0, 10.0]);
    // Side triangles
    emit_tri(verts, idxs, tris, rect,
        [-2.5, 0.0, 4.0], [-2.5, 0.0, 10.0], [-2.5, 3.0, 10.0]);
    emit_tri(verts, idxs, tris, rect,
        [ 2.5, 0.0, 4.0], [ 2.5, 3.0, 10.0], [ 2.5, 0.0, 10.0]);

    // Raised platform: 5w × 5d at y=3, z=10..15
    emit_top_quad(verts, idxs, tris, rect,
        [-2.5, 3.0, 10.0], [ 2.5, 3.0, 10.0],
        [ 2.5, 3.0, 15.0], [-2.5, 3.0, 15.0]);
}

fn emit_test_map_tris(tris: &mut Vec<MapTri>) {
    let fw = 20.0_f32;
    push_tris_top(tris, [-fw,0.0,-fw],[fw,0.0,-fw],[fw,0.0,fw],[-fw,0.0,fw]);
    push_tris_box(tris, -8.0, 0.0, -8.0, 6.0, 4.0, 6.0);
    push_tris_box(tris,  6.0, 0.0, -6.0, 4.0, 3.0, 4.0);
    push_tris_top(tris,
        [-2.5,0.0,4.0],[2.5,0.0,4.0],[2.5,3.0,10.0],[-2.5,3.0,10.0]);
    tris.push(MapTri::new([-2.5,0.0,4.0],[-2.5,0.0,10.0],[-2.5,3.0,10.0]));
    tris.push(MapTri::new([ 2.5,0.0,4.0],[ 2.5,3.0,10.0],[ 2.5,0.0,10.0]));
    push_tris_top(tris,
        [-2.5,3.0,10.0],[2.5,3.0,10.0],[2.5,3.0,15.0],[-2.5,3.0,15.0]);
}

fn emit_top_quad(
    verts: &mut Vec<SceneVertex>,
    idxs: &mut Vec<u32>,
    tris: &mut Vec<MapTri>,
    rect: AtlasRect,
    p0: [f32;3],
    p1: [f32;3],
    p2: [f32;3],
    p3: [f32;3],
) {
    push_quad(verts, idxs, rect, false, p0, p3, p2, p1);
    push_tris_top(tris, p0, p1, p2, p3);
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
    emit_top_quad(verts, idxs, tris, rect, [x0,y1,z0],[x1,y1,z0],[x1,y1,z1],[x0,y1,z1]);
    // Front (+Z)
    emit_quad(verts, idxs, tris, rect, [x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1]);
    // Back (-Z)
    emit_quad(verts, idxs, tris, rect, [x1,y0,z0],[x0,y0,z0],[x0,y1,z0],[x1,y1,z0]);
    // Left (-X)
    emit_quad(verts, idxs, tris, rect, [x0,y0,z0],[x0,y0,z1],[x0,y1,z1],[x0,y1,z0]);
    // Right (+X)
    emit_quad(verts, idxs, tris, rect, [x1,y0,z1],[x1,y0,z0],[x1,y1,z0],[x1,y1,z1]);
}

/// Vertical-face quad: winding new(p0,p1,p2) / new(p0,p2,p3) gives correct sideways normals.
fn push_tris_wall(tris: &mut Vec<MapTri>, p0: [f32;3], p1: [f32;3], p2: [f32;3], p3: [f32;3]) {
    tris.push(MapTri::new(p0, p1, p2));
    tris.push(MapTri::new(p0, p2, p3));
}

/// Upward-facing quad: reversed winding new(p0,p3,p1) / new(p1,p3,p2) gives +Y normal.
fn push_tris_top(tris: &mut Vec<MapTri>, p0: [f32;3], p1: [f32;3], p2: [f32;3], p3: [f32;3]) {
    tris.push(MapTri::new(p0, p3, p1));
    tris.push(MapTri::new(p1, p3, p2));
}

fn push_tris_box(tris: &mut Vec<MapTri>, x: f32, y: f32, z: f32, w: f32, h: f32, d: f32) {
    let (x0, x1) = (x, x + w);
    let (y0, y1) = (y, y + h);
    let (z0, z1) = (z, z + d);
    push_tris_top( tris, [x0,y1,z0],[x1,y1,z0],[x1,y1,z1],[x0,y1,z1]); // Top (+Y)
    push_tris_wall(tris, [x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1]); // Front (+Z)
    push_tris_wall(tris, [x1,y0,z0],[x0,y0,z0],[x0,y1,z0],[x1,y1,z0]); // Back (-Z)
    push_tris_wall(tris, [x0,y0,z0],[x0,y0,z1],[x0,y1,z1],[x0,y1,z0]); // Left (-X)
    push_tris_wall(tris, [x1,y0,z1],[x1,y0,z0],[x1,y1,z0],[x1,y1,z1]); // Right (+X)
}

#[cfg(test)]
mod tests {
    use super::{build_test_map, build_test_map_tris};
    use crate::renderer::mesh::AtlasRect;

    #[test]
    fn test_map_collision_matches_render_triangles() {
        let atlas_rect = AtlasRect {
            u0: 0.0,
            v0: 0.0,
            u1: 1.0,
            v1: 1.0,
            pixel_width: 1,
            pixel_height: 1,
        };

        let mesh_tris = build_test_map(atlas_rect).tris;
        let collision_tris = build_test_map_tris();

        assert_eq!(mesh_tris.len(), collision_tris.len());
        for (mesh, collision) in mesh_tris.iter().zip(collision_tris.iter()) {
            assert_eq!(mesh.a, collision.a);
            assert_eq!(mesh.b, collision.b);
            assert_eq!(mesh.c, collision.c);
            assert_eq!(mesh.normal, collision.normal);
        }
    }
}
