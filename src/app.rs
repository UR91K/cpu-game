use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::font::Font;
use crate::input::InputMessage;
use crate::model::ControllerId;
use crate::net::server::Server;
use crate::render_assembly;
use crate::renderer::scene_renderer::{SCENE_HEIGHT, SCENE_WIDTH, SceneRenderer};
use crate::text_layer::{HAlign, TextLayer, VAlign, place_text, place_text_at};
use crate::texture::TextureManager;

const TARGET_FPS: u64 = 60;

const fn rgba_from_hex(hex: &str, a: u8) -> [u8; 4] {
    let b = hex.as_bytes();
    // b[0] == b'#'
    let r = hex_pair(b[1], b[2]);
    let g = hex_pair(b[3], b[4]);
    let b = hex_pair(b[5], b[6]);
    [r, g, b, a]
}

const fn hex_pair(hi: u8, lo: u8) -> u8 {
    hex_digit(hi) << 4 | hex_digit(lo)
}

const fn hex_digit(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => panic!("invalid hex digit"),
    }
}

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
    texture_manager: TextureManager,
}

pub struct App {
    state: Option<WindowState>,
    server: Server,
    input_queue: Arc<Mutex<VecDeque<InputMessage>>>,
    human_id: ControllerId,
    keys: HashSet<KeyCode>,
    last_frame: Instant,
    frame_duration: Duration,
    mouse_sensitivity: f64,
    fov_plane_len: f64,
    texture_manager: Option<TextureManager>,
    current_tick: u64,
    anim_elapsed_ms: f64,
    mouse_capture_mode: MouseCaptureMode,
    ignore_next_motion: bool,
    pending_fire: bool,
    font: Font,
    text_layer: TextLayer,
    show_font_test: bool,
    show_debug_info: bool,
}

