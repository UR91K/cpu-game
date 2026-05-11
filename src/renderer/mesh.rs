use glam::Vec3;
use std::collections::HashMap;

use crate::model::Level;
use crate::render_assembly::{RenderBillboard, RenderCamera};
use crate::renderer::animation::select_sprite_uv_rect;
use crate::renderer::uniforms::SceneVertex;
use crate::texture::{TextureKey, TextureManager};

// TODO: read the per tile wall_height from the level data instead of hardcoding it here.
pub const WALL_HEIGHT: f32 = 1.0;

#[derive(Clone, Copy)]
pub struct AtlasRect {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub pixel_width: u32,
    pub pixel_height: u32,
}

impl SceneVertex {
    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SceneVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

pub fn build_static_mesh(
    level: &Level,
    atlas_rects: &[AtlasRect],
    texture_manager: &TextureManager,
) -> (Vec<SceneVertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let height = level.tiles.len();
    let width = level.tiles.first().map_or(0, Vec::len);

    for z in 0..height {
        for x in 0..width {
            if !level.is_wall(x, z) {
                let texture_index =
                    texture_manager.texture_index(TextureKey::Floor(level.floor_at(x, z)));
                let rect = inset_atlas_rect_half_texel(
                    atlas_rects[texture_index.min(atlas_rects.len().saturating_sub(1))],
                );
                let x0 = x as f32;
                let x1 = x0 + 1.0;
                let z0 = z as f32;
                let z1 = z0 + 1.0;

                push_quad(
                    &mut vertices,
                    &mut indices,
                    rect,
                    false,
                    [x0, 0.0, z1],
                    [x1, 0.0, z1],
                    [x1, 0.0, z0],
                    [x0, 0.0, z0],
                );
                continue;
            }

            let texture_key = texture_manager.wall_texture(level.tile_at(x, z));
            let texture_index = texture_manager.texture_index(texture_key);
            let rect = inset_atlas_rect_half_texel(
                atlas_rects[texture_index.min(atlas_rects.len().saturating_sub(1))],
            );
            let x0 = x as f32;
            let x1 = x0 + 1.0;
            let z0 = z as f32;
            let z1 = z0 + 1.0;

            let left_empty = x == 0 || !level.is_wall(x - 1, z);
            let right_empty = x + 1 >= width || !level.is_wall(x + 1, z);
            let north_empty = z == 0 || !level.is_wall(x, z - 1);
            let south_empty = z + 1 >= height || !level.is_wall(x, z + 1);

            if left_empty {
                push_quad(
                    &mut vertices,
                    &mut indices,
                    rect,
                    false,
                    [x0, 0.0, z1],
                    [x0, 0.0, z0],
                    [x0, WALL_HEIGHT, z0],
                    [x0, WALL_HEIGHT, z1],
                );
            }
            if right_empty {
                push_quad(
                    &mut vertices,
                    &mut indices,
                    rect,
                    false,
                    [x1, 0.0, z0],
                    [x1, 0.0, z1],
                    [x1, WALL_HEIGHT, z1],
                    [x1, WALL_HEIGHT, z0],
                );
            }
            if north_empty {
                push_quad(
                    &mut vertices,
                    &mut indices,
                    rect,
                    true,
                    [x1, 0.0, z0],
                    [x0, 0.0, z0],
                    [x0, WALL_HEIGHT, z0],
                    [x1, WALL_HEIGHT, z0],
                );
            }
            if south_empty {
                push_quad(
                    &mut vertices,
                    &mut indices,
                    rect,
                    true,
                    [x0, 0.0, z1],
                    [x1, 0.0, z1],
                    [x1, WALL_HEIGHT, z1],
                    [x0, WALL_HEIGHT, z1],
                );
            }
        }
    }

    (vertices, indices)
}

pub fn build_sprite_vertices(
    camera: &RenderCamera,
    billboards: &[RenderBillboard],
    atlas_rects: &[AtlasRect],
    atlas_index_by_texture: &HashMap<TextureKey, usize>,
    anim_elapsed_ms: f64,
) -> Vec<SceneVertex> {
    let mut sorted = billboards.to_vec();
    sorted.sort_by(|left, right| {
        let left_dist = (left.x - camera.x).powi(2) + (left.y - camera.y).powi(2);
        let right_dist = (right.x - camera.x).powi(2) + (right.y - camera.y).powi(2);
        right_dist.total_cmp(&left_dist)
    });

    let right = Vec3::new(camera.plane_x as f32, 0.0, camera.plane_y as f32).normalize_or_zero();
    let mut vertices = Vec::with_capacity(sorted.len() * 6);
    let camera_dir = (camera.dir_x, camera.dir_y);

    for billboard in sorted {
        let Some(texture_index) = atlas_index_by_texture.get(&billboard.texture).copied() else {
            continue;
        };
        let Some(rect) = atlas_rects.get(texture_index) else {
            continue;
        };
        let uv_rect = select_sprite_uv_rect(*rect, &billboard, camera_dir, anim_elapsed_ms);
        let center = Vec3::new(billboard.x as f32, 0.0, billboard.y as f32);
        let half_width = right * (billboard.width * 0.5);
        let up = Vec3::new(0.0, billboard.height, 0.0);
        let bottom_left = center - half_width;
        let bottom_right = center + half_width;
        let top_left = bottom_left + up;
        let top_right = bottom_right + up;

        vertices.extend_from_slice(&[
            SceneVertex {
                position: bottom_left.to_array(),
                uv: [uv_rect.u0, uv_rect.v1],
            },
            SceneVertex {
                position: bottom_right.to_array(),
                uv: [uv_rect.u1, uv_rect.v1],
            },
            SceneVertex {
                position: top_right.to_array(),
                uv: [uv_rect.u1, uv_rect.v0],
            },
            SceneVertex {
                position: bottom_left.to_array(),
                uv: [uv_rect.u0, uv_rect.v1],
            },
            SceneVertex {
                position: top_right.to_array(),
                uv: [uv_rect.u1, uv_rect.v0],
            },
            SceneVertex {
                position: top_left.to_array(),
                uv: [uv_rect.u0, uv_rect.v0],
            },
        ]);
    }

    vertices
}

pub fn push_quad(
    vertices: &mut Vec<SceneVertex>,
    indices: &mut Vec<u32>,
    rect: AtlasRect,
    flip_u: bool,
    p0: [f32; 3],
    p1: [f32; 3],
    p2: [f32; 3],
    p3: [f32; 3],
) {
    // Subdivide each quad into a 2x2 grid (4 sub-quads = 8 triangles) via bilinear interpolation.
    // Corner parameterization: p0=(s=0,t=0), p1=(s=1,t=0), p2=(s=1,t=1), p3=(s=0,t=1)
    const DIVS: usize = 2;

    let (left_u, right_u) = if flip_u {
        (rect.u1, rect.u0)
    } else {
        (rect.u0, rect.u1)
    };

    let lerp3 = |a: [f32; 3], b: [f32; 3], t: f32| -> [f32; 3] {
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
        ]
    };

