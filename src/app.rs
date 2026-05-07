use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::gpu_renderer::{SceneRenderer, SCENE_HEIGHT, SCENE_WIDTH};
use crate::input::InputMessage;
use crate::model::PlayerId;
use crate::net::server::Server;
use crate::render_assembly;
use crate::texture::TextureManager;

const TARGET_FPS: u64 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MouseCaptureMode {
    None,
    Locked,
    ConfinedWarp,
}

struct WindowState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    renderer: SceneRenderer,
}

pub struct App {
    state: Option<WindowState>,
    server: Server,
    input_queue: Arc<Mutex<VecDeque<InputMessage>>>,
    human_id: PlayerId,
    keys: HashSet<KeyCode>,
    last_frame: Instant,
    frame_duration: Duration,
    mouse_sensitivity: f64,
    texture_manager: Option<TextureManager>,
    current_tick: u64,
    anim_elapsed_ms: f64,
    mouse_capture_mode: MouseCaptureMode,
    ignore_next_motion: bool,
    pending_fire: bool,
}

impl App {
    pub fn new(
        server: Server,
        input_queue: Arc<Mutex<VecDeque<InputMessage>>>,
        human_id: PlayerId,
        texture_manager: TextureManager,
    ) -> Self {
        Self {
            state: None,
            server,
            input_queue,
            human_id,
            keys: HashSet::new(),
            last_frame: Instant::now(),
            frame_duration: Duration::from_nanos(1_000_000_000 / TARGET_FPS),
            mouse_sensitivity: 0.003,
            texture_manager: Some(texture_manager),
            current_tick: 0,
            anim_elapsed_ms: 0.0,
            mouse_capture_mode: MouseCaptureMode::None,
            ignore_next_motion: false,
            pending_fire: false,
        }
    }

    fn center_cursor(window: &Window) {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        let center = winit::dpi::PhysicalPosition::new(
            f64::from(size.width) * 0.5,
            f64::from(size.height) * 0.5,
        );
        let _ = window.set_cursor_position(center);
    }

    fn set_mouse_capture(window: &Window, capture: bool) -> MouseCaptureMode {
        if capture {
            // WINDOWS: prefer Confined to keep clicks inside the game window
            if cfg!(target_os = "windows") {
                if window.set_cursor_grab(CursorGrabMode::Confined).is_ok() {
                    window.set_cursor_visible(false);
                    Self::center_cursor(window);
                    return MouseCaptureMode::ConfinedWarp;
                }

                if window.set_cursor_grab(CursorGrabMode::Locked).is_ok() {
                    window.set_cursor_visible(false);
                    return MouseCaptureMode::Locked;
                }
            } else {
                if window.set_cursor_grab(CursorGrabMode::Locked).is_ok() {
                    window.set_cursor_visible(false);
                    return MouseCaptureMode::Locked;
                }

                if window.set_cursor_grab(CursorGrabMode::Confined).is_ok() {
                    window.set_cursor_visible(false);
                    Self::center_cursor(window);
                    return MouseCaptureMode::ConfinedWarp;
                }
            }

            window.set_cursor_visible(true);
            MouseCaptureMode::None
        } else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
            MouseCaptureMode::None
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
            fire: std::mem::take(&mut self.pending_fire),
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

        let scene = match render_assembly::assemble_scene(&self.server.state, self.human_id) {
            Some(scene) => scene,
            None => return,
        };

        let output = match state.surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                state
                    .surface
                    .configure(&state.renderer.device, &state.surface_config);
                return;
            }
            Err(_) => {
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            state
                .renderer
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cpu_game_scene_encoder"),
                });

        let (vx, vy, vw, vh) = SceneRenderer::calculate_aspect_preserving_viewport(
            state.surface_config.width,
            state.surface_config.height,
            SCENE_WIDTH,
            SCENE_HEIGHT,
        );

        state
            .renderer
            .render_frame(
                &mut encoder,
                &view,
                (vx, vy, vw, vh),
                &scene.camera,
                &scene.billboards,
                self.anim_elapsed_ms,
                self.current_tick,
            );

        state.renderer.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            if let Some(state) = &mut self.state {
                state.surface_config.width = new_size.width;
                state.surface_config.height = new_size.height;
                state
                    .surface
                    .configure(&state.renderer.device, &state.surface_config);
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("cpu-game")
            .with_inner_size(winit::dpi::PhysicalSize::new(SCENE_WIDTH, SCENE_HEIGHT))
            .with_resizable(false);

        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .unwrap();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("cpu_game_device"),
            required_features: wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER
                | wgpu::Features::FLOAT32_FILTERABLE,
            required_limits: wgpu::Limits::default(),
            memory_hints: Default::default(),
            ..Default::default()
        }))
        .unwrap();

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let caps = surface.get_capabilities(&adapter);
        let surface_format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let texture_manager = self
            .texture_manager
            .take()
            .expect("texture manager should only be consumed once");
        let renderer = SceneRenderer::new(
            device.clone(),
            queue.clone(),
            &self.server.map,
            &texture_manager,
            surface_format,
        );

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: SCENE_WIDTH,
            height: SCENE_HEIGHT,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        self.mouse_capture_mode = Self::set_mouse_capture(window.as_ref(), false);
        self.ignore_next_motion = false;
        self.state = Some(WindowState {
            window,
            surface,
            surface_config,
            renderer,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(focused) => {
                if let Some(state) = &self.state {
                    if !focused {
                        self.mouse_capture_mode =
                            Self::set_mouse_capture(state.window.as_ref(), false);
                        self.ignore_next_motion = false;
                    }
                }
            }
            WindowEvent::Resized(new_size) => {
                self.resize(new_size);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if code == KeyCode::Escape && event.state == ElementState::Pressed {
                        if let Some(state) = &self.state {
                            let should_capture = self.mouse_capture_mode == MouseCaptureMode::None;
                            self.mouse_capture_mode =
                                Self::set_mouse_capture(state.window.as_ref(), should_capture);
                            self.ignore_next_motion =
                                should_capture && self.mouse_capture_mode != MouseCaptureMode::None;
                        }
                        return;
                    }

                    if code == KeyCode::Space && event.state == ElementState::Pressed {
                        self.pending_fire = true;
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
            WindowEvent::MouseInput { state: mouse_state, button, .. } => {
                if mouse_state == ElementState::Released
                    && self.mouse_capture_mode == MouseCaptureMode::None
                {
                    if let Some(state) = &self.state {
                        self.mouse_capture_mode =
                            Self::set_mouse_capture(state.window.as_ref(), true);
                        self.ignore_next_motion =
                            self.mouse_capture_mode != MouseCaptureMode::None;
                    }
                }

                if mouse_state == ElementState::Pressed
                    && button == MouseButton::Left
                    && self.mouse_capture_mode != MouseCaptureMode::None
                {
                    self.pending_fire = true;
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
        if self.mouse_capture_mode != MouseCaptureMode::None {
            if let DeviceEvent::MouseMotion { delta: (dx, _dy) } = event {
                if self.ignore_next_motion {
                    self.ignore_next_motion = false;
                    return;
                }
                self.push_rotation(-dx * self.mouse_sensitivity);
            }
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
                if self.mouse_capture_mode == MouseCaptureMode::ConfinedWarp {
                    Self::center_cursor(state.window.as_ref());
                    self.ignore_next_motion = true;
                }
                state.window.request_redraw();
            }
        }

        let next_frame = self.last_frame + self.frame_duration;
        event_loop.set_control_flow(ControlFlow::WaitUntil(next_frame));
    }
}
