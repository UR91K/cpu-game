use crate::render_assembly::RenderBillboard;
use crate::renderer::mesh::AtlasRect;
use crate::texture::{
    AnimationDescriptor, AnimationPlayback, animation_descriptor,
};

pub const WALK_PING_PONG: [u32; 4] = [0, 1, 2, 1];

#[derive(Clone, Copy)]
enum VisibleSide {
    Front,
    Back,
    Left,
    Right,
}

pub fn select_sprite_uv_rect(
    rect: AtlasRect,
    billboard: &RenderBillboard,
    camera_dir: (f64, f64),
    anim_elapsed_ms: f64,
) -> AtlasRect {
    let Some(animation) = animation_descriptor(billboard.animation) else {
        return rect;
    };
    let atlas_width = animation.columns * animation.frame_width;
    let atlas_height = animation.rows * animation.frame_height;
    if (rect.pixel_width as usize) < atlas_width || (rect.pixel_height as usize) < atlas_height {
        return rect;
    }

    let frame_step = (anim_elapsed_ms / animation.ms_per_frame).floor() as u32;
    let frame_col = animation_frame_column(animation, frame_step, billboard.is_moving);
    let side_row = if animation.directional_rows
        && matches!(billboard.facing_mode, crate::texture::FacingMode::Movement)
    {
        side_to_row(get_visible_side(billboard.facing_dir, camera_dir)) as usize
    } else {
        animation_frame_row(animation, frame_step)
    };

    let frame_origin_x = frame_col * animation.frame_width;
    let frame_origin_y = side_row * animation.frame_height;
    let width_scale = (rect.u1 - rect.u0) / rect.pixel_width as f32;
    let height_scale = (rect.v1 - rect.v0) / rect.pixel_height as f32;
    let half_texel_u = width_scale * 0.5;
    let half_texel_v = height_scale * 0.5;

    AtlasRect {
        u0: rect.u0 + frame_origin_x as f32 * width_scale + half_texel_u,
        v0: rect.v0 + frame_origin_y as f32 * height_scale + half_texel_v,
        u1: rect.u0 + (frame_origin_x + animation.frame_width) as f32 * width_scale - half_texel_u,
        v1: rect.v0 + (frame_origin_y + animation.frame_height) as f32 * height_scale
            - half_texel_v,
        pixel_width: animation.frame_width as u32,
        pixel_height: animation.frame_height as u32,
    }
}

fn animation_frame_column(
    animation: AnimationDescriptor,
    frame_step: u32,
    is_moving: bool,
) -> usize {
    match animation.playback {
        AnimationPlayback::PingPong => walk_frame_col(frame_step, is_moving) as usize,
        AnimationPlayback::Loop => (frame_step as usize) % animation.columns.max(1),
    }
}

fn animation_frame_row(animation: AnimationDescriptor, frame_step: u32) -> usize {
    if animation.directional_rows {
        0
    } else {
        match animation.playback {
            AnimationPlayback::PingPong => 0,
            AnimationPlayback::Loop => (frame_step as usize) % animation.rows.max(1),
        }
    }
}

fn get_visible_side(entity_dir: (f64, f64), camera_dir: (f64, f64)) -> VisibleSide {
    // Use dot/cross products to avoid atan2.
    // dot = |e||c|cos(rel), cross z = e×c = -|e||c|sin(rel), where rel = entity_angle - camera_angle.
    let dot = entity_dir.0 * camera_dir.0 + entity_dir.1 * camera_dir.1;
    let cross = entity_dir.0 * camera_dir.1 - entity_dir.1 * camera_dir.0;
    let len_sq = (entity_dir.0 * entity_dir.0 + entity_dir.1 * entity_dir.1)
        * (camera_dir.0 * camera_dir.0 + camera_dir.1 * camera_dir.1);
    if len_sq < 1e-20 {
        return VisibleSide::Front;
    }
    // |cos(rel)| >= cos(45°)=1/√2  ↔  dot² >= 0.5*len_sq
    if dot * dot >= 0.5 * len_sq {
        if dot >= 0.0 {
            VisibleSide::Back
        } else {
            VisibleSide::Front
        }
    } else if cross < 0.0 {
        VisibleSide::Left
    } else {
        VisibleSide::Right
    }
}

fn side_to_row(side: VisibleSide) -> u32 {
    match side {
        VisibleSide::Front => 0,
        VisibleSide::Back => 1,
        VisibleSide::Left => 2,
        VisibleSide::Right => 3,
    }
}

fn walk_frame_col(frame_step: u32, is_moving: bool) -> u32 {
    if !is_moving {
        1
    } else {
        WALK_PING_PONG[(frame_step % WALK_PING_PONG.len() as u32) as usize]
    }
}