    let point_at = |s: f32, t: f32| -> SceneVertex {
        let bottom = lerp3(p0, p1, s);
        let top = lerp3(p3, p2, s);
        let pos = lerp3(bottom, top, t);
        let u = left_u + (right_u - left_u) * s;
        let v = rect.v1 + (rect.v0 - rect.v1) * t;
        SceneVertex {
            position: pos,
            uv: [u, v],
        }
    };

    for ti in 0..DIVS {
        for si in 0..DIVS {
            let s0 = si as f32 / DIVS as f32;
            let s1 = (si + 1) as f32 / DIVS as f32;
            let t0 = ti as f32 / DIVS as f32;
            let t1 = (ti + 1) as f32 / DIVS as f32;
            let base = vertices.len() as u32;
            vertices.extend_from_slice(&[
                point_at(s0, t0),
                point_at(s1, t0),
                point_at(s1, t1),
                point_at(s0, t1),
            ]);
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
}

pub fn inset_atlas_rect_half_texel(rect: AtlasRect) -> AtlasRect {
    let width_scale = (rect.u1 - rect.u0) / rect.pixel_width.max(1) as f32;
    let height_scale = (rect.v1 - rect.v0) / rect.pixel_height.max(1) as f32;
    let half_texel_u = width_scale * 0.5;
    let half_texel_v = height_scale * 0.5;

    AtlasRect {
        u0: rect.u0 + half_texel_u,
        v0: rect.v0 + half_texel_v,
        u1: rect.u1 - half_texel_u,
        v1: rect.v1 - half_texel_v,
        pixel_width: rect.pixel_width,
        pixel_height: rect.pixel_height,
    }
}

pub fn create_sprite_buffer(device: &wgpu::Device, vertex_capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cpu_game_sprite_vertices"),
        size: (vertex_capacity * std::mem::size_of::<SceneVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
