use minifb::{Key, Window, WindowOptions};
use palette::Srgb;

const WIDTH: usize = 640;
const HEIGHT: usize = 480;

const MAP_WIDTH: usize = 32;
const MAP_HEIGHT: usize = 32;

const FORWARD_KEY: Key = Key::W;
const BACKWARD_KEY: Key = Key::S;
const LEFT_KEY: Key = Key::A;
const RIGHT_KEY: Key = Key::D;

// 2d array of 32x32 representing the world map, where 0 is empty space and 1 is a wall
// TODO: load this from a bitmap instead of hardcoding it, and maybe add more types of walls and objects in the future
const WORLD_MAP: [[u8; MAP_WIDTH]; MAP_HEIGHT] = [
    [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,1,1,1,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,1,1,1,1,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,1,1,1,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,1,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,1,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,1,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,1,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],
    [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],
];

const WALL_COLOR: Srgb = Srgb::new(90.0, 0.0, 140.0);
const BACKGROUND_COLOR: Srgb = Srgb::new(30.0, 30.0, 30.0);

fn srgb_to_u32(color: Srgb) -> u32 {
    let r = (color.red * 255.0) as u32;
    let g = (color.green * 255.0) as u32;
    let b = (color.blue * 255.0) as u32;
    (r << 16) | (g << 8) | b
}



struct Player {
    x: f64,
    y: f64,
    dir_x: f64,
    dir_y: f64,
    plane_x: f64,
    plane_y: f64,
    move_speed: f64,
    rot_speed: f64,
}

fn draw_vertical_line(buffer: &mut Vec<u32>, x: usize, start: usize, end: usize, color: u32) {
    for y in start..end {
        buffer[y * WIDTH + x] = color;
    }
}

