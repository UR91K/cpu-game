use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use engine_core::PresentationRequest;
use shader_test::ShaderRenderer;
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
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    presenter: ShaderRenderer,
    frame_rgb: Vec<u32>,
    frame_rgba: Vec<u8>,
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

        let player = match self.server.state.players.get(&self.human_id) {
            Some(p) => p.clone(),
            None => return,
        };
        let sprites = self.server.state.sprites.clone();

        renderer::render(
            &mut state.frame_rgb,
            &player,
            &sprites,
            &self.server.map,
            &self.textures,
            self.pitch,
            self.anim_elapsed_ms,
        );

        for (src, dst) in state
            .frame_rgb
            .iter()
            .zip(state.frame_rgba.chunks_exact_mut(4))
        {
            dst[0] = ((src >> 16) & 0xFF) as u8;
            dst[1] = ((src >> 8) & 0xFF) as u8;
            dst[2] = (src & 0xFF) as u8;
            dst[3] = 255;
        }

        let mut request = PresentationRequest::new(
            std::mem::take(&mut state.frame_rgba),
            WIDTH as u32,
            HEIGHT as u32,
            self.current_tick,
        );
        state.presenter.load_presentation(&request);
        state.frame_rgba = std::mem::take(&mut request.pixel_data);

        let output = match state.surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                state
                    .surface
                    .configure(&state.presenter.device, &state.surface_config);
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
                .presenter
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cpu_game_presenter_encoder"),
                });

        let (vx, vy, vw, vh) = ShaderRenderer::calculate_aspect_preserving_viewport(
            state.surface_config.width,
            state.surface_config.height,
            WIDTH as u32,
            HEIGHT as u32,
        );

        {
            let _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cpu_game_clear_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        if state
            .presenter
            .render_frame_to_viewport(
                &mut encoder,
                &view,
                librashader::runtime::Size::new(vw, vh),
                state.surface_config.format,
                vx,
                vy,
            )
            .is_err()
        {
            return;
        }

        state.presenter.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            if let Some(state) = &mut self.state {
                state.surface_config.width = new_size.width;
                state.surface_config.height = new_size.height;
                state
                    .surface
                    .configure(&state.presenter.device, &state.surface_config);
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("cpu-game")
            .with_inner_size(winit::dpi::PhysicalSize::new(WIDTH as u32, HEIGHT as u32))
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

        let mut presenter = ShaderRenderer::new(device.clone(), queue.clone());
        presenter.load_default_preset().unwrap();

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: WIDTH as u32,
            height: HEIGHT as u32,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        self.mouse_captured = Self::set_mouse_capture(window.as_ref(), true);
        self.state = Some(WindowState {
            window,
            surface,
            surface_config,
            presenter,
            frame_rgb: vec![0u32; WIDTH * HEIGHT],
            frame_rgba: vec![0u8; WIDTH * HEIGHT * 4],
        });
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
            WindowEvent::Resized(new_size) => {
                self.resize(new_size);
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