impl App {
    pub fn new(
        server: Server,
        input_queue: Arc<Mutex<VecDeque<InputMessage>>>,
        human_id: ControllerId,
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
            fov_plane_len: 0.85,
            texture_manager: Some(texture_manager),
            current_tick: 0,
            anim_elapsed_ms: 0.0,
            mouse_capture_mode: MouseCaptureMode::None,
            ignore_next_motion: false,
            pending_fire: false,
            font: Font::load(),
            text_layer: TextLayer::new(SCENE_WIDTH, SCENE_HEIGHT),
            show_font_test: false,
            show_debug_info: false,
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
            controller_id: self.human_id,
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
            controller_id: self.human_id,
            tick: self.current_tick,
            rotate_delta: angle,
            ..Default::default()
        };
        self.input_queue.lock().unwrap().push_back(msg);
    }

    fn build_hud(&mut self, _scene: &render_assembly::RenderScene) {
        let controls = "ESC CAPTURE  WASD MOVE  SPACE/LMB FIRE  F4 FONT  F11 FULLSCREEN";
        let test = "Meow! Test Test Test";

        place_text(
            &mut self.text_layer,
            controls,
            HAlign::Center,
            VAlign::Bottom,
            0,
            0,
            rgba_from_hex("#DCDCDA", 255),
            rgba_from_hex("#000000", 140),
        );

        place_text(
            &mut self.text_layer,
            test,
            HAlign::Center,
            VAlign::Bottom,
            0,
            -1,
            rgba_from_hex("#f00cca", 255),
            rgba_from_hex("#23c9f3", 140),
        );
    }
    
    fn build_debug_overlay(&mut self, scene: &render_assembly::RenderScene) {
        let fg = [255, 255, 255, 255];
        let bg = [0, 0, 0, 160];
        let capture = match self.mouse_capture_mode {
            MouseCaptureMode::None => "FREE",
            MouseCaptureMode::Locked => "LOCK",
            MouseCaptureMode::ConfinedWarp => "WARP",
        };

        let status = format!(
            "TICK {:05}  POS {:.1},{:.1}  SPR {:02}  MOUSE {}",
            self.current_tick,
            scene.camera.x,
            scene.camera.y,
            scene.billboards.len(),
            capture,
        );
        place_text(
            &mut self.text_layer,
            &status,
            HAlign::Left,
            VAlign::Top,
            1,
            1,
            fg,
            bg,
        );
    }

    fn build_font_test_overlay(&mut self) {
        use crate::font::{FIRST_ASCII, FONT_COLS, FONT_ROWS};

        place_text(
            &mut self.text_layer,
            "FONT TEST  (F4 TO TOGGLE)",
            HAlign::Center,
            VAlign::Top,
            0,
            0,
            [255, 255, 255, 255],
            [0, 0, 0, 180],
        );

        for row in 0..FONT_ROWS {
            let start = FIRST_ASCII as u32 + (row * FONT_COLS) as u32;
            let end = (start + FONT_COLS as u32).min(128);
            let line: String = (start..end).filter_map(char::from_u32).collect();
            place_text_at(
                &mut self.text_layer,
                &line,
                1,
                row + 3,
                [255, 255, 255, 255],
                [0, 0, 0, 0],
            );
        }
    }

    fn render(&mut self) {
        let scene = match render_assembly::assemble_scene(
            &self.server.state,
            self.human_id,
            self.fov_plane_len,
        ) {
            Some(scene) => scene,
            None => return,
        };

        if let Some(state) = &self.state {
            let scene_size = state.renderer.scene_size();
            let expected_size = (scene_size.0 as usize, scene_size.1 as usize);
            if self.text_layer.scene_size() != expected_size {
                self.text_layer = TextLayer::new(scene_size.0, scene_size.1);
            }
        }

        self.text_layer.clear_all();
        if self.show_font_test {
            self.build_font_test_overlay();
        } else {
            self.build_hud(&scene);
            if self.show_debug_info {
                self.build_debug_overlay(&scene);
            }
        }
        let (overlay_width, overlay_height) = self.text_layer.scene_size();
        let mut overlay_buf = vec![0u8; overlay_width * overlay_height * 4];
        self.text_layer.render_to_buf(&mut overlay_buf, &self.font);

        let Some(state) = &mut self.state else {
            return;
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
            state.renderer.scene_size().0,
            state.renderer.scene_size().1,
        );

        state.renderer.render_frame(
            &mut encoder,
            &view,
            (vx, vy, vw, vh),
            &scene.camera,
            &scene.billboards,
            self.anim_elapsed_ms,
            self.current_tick,
            Some(overlay_buf.as_slice()),
        );

        state
            .renderer
            .queue
            .submit(std::iter::once(encoder.finish()));
        output.present();
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            if let Some(state) = &mut self.state {
                state.surface_config.width = new_size.width;
                state.surface_config.height = new_size.height;
                let scene_width =
                    SceneRenderer::calculate_scene_width(new_size.width, new_size.height);
                state.renderer = SceneRenderer::new(
                    state.renderer.device.clone(),
                    state.renderer.queue.clone(),
                    &self.server.level,
                    &state.texture_manager,
                    state.surface_config.format,
                    scene_width,
                    SCENE_HEIGHT,
                );
                self.text_layer = TextLayer::new(scene_width, SCENE_HEIGHT);
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
            .with_resizable(true);

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
        let window_size = window.inner_size();
        let scene_width =
            SceneRenderer::calculate_scene_width(window_size.width, window_size.height);
        let renderer = SceneRenderer::new(
            device.clone(),
            queue.clone(),
            &self.server.level,
            &texture_manager,
            surface_format,
            scene_width,
            SCENE_HEIGHT,
        );
        self.text_layer = TextLayer::new(scene_width, SCENE_HEIGHT);

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
            texture_manager,
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

                    if code == KeyCode::F11 && event.state == ElementState::Pressed {
                        if let Some(state) = &self.state {
                            let is_fullscreen = state.window.fullscreen().is_some();
                            state.window.set_fullscreen(if is_fullscreen {
                                None
                            } else {
                                Some(winit::window::Fullscreen::Borderless(None))
                            });
                        }
                        return;
                    }

                    if code == KeyCode::F4 && event.state == ElementState::Pressed {
                        self.show_font_test = !self.show_font_test;
                        return;
                    }

                    if code == KeyCode::F3 && event.state == ElementState::Pressed {
                        self.show_debug_info = !self.show_debug_info;
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
            WindowEvent::MouseInput {
                state: mouse_state,
                button,
                ..
            } => {
                if mouse_state == ElementState::Released
                    && self.mouse_capture_mode == MouseCaptureMode::None
                {
                    if let Some(state) = &self.state {
                        self.mouse_capture_mode =
                            Self::set_mouse_capture(state.window.as_ref(), true);
                        self.ignore_next_motion = self.mouse_capture_mode != MouseCaptureMode::None;
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
