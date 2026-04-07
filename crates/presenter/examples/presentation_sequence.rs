//! Presents a stream of synthetic frames through the presenter pipeline to verify stability.
//!
//! This example fires up a window, runs the librashader shader preset, and feeds 60 FPS
//! `PresentationRequest`s generated from the `engine-core::test_helpers` image helpers.
//! It exits cleanly after a few seconds so it can be used as a lightweight acceptance test.

use anyhow::Result;
use tracing::{error, info};
use engine_core::test_helpers::{create_test_image, image_to_presentation};
use engine_core::PresentationRequest;
use librashader::runtime::Size;
use rayon::prelude::*;
use shader_test::{ShaderRenderer, examples_common::gpu::{GpuContextBuilder, configure_surface}};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

const FRAME_WIDTH: u32 = 320;
const FRAME_HEIGHT: u32 = 224;

// 60 FPS = ~16.67ms per frame
fn frame_duration() -> Duration {
    Duration::from_secs_f64(1.0 / 60.0)
}

fn run_duration() -> Duration {
    Duration::from_secs(600)
}

fn main() -> Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    info!("Starting Presenter Frame Feed Test");

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new();
    event_loop.run_app(&mut app)?;

    info!("Test completed successfully!");
    Ok(())
}

struct App {
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    renderer: Option<ShaderRenderer>,
    frame_generator: FrameGenerator,
    next_frame_time: Instant,
    _deadline: Instant,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            surface: None,
            surface_config: None,
            renderer: None,
            frame_generator: FrameGenerator::new(),
            next_frame_time: Instant::now(),
            _deadline: Instant::now() + run_duration(),
        }
    }

    fn init_wgpu(&mut self, window: Arc<Window>) -> Result<()> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            ..Default::default()
        });

        let surface = instance.create_surface(Arc::clone(&window))?;

        // Use shared GPU context builder
        let gpu_context = GpuContextBuilder::new(&instance)
            .with_backends(wgpu::Backends::DX12)
            .with_features(
                wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER
                    | wgpu::Features::FLOAT32_FILTERABLE
            )
            .with_surface(&surface)
            .build()?;

        // Use shared surface configuration
        let size = window.inner_size();
        let config = configure_surface(
            &surface,
            &gpu_context.adapter,
            &gpu_context.device,
            size.width,
            size.height,
        );
        surface.configure(&gpu_context.device, &config);

        // Create renderer and load shader preset
        let mut renderer = ShaderRenderer::new(Arc::clone(&gpu_context.device), Arc::clone(&gpu_context.queue));

        #[cfg(feature = "embedded-shaders")]
        {
            info!("Loading embedded shader preset...");
            renderer.load_default_preset()?;
            info!("Embedded shader preset loaded!");
        }

        #[cfg(not(feature = "embedded-shaders"))]
        {
            info!("Loading shader preset from file...");
            renderer.load_default_preset()?;
            info!("Shader preset loaded!");
        }

        // Load initial frame
        let initial_frame = self.frame_generator.next();
        renderer.load_presentation(initial_frame);
        info!("Initial frame loaded!");

        self.window = Some(window);
        self.surface = Some(surface);
        self.surface_config = Some(config);
        self.renderer = Some(renderer);
        self.next_frame_time = Instant::now() + frame_duration();

        Ok(())
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            if let (Some(surface), Some(config), Some(renderer)) =
                (&self.surface, &mut self.surface_config, &self.renderer)
            {
                config.width = new_size.width;
                config.height = new_size.height;
                surface.configure(&renderer.device, config);
                info!(
                    "Window resized to: {}x{} (aspect ratio: {:.3})",
                    new_size.width,
                    new_size.height,
                    new_size.width as f32 / new_size.height as f32
                );
            }
        }
    }

    fn load_next_frame(&mut self) {
        if let Some(renderer) = &mut self.renderer {
            let req = self.frame_generator.next();
            renderer.load_presentation(req);
        }
    }

    fn render(&mut self) -> Result<()> {
        let surface = self.surface.as_ref().unwrap();
        let renderer = self.renderer.as_mut().unwrap();
        let config = self.surface_config.as_ref().unwrap();

        let output = surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = renderer
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("presentation_test_encoder"),
            });

        // Calculate aspect-ratio correct viewport within the window
        let (viewport_x, viewport_y, viewport_width, viewport_height) =
            ShaderRenderer::calculate_aspect_preserving_viewport(
                config.width,
                config.height,
                FRAME_WIDTH,
                FRAME_HEIGHT,
            );

        // Clear the entire window to black first
        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear_pass"),
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

        // Set scissor rect to render only to the calculated viewport
        // Render shader output to the calculated viewport region
        let output_size = Size::new(viewport_width, viewport_height);
        renderer.render_frame_to_viewport(
            &mut encoder,
            &view,
            output_size,
            config.format,
            viewport_x,
            viewport_y,
        )?;

        renderer.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    fn update_and_request_redraw(&mut self, _event_loop: &ActiveEventLoop) {
        let now = Instant::now();

        // // Check if we've exceeded the run duration
        // if now >= self.deadline {
        //     info!("Run duration exceeded, exiting...");
        //     _event_loop.exit();
        //     return;
        // }

        // Check if it's time for the next frame
        if now >= self.next_frame_time {
            self.load_next_frame();

            if let Some(window) = &self.window {
                window.request_redraw();
            }

            self.next_frame_time = now + frame_duration();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Calculate window size to show 320x224 content at 2x pixel-perfect scaling
        // 320x224 * 2 = 640x448 logical pixels
        let scale_factor = 3.0;
        let logical_width = FRAME_WIDTH as f64 * scale_factor;
        let logical_height = FRAME_HEIGHT as f64 * scale_factor;

        let window_attrs = Window::default_attributes()
            .with_title("game-manager 60p embedded test")
            .with_inner_size(winit::dpi::LogicalSize::new(logical_width, logical_height));

        let window = Arc::new(event_loop.create_window(window_attrs).unwrap());

        if let Err(e) = self.init_wgpu(Arc::clone(&window)) {
            error!("Failed to initialize wgpu: {}", e);
            event_loop.exit();
            return;
        }

        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                info!("Close requested, exiting...");
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                self.resize(new_size);
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.render() {
                    error!("Render error: {}", e);
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.update_and_request_redraw(event_loop);
    }
}

struct FrameGenerator {
    frames: Vec<PresentationRequest>,
    current_index: usize,
}

impl FrameGenerator {
    fn new() -> Self {
        info!("Pre-generating animation frames...");
        let start = Instant::now();

        // The animation is periodic:
        // - Sine wave period: 180 frames
        // - 4 background colors, each for 180 frames
        // - Full cycle: 4 * 180 = 720 frames
        const CYCLE_LENGTH: usize = 180;
        const NUM_COLORS: usize = 4;
        let total_frames = CYCLE_LENGTH * NUM_COLORS;

        let frames: Vec<PresentationRequest> = (0..total_frames)
            .into_par_iter()
            .map(|frame| Self::generate_frame(frame as u64))
            .collect();

        info!(
            "Pre-generated {} frames in {:?} ({:.1} MB)",
            total_frames,
            start.elapsed(),
            (frames.len() * std::mem::size_of::<PresentationRequest>()) as f64 / 1_000_000.0
        );

        Self {
            frames,
            current_index: 0,
        }
    }

    fn generate_frame(frame: u64) -> PresentationRequest {
        // Create a pulsing/breathing effect: radius oscillates between 10 and 60
        let base_radius = 10;
        let max_radius = 60;
        let radius_range = max_radius - base_radius;

        // Background color cycle: switch every full pulse cycle
        const BG_COLORS: &[[u8; 4]] = &[
            [18, 24, 64, 255], // original
            [64, 18, 24, 255], // reddish
            [24, 64, 18, 255], // greenish
            [36, 24, 64, 255], // purple
        ];

        const BLUR_RADIUS: f32 = 0.5;
        let cycle_length = 180;
        let color_index = ((frame / cycle_length) % BG_COLORS.len() as u64) as usize;
        let background_color = BG_COLORS[color_index];

        // Animation calcs
        let t = (frame as f64) / 180.0;
        let sin_value = (t * std::f64::consts::PI * 2.0).sin();
        let radius_offset = (sin_value * radius_range as f64 / 2.0) + (radius_range as f64 / 2.0);

        let circle_radius = base_radius + radius_offset as i32;
        let circle_x = FRAME_WIDTH as i32 / 2;
        let circle_y = FRAME_HEIGHT as i32 / 2;

        let img = create_test_image(
            FRAME_WIDTH,
            FRAME_HEIGHT,
            background_color,
            circle_x,
            circle_y,
            circle_radius,
            BLUR_RADIUS,
        );

        image_to_presentation(&img, frame)
    }

    fn next(&mut self) -> &PresentationRequest {
        let frame = &self.frames[self.current_index];
        self.current_index = (self.current_index + 1) % self.frames.len();
        frame
    }
}
