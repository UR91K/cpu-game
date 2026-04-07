//! Tests the presenter with various resolutions to demonstrate resolution independence.
//!
//! This example creates PresentationRequests with different resolutions and renders them
//! through the NTSC shader pipeline to verify that arbitrary resolutions work correctly.

use anyhow::Result;
use tracing::{error, info};
use engine_core::test_helpers::{create_test_image, image_to_presentation};
use engine_core::PresentationRequest;
use shader_test::{ShaderRenderer, examples_common::gpu::{GpuContextBuilder, configure_surface}};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};

// Test different resolutions
const TEST_RESOLUTIONS: &[(u32, u32)] = &[
    (320, 224),   // Classic console (SNES, Genesis)
    (640, 480),   // VGA resolution
    (800, 600),   // SVGA resolution
    (1024, 768),  // XGA resolution
    (1280, 720),  // HD 720p
    (1920, 1080), // Full HD 1080p
    (2560, 1440), // QHD resolution
];

// 60 FPS = ~16.67ms per frame
fn frame_duration() -> Duration {
    Duration::from_secs_f64(1.0 / 60.0)
}

struct App {
    window: Option<Arc<winit::window::Window>>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    renderer: Option<ShaderRenderer>,
    test_frames: Vec<PresentationRequest>,
    current_resolution_idx: usize,
    current_frame_idx: usize,
    next_frame_time: Instant,
    frames_since_resolution_change: usize,
}

impl App {
    fn new() -> Self {
        info!("Creating test frames for various resolutions...");

        let mut test_frames = Vec::new();

        // Create test frames for each resolution
        for (width, height) in TEST_RESOLUTIONS {
            info!("Generating frames for {}x{} resolution", width, height);

            // Create a few frames per resolution to see animation
            for frame_num in 0..10 {
                let image = create_test_image(
                    *width,
                    *height,
                    [64, 128, 255, 255], // Blue background for variety
                    (width / 2) as i32,
                    (height / 2) as i32,
                    30, // Smaller circle for higher resolutions
                    0.5,
                );

                let request = image_to_presentation(&image, frame_num as u64);
                test_frames.push(request);
            }
        }

        info!("Generated {} test frames total", test_frames.len());

        Self {
            window: None,
            surface: None,
            surface_config: None,
            renderer: None,
            test_frames,
            current_resolution_idx: 0,
            current_frame_idx: 0,
            next_frame_time: Instant::now(),
            frames_since_resolution_change: 0,
        }
    }

    fn init_wgpu(&mut self, window: Arc<winit::window::Window>) -> Result<()> {
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
        let mut renderer = ShaderRenderer::new(std::sync::Arc::clone(&gpu_context.device), std::sync::Arc::clone(&gpu_context.queue));

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

        // Load first frame
        let first_frame = &self.test_frames[0];
        renderer.load_presentation(first_frame);
        info!("Initial frame loaded! ({}x{})", first_frame.width, first_frame.height);

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
            self.current_frame_idx = (self.current_frame_idx + 1) % self.test_frames.len();
            let frame = &self.test_frames[self.current_frame_idx];

            renderer.load_presentation(frame);

            // Check if we've moved to a new resolution
            let frames_per_resolution = 10;
            let new_resolution_idx = self.current_frame_idx / frames_per_resolution;

            if new_resolution_idx != self.current_resolution_idx {
                self.current_resolution_idx = new_resolution_idx;
                let (width, height) = TEST_RESOLUTIONS[self.current_resolution_idx];
                info!("Switched to resolution: {}x{}", width, height);
                self.frames_since_resolution_change = 0;
            } else {
                self.frames_since_resolution_change += 1;
            }
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
                label: Some("resolution_test_encoder"),
            });

        // Calculate aspect-ratio correct viewport within the window
        let current_frame = &self.test_frames[self.current_frame_idx];
        let (viewport_x, viewport_y, viewport_width, viewport_height) =
            ShaderRenderer::calculate_aspect_preserving_viewport(
                config.width,
                config.height,
                current_frame.width,
                current_frame.height,
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

        // Render shader output to the calculated viewport region
        let output_size = librashader::runtime::Size::new(viewport_width, viewport_height);
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
        let window_attrs = winit::window::Window::default_attributes()
            .with_title("Resolution Independence Test")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));

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

fn main() -> Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    info!("Starting Resolution Independence Test");
    info!("Testing resolutions: {:?}", TEST_RESOLUTIONS);
    info!(
        "Will cycle through {} resolutions, {} frames each",
        TEST_RESOLUTIONS.len(),
        10
    );

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new();
    event_loop.run_app(&mut app)?;

    info!("Resolution independence test completed successfully!");
    Ok(())
}