fn main() {
    // TODO: put all this stuff in a struct
    let width = WIDTH;
    let height = HEIGHT;
    let title = "Raycaster";
    let target_fps = 60;
    let mut last_frame = std::time::Instant::now();

    
    let mut player = Player {
        x: 22.0,
        y: 12.0,
        dir_x: -1.0,
        dir_y: 0.0,
        plane_x: 0.0,
        plane_y: 0.66,
        move_speed: 5.0,
        rot_speed: 1.0,
    };

    let mut window = Window::new(
        title,
        width,
        height,
        WindowOptions::default(),
    )
    .unwrap_or_else(|e| {
        panic!("{}", e);
    });

    let mut buffer: Vec<u32> = vec![0; WIDTH * HEIGHT];

    
    window.set_target_fps(target_fps);

    while window.is_open() && !window.is_key_down(Key::Escape) {
            
        for i in buffer.iter_mut() {
            *i = srgb_to_u32(BACKGROUND_COLOR); // clear the buffer by setting all pixels to black
        }

        for x in 0..width {
            let camera_x: f64 = 2.0 * x as f64 / width as f64 - 1.0; // x-coordinate in camera space
            let ray_dir_x: f64 = player.dir_x + player.plane_x * camera_x;
            let ray_dir_y: f64 = player.dir_y + player.plane_y * camera_x;

            //which box of the map we're in
            let mut map_x = player.x as i32;
            let mut map_y = player.y as i32;
            
            //length of ray from current position to next x or y-side
            let mut side_dist_x: f64;
            let mut side_dist_y: f64;

            //length of ray from one x or y-side to next x or y-side
            let delta_dist_x: f64 = if ray_dir_x == 0.0 { 1e30 } else { (1.0 / ray_dir_x).abs() };
            let delta_dist_y: f64 = if ray_dir_y == 0.0 { 1e30 } else { (1.0 / ray_dir_y).abs() };
            let perp_wall_dist: f64;

            // what dir to step in x or y direction (either +1 or -1)
            let step_x: i32; 
            let step_y: i32;

            let mut hit = 0; // did we hit a wall?
            let mut side: i32; // was a north-south wall or a east-west wall hit?

            // calculate step and initial sideDist
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

            //perform DDA
            side = 0;
            while hit == 0 {
                // jump to next map square, either in x-direction, or in y-direction
                if side_dist_x < side_dist_y {
                    side_dist_x += delta_dist_x;
                    map_x += step_x;
                    side = 0;
                } else {
                    side_dist_y += delta_dist_y;
                    map_y += step_y;
                    side = 1;
                }
                // check if the ray has hit a wall
                if WORLD_MAP[map_x as usize][map_y as usize] > 0 {
                    hit = 1;
                }
            }   

            if side == 0 {
                perp_wall_dist = (map_x as f64 - player.x + (1.0 - step_x as f64) / 2.0) / ray_dir_x;
            } else {
                perp_wall_dist = (map_y as f64 - player.y + (1.0 - step_y as f64) / 2.0) / ray_dir_y;
            }

            let line_height: i32 = (height as f64 / perp_wall_dist) as i32;

            let mut draw_start = -line_height / 2 + height as i32 / 2;
            if draw_start < 0 {
                draw_start = 0;
            }
            let mut draw_end = line_height / 2 + height as i32 / 2;
            if draw_end >= height as i32 {
                draw_end = height as i32 - 1;
            }

            // TODO: replace this with a texture later
            let mut color: u32 = if side == 0 {
                // make x-sides brighter
                srgb_to_u32(WALL_COLOR)
            } else {
                // make y-sides darker
                srgb_to_u32(WALL_COLOR * 0.5)
            };

            if side == 1 {color = color / 2;}

            draw_vertical_line(&mut buffer, x, draw_start as usize, draw_end as usize, color);
        }

        let frame_time = last_frame.elapsed().as_secs_f64();
        last_frame = std::time::Instant::now();
        let move_step = player.move_speed * frame_time;
        let rot_step = player.rot_speed * frame_time;

        if window.is_key_down(FORWARD_KEY) {
            if WORLD_MAP[(player.x + player.dir_x * move_step) as usize][player.y as usize] == 0 {
                player.x += player.dir_x * move_step;
            }
            if WORLD_MAP[player.x as usize][(player.y + player.dir_y * move_step) as usize] == 0 {
                player.y += player.dir_y * move_step;
            }
        }
        if window.is_key_down(BACKWARD_KEY) {
            if WORLD_MAP[(player.x - player.dir_x * move_step) as usize][player.y as usize] == 0 {
                player.x -= player.dir_x * move_step;
            }
            if WORLD_MAP[player.x as usize][(player.y - player.dir_y * move_step) as usize] == 0 {
                player.y -= player.dir_y * move_step;
            }
        }
        if window.is_key_down(LEFT_KEY) {
            let old_dir_x = player.dir_x;
            player.dir_x = player.dir_x * f64::cos(rot_step) - player.dir_y * f64::sin(rot_step);
            player.dir_y = old_dir_x * f64::sin(rot_step) + player.dir_y * f64::cos(rot_step);
            let old_plane_x = player.plane_x;
            player.plane_x = player.plane_x * f64::cos(rot_step) - player.plane_y * f64::sin(rot_step);
            player.plane_y = old_plane_x * f64::sin(rot_step) + player.plane_y * f64::cos(rot_step);
        }
        if window.is_key_down(RIGHT_KEY) {
            let old_dir_x = player.dir_x;
            player.dir_x = player.dir_x * f64::cos(-rot_step) - player.dir_y * f64::sin(-rot_step);
            player.dir_y = old_dir_x * f64::sin(-rot_step) + player.dir_y * f64::cos(-rot_step);
            let old_plane_x = player.plane_x;
            player.plane_x = player.plane_x * f64::cos(-rot_step) - player.plane_y * f64::sin(-rot_step);
            player.plane_y = old_plane_x * f64::sin(-rot_step) + player.plane_y * f64::cos(-rot_step);
        }
        
        // TODO: maybe remove the unwrap and handle the error properly
        window
            .update_with_buffer(&buffer, width, height)
            .unwrap();
    }
}
