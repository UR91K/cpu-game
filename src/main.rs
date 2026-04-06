use std::collections::HashSet;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use palette::Srgb;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

mod map;
mod texture;
use map::load_map;

const WIDTH: usize = 640;
const HEIGHT: usize = 480;
const TARGET_FPS: u64 = 60;
const TEXTURE_SIZE: usize = 64;

const FLOOR_COLOR: Srgb = Srgb::new(50.0, 50.0, 50.0);
const CEILING_COLOR: Srgb = Srgb::new(20.0, 20.0, 20.0);

fn srgb_to_u32(color: Srgb) -> u32 {
    let r = color.red as u32;
    let g = color.green as u32;
    let b = color.blue as u32;
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
    vel_x: f64,
    vel_y: f64,
    friction: f64,
}

struct Sprite {
    x: f64,
    y: f64,
    texture_index: usize,
}

struct WindowState {
    window: Arc<Window>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
}

struct App {
    state: Option<WindowState>,
    player: Player,
    sprites: Vec<Sprite>,
    map: Vec<Vec<u8>>,
    keys: HashSet<KeyCode>,
    last_frame: Instant,
    frame_duration: Duration,
    mouse_sensitivity: f64,
    textures: Vec<image::RgbImage>,
    pitch: i32,
    delta: f64,
    skip_mouse_event: bool,
}

impl App {
    fn new() -> Self {
        let map = load_map("textures/map.png");
        let textures = texture::load_textures("textures");
        Self {
            state: None,
            player: Player {
                x: 21.0,
                y: 11.0,
                dir_x: -1.0,
                dir_y: 0.0,
                plane_x: 0.0,
                plane_y: 0.66,
                move_speed: 40.0,
                vel_x: 0.0,
                vel_y: 0.0,
                friction: 10.0,
            },
            // TODO: replace this with empty vec and populate with actual sprites from map/server
            // or just get the sprite data from the entities when they are added.
            sprites: vec![
                Sprite {
                    x: 21.0,
                    y: 11.0,
                    texture_index: 3,
                }
            ],
            map,
            keys: HashSet::new(),
            last_frame: Instant::now(),
            frame_duration: Duration::from_nanos(1_000_000_000 / TARGET_FPS),
            mouse_sensitivity: 0.003,
            textures,
            pitch: 34,
            delta: 0.0,
            skip_mouse_event: false,
        }
    }

    fn rotate(&mut self, angle: f64) {
        let (sin, cos) = angle.sin_cos();
        let old_dir_x = self.player.dir_x;
        self.player.dir_x = old_dir_x * cos - self.player.dir_y * sin;
        self.player.dir_y = old_dir_x * sin + self.player.dir_y * cos;
        let old_plane_x = self.player.plane_x;
        self.player.plane_x = old_plane_x * cos - self.player.plane_y * sin;
        self.player.plane_y = old_plane_x * sin + self.player.plane_y * cos;
    }

    fn update(&mut self, delta: f64) {
        let mut move_dir_x = 0.0f64;
        let mut move_dir_y = 0.0f64;

        if self.keys.contains(&KeyCode::KeyW) {
            move_dir_x += self.player.dir_x;
            move_dir_y += self.player.dir_y;
        }
        if self.keys.contains(&KeyCode::KeyS) {
            move_dir_x -= self.player.dir_x;
            move_dir_y -= self.player.dir_y;
        }
        if self.keys.contains(&KeyCode::KeyA) {
            move_dir_x -= self.player.plane_x;
            move_dir_y -= self.player.plane_y;
        }
        if self.keys.contains(&KeyCode::KeyD) {
            move_dir_x += self.player.plane_x;
            move_dir_y += self.player.plane_y;
        }

        self.player.vel_x += move_dir_x * self.player.move_speed * delta;
        self.player.vel_y += move_dir_y * self.player.move_speed * delta;

        let speed_sq = self.player.vel_x * self.player.vel_x + self.player.vel_y * self.player.vel_y;
        if speed_sq > 0.0 {
            let speed = speed_sq.sqrt();
            let drop = speed * self.player.friction * delta;
            let new_speed = (speed - drop).max(0.0);
            if new_speed != speed {
                self.player.vel_x *= new_speed / speed;
                self.player.vel_y *= new_speed / speed;
            }
        }

        let dx = self.player.vel_x * delta;
        let dy = self.player.vel_y * delta;

        if self.map[(self.player.x + dx) as usize][self.player.y as usize] == 0 {
            self.player.x += dx;
        } else {
            self.player.vel_x = 0.0;
        }
        if self.map[self.player.x as usize][(self.player.y + dy) as usize] == 0 {
            self.player.y += dy;
        } else {
            self.player.vel_y = 0.0;
        }
    }

