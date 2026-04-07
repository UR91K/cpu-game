use std::collections::{HashSet, VecDeque};
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::input::InputMessage;
use crate::model::PlayerId;
use crate::net::server::Server;
use crate::renderer::{self, HEIGHT, WIDTH};

const TARGET_FPS: u64 = 60;

struct WindowState {
    window: Arc<Window>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
}

pub struct App {
    state: Option<WindowState>,
    server: Server,
    /// Shared queue — App pushes, LocalClient drains via poll_inputs().
    input_queue: Arc<Mutex<VecDeque<InputMessage>>>,
    human_id: PlayerId,
    keys: HashSet<KeyCode>,
    last_frame: Instant,
    frame_duration: Duration,
    mouse_sensitivity: f64,
    textures: Vec<image::RgbaImage>,
    pitch: i32,
    current_tick: u64,
    anim_elapsed_ms: f64,
    mouse_captured: bool,
}

impl App {
    pub fn new(server: Server, input_queue: Arc<Mutex<VecDeque<InputMessage>>>, human_id: PlayerId, textures: Vec<image::RgbaImage>) -> Self {
        Self {
            state: None,
            server,
            input_queue,
            human_id,
            keys: HashSet::new(),
            last_frame: Instant::now(),
            frame_duration: Duration::from_nanos(1_000_000_000 / TARGET_FPS),
            mouse_sensitivity: 0.003,
            textures,
            pitch: 34,
            current_tick: 0,
            anim_elapsed_ms: 0.0,
            mouse_captured: false,
        }
    }

    fn set_mouse_capture(window: &Window, capture: bool) -> bool {
        if capture {
            let grabbed = window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                .is_ok();
            window.set_cursor_visible(!grabbed);
            grabbed
        } else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
            false
        }
    }

    fn update(&mut self, delta: f64) {
        self.anim_elapsed_ms += delta * 1000.0;
        self.current_tick += 1;
        let msg = InputMessage {
            player_id: self.human_id,
            tick: self.current_tick,
            forward: self.keys.contains(&KeyCode::KeyW),
            back: self.keys.contains(&KeyCode::KeyS),
            strafe_left: self.keys.contains(&KeyCode::KeyA),
            strafe_right: self.keys.contains(&KeyCode::KeyD),
            rotate_delta: 0.0, // Mouse rotation is accumulated via device events; see below.
        };
        self.input_queue.lock().unwrap().push_back(msg);
        self.server.tick(delta);
    }

    fn push_rotation(&mut self, angle: f64) {
        // push a rotation-only message immediately so it's included in the next server tick.
        self.current_tick += 1;
        let msg = InputMessage {
            player_id: self.human_id,
            tick: self.current_tick,
            rotate_delta: angle,
            ..Default::default()
        };
        self.input_queue.lock().unwrap().push_back(msg);
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

        let player = match self.server.state.players.get(&self.human_id) {
            Some(p) => p.clone(),
            None => return,
        };
        let sprites = self.server.state.sprites.clone();

        let mut buffer = state.surface.buffer_mut().unwrap();
        renderer::render(
            &mut buffer,
            &player,
            &sprites,
            &self.server.map,
            &self.textures,
            self.pitch,
            self.anim_elapsed_ms,
        );
        buffer.present().unwrap();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("cpu-game")
            .with_inner_size(winit::dpi::PhysicalSize::new(WIDTH as u32, HEIGHT as u32))
            .with_resizable(false);

        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();

        self.mouse_captured = Self::set_mouse_capture(window.as_ref(), true);
        self.state = Some(WindowState { window, surface });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(focused) => {
                if let Some(state) = &self.state {
                    self.mouse_captured =
                        Self::set_mouse_capture(state.window.as_ref(), focused);
                }
            }
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
        if self.mouse_captured {
            let DeviceEvent::MouseMotion { delta: (dx, _dy) } = event else {
                return;
            };
            self.push_rotation(-dx * self.mouse_sensitivity);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let next_frame = self.last_frame + self.frame_duration;

        if now >= next_frame {
            let delta = now.duration_since(self.last_frame).as_secs_f64();
            self.last_frame = now;
            self.update(delta);
            if let Some(state) = &self.state {
                state.window.request_redraw();
            }
        }

        let next_frame = self.last_frame + self.frame_duration;
        event_loop.set_control_flow(ControlFlow::WaitUntil(next_frame));
    }
}
