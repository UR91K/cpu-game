use palette::Srgb;

use crate::model::{Map, Sprite};
use crate::simulation::PlayerState;

pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 480;
pub const TEXTURE_SIZE: usize = 64;

const FLOOR_COLOR: Srgb = Srgb::new(50.0, 50.0, 50.0);
const CEILING_COLOR: Srgb = Srgb::new(20.0, 20.0, 20.0);

fn srgb_to_u32(color: Srgb) -> u32 {
    let r = color.red as u32;
    let g = color.green as u32;
    let b = color.blue as u32;
    (r << 16) | (g << 8) | b
}

pub fn render(
    buffer: &mut [u32],
    player: &PlayerState,
    sprites: &[Sprite],
    map: &Map,
    textures: &[image::RgbaImage],
    pitch: i32,
) {
    let mut z_buffer = vec![0.0f64; WIDTH];

    let floor_color = srgb_to_u32(FLOOR_COLOR);
    let ceiling_color = srgb_to_u32(CEILING_COLOR);
    let horizon = (HEIGHT as i32 / 2 + pitch).clamp(0, HEIGHT as i32) as usize;

    // floor + ceiling
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if y < horizon {
                buffer[y * WIDTH + x] = ceiling_color;
            } else {
                buffer[y * WIDTH + x] = floor_color;
            }
        }
    }

    // walls
    for x in 0..WIDTH {
        let camera_x: f64 = 2.0 * x as f64 / WIDTH as f64 - 1.0;
        let ray_dir_x: f64 = player.dir_x + player.plane_x * camera_x;
        let ray_dir_y: f64 = player.dir_y + player.plane_y * camera_x;

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

        let line_height = (HEIGHT as f64 / perp_wall_dist) as i32;
        let draw_start = ((-line_height / 2 + HEIGHT as i32 / 2 + pitch).max(0)) as usize;
        let draw_end = ((line_height / 2 + HEIGHT as i32 / 2 + pitch).min(HEIGHT as i32 - 1)) as usize;

        let texture_index = (map.tile_at(map_x as usize, map_y as usize) - 1) as usize;

        let wall_x = if side == 0 {
            player.y + perp_wall_dist * ray_dir_y
        } else {
            player.x + perp_wall_dist * ray_dir_x
        };
        let wall_x = wall_x - wall_x.floor();

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
        let texture_position = (draw_start as f64 - pitch as f64 - HEIGHT as f64 / 2.0
            + line_height as f64 / 2.0)
            * step;
        for y in draw_start..draw_end {
            let tex_y =
                ((texture_position + (y - draw_start) as f64 * step) as usize) % TEXTURE_SIZE;
            let color = texture.get_pixel(tex_x as u32, tex_y as u32);
            buffer[y * WIDTH + x] =
                ((color[0] as u32) << 16) | ((color[1] as u32) << 8) | (color[2] as u32);
        }
    }

    // draw sprites
    for sprite in sprites {
        let sprite_x = sprite.x - player.x;
        let sprite_y = sprite.y - player.y;

        let inv_det =
            1.0 / (player.plane_x * player.dir_y - player.dir_x * player.plane_y);
        let transform_x = inv_det * (player.dir_y * sprite_x - player.dir_x * sprite_y);
        let transform_y =
            inv_det * (-player.plane_y * sprite_x + player.plane_x * sprite_y);

        if transform_y <= 0.0 {
            continue;
        }

        let sprite_screen_x =
            ((WIDTH as f64 / 2.0) * (1.0 + transform_x / transform_y)) as i32;

        let sprite_height = (HEIGHT as f64 / transform_y).abs() as i32;
        let draw_start_y = ((-sprite_height / 2 + HEIGHT as i32 / 2 + pitch).max(0)) as usize;
        let draw_end_y =
            ((sprite_height / 2 + HEIGHT as i32 / 2 + pitch).min(HEIGHT as i32 - 1)) as usize;

        let sprite_width = sprite_height;
        let draw_start_x = ((-(i64::from(sprite_width)) / 2 + i64::from(sprite_screen_x))
            .clamp(0, WIDTH as i64)) as usize;
        let draw_end_x = (((i64::from(sprite_width)) / 2 + i64::from(sprite_screen_x))
            .clamp(0, WIDTH as i64)) as usize;

        if draw_start_x >= draw_end_x {
            continue;
        }

        let texture = &textures[sprite.texture_index];
        for sx in draw_start_x..draw_end_x {
            let tex_x = ((sx as i32 - (-sprite_width / 2 + sprite_screen_x)) * TEXTURE_SIZE as i32
                / sprite_width) as u32;
            if transform_y >= z_buffer[sx] {
                continue;
            }
            for sy in draw_start_y..draw_end_y {
                let d = sy as i32 - HEIGHT as i32 / 2 - pitch + sprite_height / 2;
                let tex_y = (d * TEXTURE_SIZE as i32 / sprite_height) as u32;
                let color = texture.get_pixel(tex_x % TEXTURE_SIZE as u32, tex_y % TEXTURE_SIZE as u32);

                if color[3] > 0 {
                    buffer[sy * WIDTH + sx] =
                        ((color[0] as u32) << 16) | ((color[1] as u32) << 8) | (color[2] as u32);
                }
            }
        }
    }
}
