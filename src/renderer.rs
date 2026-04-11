use palette::Srgb;

use crate::model::{AoField, Map, Sprite};
use crate::simulation::PlayerState;

pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 480;
pub const TEXTURE_SIZE: usize = 64;
const BASE_ASPECT: f64 = WIDTH as f64 / HEIGHT as f64;

const FLOOR_COLOR: Srgb = Srgb::new(66.0, 119.0, 41.0);
const CEILING_COLOR: Srgb = Srgb::new(20.0, 20.0, 20.0);

const ANIM_COLS: usize = 3;
const ANIM_ROWS: usize = 4;
const FRAME_W: usize = TEXTURE_SIZE;
const FRAME_H: usize = TEXTURE_SIZE;
const ANIM_FRAME_COUNT: u32 = 3;
const ANIM_MS_PER_FRAME: f64 = 90.0;
const WALK_PING_PONG: [u32; 4] = [0, 1, 2, 1];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VisibleSide {
    Front,
    Back,
    Left,
    Right,
}

pub fn get_visible_side(entity_movement_angle: f64, camera_facing_angle: f64) -> VisibleSide {
    const SIDE_HALF_ANGLE: f64 = 0.785398;

    let mut rel = entity_movement_angle - camera_facing_angle;

    rel = (rel + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;

    let abs_rel = rel.abs();

    if abs_rel < SIDE_HALF_ANGLE {
        VisibleSide::Back
    } else if abs_rel > std::f64::consts::PI - SIDE_HALF_ANGLE {
        VisibleSide::Front
    } else if abs_rel > std::f64::consts::FRAC_PI_2 - SIDE_HALF_ANGLE
           && abs_rel < std::f64::consts::FRAC_PI_2 + SIDE_HALF_ANGLE
    {
        if rel > 0.0 { VisibleSide::Left } else { VisibleSide::Right }
    } else if abs_rel < std::f64::consts::FRAC_PI_2 {
        VisibleSide::Back
    } else {
        VisibleSide::Front
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
        ANIM_FRAME_COUNT / 2
    } else {
        WALK_PING_PONG[(frame_step % WALK_PING_PONG.len() as u32) as usize]
    }
}

fn srgb_to_u32(color: Srgb) -> u32 {
    let r = color.red as u32;
    let g = color.green as u32;
    let b = color.blue as u32;
    (r << 16) | (g << 8) | b
}

fn lerp_u8(a: u8, b: u8, t: f64) -> u8 {
    (a as f64 + (b as f64 - a as f64) * t.clamp(0.0, 1.0)).round() as u8
}

fn modulate_rgb_u8(r: u8, g: u8, b: u8, light: u8) -> u32 {
    let l = light as u32;
    let rr = (r as u32 * l) / 255;
    let gg = (g as u32 * l) / 255;
    let bb = (b as u32 * l) / 255;
    (rr << 16) | (gg << 8) | bb
}

fn tile_corners(ao: &AoField, x: usize, y: usize) -> [u8; 4] {
    if x >= ao.width || y >= ao.height {
        return [255, 255, 255, 255];
    }
    ao.corners[y * ao.width + x]
}

fn wall_ao_light(ao: &AoField, x: usize, y: usize, side: i32, ray_dir_x: f64, ray_dir_y: f64, wall_u: f64) -> u8 {
    // Sample the adjacent *floor* tile's corners rather than the wall tile itself,
    // picking the edge of that floor tile that abuts the wall.
    let t = wall_u.clamp(0.0, 1.0);

    if side == 0 {
        if ray_dir_x > 0.0 {
            // Hit west face of wall; floor tile is one step left (x-1, y).
            let [_tl, tr, br, _bl] = tile_corners(ao, x.wrapping_sub(1), y);
            lerp_u8(tr, br, t) // right edge of that floor tile
        } else {
            // Hit east face of wall; floor tile is one step right (x+1, y).
            let [tl, _tr, _br, bl] = tile_corners(ao, x + 1, y);
            lerp_u8(tl, bl, t) // left edge of that floor tile
        }
    } else {
        if ray_dir_y > 0.0 {
            // Hit north face of wall; floor tile is one step up (x, y-1).
            let [_tl, _tr, br, bl] = tile_corners(ao, x, y.wrapping_sub(1));
            lerp_u8(bl, br, t) // bottom edge of that floor tile
        } else {
            // Hit south face of wall; floor tile is one step down (x, y+1).
            let [tl, tr, _br, _bl] = tile_corners(ao, x, y + 1);
            lerp_u8(tl, tr, t) // top edge of that floor tile
        }
    }
}

pub fn render(
    buffer: &mut [u32],
    render_width: usize,
    render_height: usize,
    player: &PlayerState,
    sprites: &[Sprite],
    map: &Map,
    ao: &AoField,
    textures: &[image::RgbaImage],
    pitch: i32,
    anim_elapsed_ms: f64,
) {
    assert!(
        render_width > 0 && render_height > 0,
        "render resolution must be non-zero"
    );
    assert!(
        buffer.len() >= render_width * render_height,
        "render buffer too small for resolution"
    );

    let mut z_buffer = vec![0.0f64; render_width];

    let floor_color = srgb_to_u32(FLOOR_COLOR);
    let ceiling_color = srgb_to_u32(CEILING_COLOR);
    let pitch_px = ((pitch as f64) * (render_height as f64 / HEIGHT as f64)).round() as i32;
    let horizon = (render_height as i32 / 2 + pitch_px).clamp(0, render_height as i32) as usize;

    let render_aspect = render_width as f64 / render_height as f64;
    let plane_aspect_scale = render_aspect / BASE_ASPECT;
    let proj_plane_x = player.plane_x * plane_aspect_scale;
    let proj_plane_y = player.plane_y * plane_aspect_scale;

    // floor + ceiling
    for y in 0..render_height {
        for x in 0..render_width {
            if y < horizon {
                buffer[y * render_width + x] = ceiling_color;
            } else {
                buffer[y * render_width + x] = floor_color;
            }
        }
    }

    // walls
    for x in 0..render_width {
        let camera_x: f64 = 2.0 * x as f64 / render_width as f64 - 1.0;
        let ray_dir_x: f64 = player.dir_x + proj_plane_x * camera_x;
        let ray_dir_y: f64 = player.dir_y + proj_plane_y * camera_x;

        let mut map_x = player.x as i32;
        let mut map_y = player.y as i32;

        let delta_dist_x: f64 = if ray_dir_x == 0.0 {
            1e30
        } else {
            (1.0 / ray_dir_x).abs()
        };
        let delta_dist_y: f64 = if ray_dir_y == 0.0 {
            1e30
        } else {
            (1.0 / ray_dir_y).abs()
        };

        let step_x: i32;
        let step_y: i32;
        let mut side_dist_x: f64;
        let mut side_dist_y: f64;

        if ray_dir_x < 0.0 {
            step_x = -1;
            side_dist_x = (player.x - map_x as f64) * delta_dist_x;
        } else {
            step_x = 1;
            side_dist_x = (map_x as f64 + 1.0 - player.x) * delta_dist_x;
        }
        if ray_dir_y < 0.0 {
            step_y = -1;
            side_dist_y = (player.y - map_y as f64) * delta_dist_y;
        } else {
            step_y = 1;
            side_dist_y = (map_y as f64 + 1.0 - player.y) * delta_dist_y;
        }

        let mut side;
        loop {
            if side_dist_x < side_dist_y {
                side_dist_x += delta_dist_x;
                map_x += step_x;
                side = 0;
            } else {
                side_dist_y += delta_dist_y;
                map_y += step_y;
                side = 1;
            }
            if map.is_wall(map_x as usize, map_y as usize) {
                break;
            }
        }

        let perp_wall_dist = if side == 0 {
            (map_x as f64 - player.x + (1.0 - step_x as f64) / 2.0) / ray_dir_x
        } else {
            (map_y as f64 - player.y + (1.0 - step_y as f64) / 2.0) / ray_dir_y
        };

        z_buffer[x] = perp_wall_dist;

        let line_height = (render_height as f64 / perp_wall_dist) as i32;
        let draw_start =
            ((-line_height / 2 + render_height as i32 / 2 + pitch_px).max(0)) as usize;
        let draw_end = ((line_height / 2 + render_height as i32 / 2 + pitch_px)
            .min(render_height as i32 - 1)) as usize;

        let texture_index = (map.tile_at(map_x as usize, map_y as usize) - 1) as usize;

        let wall_x = if side == 0 {
            player.y + perp_wall_dist * ray_dir_y
        } else {
            player.x + perp_wall_dist * ray_dir_x
        };
        let wall_x = wall_x - wall_x.floor();
        let ao_light = wall_ao_light(
            ao,
            map_x as usize,
            map_y as usize,
            side,
            ray_dir_x,
            ray_dir_y,
            wall_x,
        );

        let tex_x = {
            let raw = (wall_x * TEXTURE_SIZE as f64) as usize;
            let raw = raw.min(TEXTURE_SIZE - 1);
            if (side == 0 && ray_dir_x > 0.0) || (side == 1 && ray_dir_y < 0.0) {
                TEXTURE_SIZE - raw - 1
            } else {
                raw
            }
        };

        let step = TEXTURE_SIZE as f64 / line_height as f64;
        let texture = &textures[texture_index];
        let texture_position = (draw_start as f64 - pitch_px as f64 - render_height as f64 / 2.0
            + line_height as f64 / 2.0)
            * step;
        for y in draw_start..draw_end {
            let tex_y =
                ((texture_position + (y - draw_start) as f64 * step) as usize) % TEXTURE_SIZE;
            let color = texture.get_pixel(tex_x as u32, tex_y as u32);
            buffer[y * render_width + x] = modulate_rgb_u8(color[0], color[1], color[2], ao_light);
        }
    }

    // draw sprites
    let camera_facing_angle = player.dir_y.atan2(player.dir_x);
    let atlas_width = ANIM_COLS * FRAME_W;
    let atlas_height = ANIM_ROWS * FRAME_H;

    for sprite in sprites {
        let sprite_x = sprite.x - player.x;
        let sprite_y = sprite.y - player.y;

        let inv_det = 1.0 / (proj_plane_x * player.dir_y - player.dir_x * proj_plane_y);
        let transform_x = inv_det * (player.dir_y * sprite_x - player.dir_x * sprite_y);
        let transform_y =
            inv_det * (-proj_plane_y * sprite_x + proj_plane_x * sprite_y);

        if transform_y <= 0.0 {
            continue;
        }

        let sprite_screen_x =
            ((render_width as f64 / 2.0) * (1.0 + transform_x / transform_y)) as i32;

        let sprite_height = (render_height as f64 / transform_y).abs() as i32;
        let draw_start_y =
            ((-sprite_height / 2 + render_height as i32 / 2 + pitch_px).max(0)) as usize;
        let draw_end_y =
            ((sprite_height / 2 + render_height as i32 / 2 + pitch_px).min(render_height as i32 - 1)) as usize;

        let sprite_width = sprite_height;
        let draw_start_x = ((-(i64::from(sprite_width)) / 2 + i64::from(sprite_screen_x))
            .clamp(0, render_width as i64)) as usize;
        let draw_end_x = (((i64::from(sprite_width)) / 2 + i64::from(sprite_screen_x))
            .clamp(0, render_width as i64)) as usize;

        if draw_start_x >= draw_end_x {
            continue;
        }

        let texture = &textures[sprite.texture_index];
        let use_atlas = texture.width() as usize >= atlas_width
            && texture.height() as usize >= atlas_height;
        let frame_step = (anim_elapsed_ms / ANIM_MS_PER_FRAME).floor() as u32;
        let frame_col = walk_frame_col(frame_step, sprite.is_moving) as usize;
        let side_row = side_to_row(get_visible_side(
            sprite.movement_angle,
            camera_facing_angle,
        )) as usize;
        let frame_origin_x = if use_atlas { frame_col * FRAME_W } else { 0 };
        let frame_origin_y = if use_atlas { side_row * FRAME_H } else { 0 };

        for sx in draw_start_x..draw_end_x {
            let tex_x = ((sx as i32 - (-sprite_width / 2 + sprite_screen_x))
                * FRAME_W as i32
                / sprite_width)
                .rem_euclid(FRAME_W as i32) as usize;
            if transform_y >= z_buffer[sx] {
                continue;
            }
            for sy in draw_start_y..draw_end_y {
                let d = sy as i32 - render_height as i32 / 2 - pitch_px + sprite_height / 2;
                let tex_y = (d * FRAME_H as i32 / sprite_height).rem_euclid(FRAME_H as i32)
                    as usize;
                let color = texture.get_pixel(
                    (frame_origin_x + tex_x) as u32,
                    (frame_origin_y + tex_y) as u32,
                );

                if color[3] > 0 {
                    buffer[sy * render_width + sx] =
                        ((color[0] as u32) << 16) | ((color[1] as u32) << 8) | (color[2] as u32);
                }
            }
        }
    }
}
