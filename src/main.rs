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
use map::load_map;

const WIDTH: usize = 640;
const HEIGHT: usize = 480;
const TARGET_FPS: u64 = 60;

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

struct WindowState {
    window: Arc<Window>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
}

struct App {
    state: Option<WindowState>,
    player: Player,
    map: Vec<Vec<u8>>,
    keys: HashSet<KeyCode>,
    last_frame: Instant,
    frame_duration: Duration,
    /// Mouse-look sensitivity in radians per pixel
    mouse_sensitivity: f64,
}

impl App {
    fn new() -> Self {
        let map = load_map("textures/map.png");
        Self {
            state: None,
            player: Player {
                x: 22.0,
                y: 12.0,
                dir_x: -1.0,
                dir_y: 0.0,
                plane_x: 0.0,
                plane_y: 0.66,
                move_speed: 5.0,
                rot_speed: 3.0,
            },
            map,
            keys: HashSet::new(),
            last_frame: Instant::now(),
            frame_duration: Duration::from_nanos(1_000_000_000 / TARGET_FPS),
            mouse_sensitivity: 0.003,
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
        let move_step = self.player.move_speed * delta;
        let rot_step = self.player.rot_speed * delta;

        if self.keys.contains(&KeyCode::KeyW) {
            if self.map[(self.player.x + self.player.dir_x * move_step) as usize]
                [self.player.y as usize]
                == 0
            {
                self.player.x += self.player.dir_x * move_step;
            }
            if self.map[self.player.x as usize]
                [(self.player.y + self.player.dir_y * move_step) as usize]
                == 0
            {
                self.player.y += self.player.dir_y * move_step;
            }
        }
        if self.keys.contains(&KeyCode::KeyS) {
            if self.map[(self.player.x - self.player.dir_x * move_step) as usize]
                [self.player.y as usize]
                == 0
            {
                self.player.x -= self.player.dir_x * move_step;
            }
            if self.map[self.player.x as usize]
                [(self.player.y - self.player.dir_y * move_step) as usize]
                == 0
            {
                self.player.y -= self.player.dir_y * move_step;
            }
        }
        if self.keys.contains(&KeyCode::KeyA) {
            self.rotate(rot_step);
        }
        if self.keys.contains(&KeyCode::KeyD) {
            self.rotate(-rot_step);
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

        let mut buffer = state.surface.buffer_mut().unwrap();

        let bg = srgb_to_u32(BACKGROUND_COLOR);
        for pixel in buffer.iter_mut() {
            *pixel = bg;
        }

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
            let draw_start = ((-line_height / 2 + HEIGHT as i32 / 2).max(0)) as usize;
            let draw_end =
                ((line_height / 2 + HEIGHT as i32 / 2).min(HEIGHT as i32 - 1)) as usize;

            let wall_color = srgb_to_u32(WALL_COLOR);
            let color = if side == 0 { wall_color } else { wall_color / 2 };

            for y in draw_start..draw_end {
                buffer[y * WIDTH + x] = color;
            }
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

        // grab and hide the cursor for mouse look
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
                let now = Instant::now();
                let delta = now.duration_since(self.last_frame).as_secs_f64();
                self.last_frame = now;
                self.update(delta);
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
        // Mouse motion drives horizontal look
        if let DeviceEvent::MouseMotion { delta: (dx, _dy) } = event {
            self.rotate(-dx * self.mouse_sensitivity);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Sleep for the remainder of the frame budget, then request a redraw.
        // This caps CPU usage to ~TARGET_FPS instead of spinning at max speed.
        let elapsed = self.last_frame.elapsed();
        if elapsed < self.frame_duration {
            std::thread::sleep(self.frame_duration - elapsed);
        }
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