    fn render(&mut self) {
        let Some(state) = &mut self.state else {
            return;
        };

        state
            .surface
            .resize(
                NonZeroU32::new(WIDTH as u32).unwrap(),
                NonZeroU32::new(HEIGHT as u32).unwrap(),
            )
            .unwrap();
        
        let mut z_buffer = vec![0.0f64; WIDTH];
        let mut buffer = state.surface.buffer_mut().unwrap();

        let floor_color = srgb_to_u32(FLOOR_COLOR);
        let ceiling_color = srgb_to_u32(CEILING_COLOR);
        let horizon = (HEIGHT as i32 / 2 + self.pitch).clamp(0, HEIGHT as i32) as usize;
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                if y < horizon {
                    buffer[y * WIDTH + x] = ceiling_color;
                } else {
                    buffer[y * WIDTH + x] = floor_color;
                }
            }
        }

        // draw walls
        for x in 0..WIDTH {
            let camera_x: f64 = 2.0 * x as f64 / WIDTH as f64 - 1.0;
            let ray_dir_x: f64 = self.player.dir_x + self.player.plane_x * camera_x;
            let ray_dir_y: f64 = self.player.dir_y + self.player.plane_y * camera_x;

            let mut map_x = self.player.x as i32;
            let mut map_y = self.player.y as i32;

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
                side_dist_x = (self.player.x - map_x as f64) * delta_dist_x;
            } else {
                step_x = 1;
                side_dist_x = (map_x as f64 + 1.0 - self.player.x) * delta_dist_x;
            }
            if ray_dir_y < 0.0 {
                step_y = -1;
                side_dist_y = (self.player.y - map_y as f64) * delta_dist_y;
            } else {
                step_y = 1;
                side_dist_y = (map_y as f64 + 1.0 - self.player.y) * delta_dist_y;
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
                if self.map[map_x as usize][map_y as usize] > 0 {
                    break;
                }
            }

            let perp_wall_dist = if side == 0 {
                (map_x as f64 - self.player.x + (1.0 - step_x as f64) / 2.0) / ray_dir_x
            } else {
                (map_y as f64 - self.player.y + (1.0 - step_y as f64) / 2.0) / ray_dir_y
            };

            let line_height = (HEIGHT as f64 / perp_wall_dist) as i32;

            let draw_start = ((-line_height / 2 + HEIGHT as i32 / 2 + self.pitch).max(0)) as usize;
            let draw_end = ((line_height / 2 + HEIGHT as i32 / 2 + self.pitch)
                .min(HEIGHT as i32 - 1)) as usize;

            let texture_index = (self.map[map_x as usize][map_y as usize] - 1) as usize;

            let wall_x = if side == 0 {
                self.player.y + perp_wall_dist * ray_dir_y
            } else {
                self.player.x + perp_wall_dist * ray_dir_x
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

            let step = 1.0 * TEXTURE_SIZE as f64 / line_height as f64;

            let texture = &self.textures[texture_index];
            let texture_position = (draw_start as f64 - self.pitch as f64 - HEIGHT as f64 / 2.0
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
        for sprite in &self.sprites {
                let sprite_x = sprite.x - self.player.x;
                let sprite_y = sprite.y - self.player.y;

                let inv_det = 1.0 / (self.player.plane_x * self.player.dir_y - self.player.dir_x * self.player.plane_y);
                let transform_x = inv_det * (self.player.dir_y * sprite_x - self.player.dir_x * sprite_y);
                let transform_y = inv_det * (-self.player.plane_y * sprite_x + self.player.plane_x * sprite_y);
        }

        buffer.present().unwrap();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("Raycaster")
            .with_inner_size(winit::dpi::PhysicalSize::new(WIDTH as u32, HEIGHT as u32))
            .with_resizable(false);

        let window = Arc::new(event_loop.create_window(attrs).unwrap());

        window.set_cursor_visible(false);
        let _ = window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));

        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();

        self.state = Some(WindowState { window, surface });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if code == KeyCode::Escape {
                        event_loop.exit();
                        return;
                    }
                    match event.state {
                        ElementState::Pressed => {
                            self.keys.insert(code);
                        }
                        ElementState::Released => {
                            self.keys.remove(&code);
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.update(self.delta);
                self.render();
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta: (dx, _dy) } = event {
            if self.skip_mouse_event {
                self.skip_mouse_event = false;
                return;
            }
            self.rotate(-dx * self.mouse_sensitivity);
            if let Some(state) = &self.state {
                let center = winit::dpi::PhysicalPosition::new(WIDTH as f64 / 2.0, HEIGHT as f64 / 2.0);
                let _ = state.window.set_cursor_position(center);
                self.skip_mouse_event = true;
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let next_frame = self.last_frame + self.frame_duration;

        if now >= next_frame {
            self.delta = now.duration_since(self.last_frame).as_secs_f64();
            self.last_frame = now;
            if let Some(state) = &self.state {
                state.window.request_redraw();
            }
        }

        let next_frame = self.last_frame + self.frame_duration;
        event_loop.set_control_flow(ControlFlow::WaitUntil(next_frame));
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}

mod tests {
    #[test]
    fn test_srgb_to_u32() {
        let color = super::Srgb::new(30.0, 30.0, 30.0);
        let color_u32 = super::srgb_to_u32(color);
        println!("u32: {:?}", color_u32);
    }
}
