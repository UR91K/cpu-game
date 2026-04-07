//! Minimal presenter reference example.

use anyhow::Result;
use engine_core::PresentationRequest;
use shader_test::ShaderRenderer;
use std::sync::Arc;
use tracing::{error, info};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

const CONTENT_W: u32 = 320;
const CONTENT_H: u32 = 240;

struct App {
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    renderer: Option<ShaderRenderer>,
    frame_number: u64,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            surface: None,
            surface_config: None,
            renderer: None,
            frame_number: 0,
        }
    }

    fn init_wgpu(&mut self, window: Arc<Window>) -> Result<()> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance.create_surface(Arc::clone(&window))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("presenter_example_device"),
            required_features: wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER
                | wgpu::Features::FLOAT32_FILTERABLE,
            required_limits: wgpu::Limits::default(),
            memory_hints: Default::default(),
            ..Default::default()
        }))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let mut renderer = ShaderRenderer::new(device, queue);
        renderer.load_default_preset()?;

        self.window = Some(window);
        self.surface = Some(surface);
        self.surface_config = Some(config);
        self.renderer = Some(renderer);
        Ok(())
    }

    fn make_demo_frame(frame: u64) -> PresentationRequest {
        let mut pixels = vec![0u8; (CONTENT_W * CONTENT_H * 4) as usize];
        for y in 0..CONTENT_H {
            for x in 0..CONTENT_W {
                let idx = ((y * CONTENT_W + x) * 4) as usize;
                let shift = (frame as u32) % CONTENT_W;
                let band = ((x + shift) / 20) % 2;
                pixels[idx] = if band == 0 { 255 } else { 24 };
                pixels[idx + 1] = ((y * 255) / CONTENT_H) as u8;
                pixels[idx + 2] = (((x * 255) / CONTENT_W) as u8).saturating_add(20);
                pixels[idx + 3] = 255;
            }
        }
        PresentationRequest::new(pixels, CONTENT_W, CONTENT_H, frame)
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            if let (Some(surface), Some(config), Some(renderer)) =
                (&self.surface, &mut self.surface_config, &self.renderer)
            {
                config.width = new_size.width;
                config.height = new_size.height;
                surface.configure(&renderer.device, config);
            }
        }
    }

    fn render(&mut self) -> Result<()> {
        let surface = self.surface.as_ref().expect("surface missing");
        let renderer = self.renderer.as_mut().expect("renderer missing");

        self.frame_number += 1;
        let request = Self::make_demo_frame(self.frame_number);
        renderer.load_presentation(&request);

        let output = surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            renderer
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("example_render_encoder"),
                });

        let config = self.surface_config.as_ref().expect("surface config missing");
        let (vx, vy, vw, vh) = ShaderRenderer::calculate_aspect_preserving_viewport(
            config.width,
            config.height,
            CONTENT_W,
            CONTENT_H,
        );

        {
            let _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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

        renderer.render_frame_to_viewport(
            &mut encoder,
            &view,
            librashader::runtime::Size::new(vw, vh),
            config.format,
            vx,
            vy,
        )?;

        renderer.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("Presenter Reference Example")
            .with_inner_size(winit::dpi::LogicalSize::new(960, 720));
        let window = Arc::new(event_loop.create_window(attrs).expect("window create failed"));

        if let Err(e) = self.init_wgpu(Arc::clone(&window)) {
            error!(error = %e, "failed to initialize renderer");
            event_loop.exit();
            return;
        }

        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(new_size) => self.resize(new_size),
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state.is_pressed()
                    && matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape))
                {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.render() {
                    error!(error = %e, "render error");
                    event_loop.exit();
                    return;
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new();
    info!("starting presenter example");
    event_loop.run_app(&mut app)?;
    Ok(())
}
