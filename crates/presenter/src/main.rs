//! Shader test application - wgpu + librashader

use anyhow::Result;
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

struct App {
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    renderer: Option<ShaderRenderer>,
    use_shader: bool,
    blit_pipeline: Option<wgpu::RenderPipeline>,
    blit_bind_group_layout: Option<wgpu::BindGroupLayout>,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            surface: None,
            surface_config: None,
            renderer: None,
            use_shader: true,
            blit_pipeline: None,
            blit_bind_group_layout: None,
        }
    }

    fn init_wgpu(&mut self, window: Arc<Window>) -> Result<()> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            ..Default::default()
        });

        let surface = instance.create_surface(Arc::clone(&window))?;

        // Enumerate all adapters and pick the best one
        let adapters = instance.enumerate_adapters(wgpu::Backends::DX12);
        info!("Available adapters:");
        let mut adapter_list: Vec<wgpu::Adapter> = Vec::new();
        for adapter in adapters {
            let info = adapter.get_info();
            info!(name = %info.name, device_type = ?info.device_type, "  Adapter found");
            adapter_list.push(adapter);
        }

        // Prefer discrete GPU
        let adapter = adapter_list
            .into_iter()
            .filter(|a: &wgpu::Adapter| a.is_surface_supported(&surface))
            .max_by_key(|a: &wgpu::Adapter| {
                let info = a.get_info();
                match info.device_type {
                    wgpu::DeviceType::DiscreteGpu => 100,
                    wgpu::DeviceType::IntegratedGpu => 50,
                    wgpu::DeviceType::VirtualGpu => 25,
                    _ => 0,
                }
            })
            .ok_or_else(|| anyhow::anyhow!("No DX12 adapter found"))?;

        let adapter_info = adapter.get_info();
        info!(
            name = %adapter_info.name,
            device_type = ?adapter_info.device_type,
            "Selected adapter"
        );

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER
                    | wgpu::Features::FLOAT32_FILTERABLE,
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
                ..Default::default()
            }))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Create renderer
        let mut renderer = ShaderRenderer::new(Arc::clone(&device), Arc::clone(&queue));

        // Load shader preset
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

        // Load input image
        info!("Loading input image...");
        let input_size = renderer.load_image("images/t4.png")?;
        info!(width = input_size.width, height = input_size.height, "Loaded image");

        // Create blit pipeline for drawing input directly
        let (blit_pipeline, blit_bind_group_layout) = create_blit_pipeline(&device, surface_format);

        self.window = Some(window);
        self.surface = Some(surface);
        self.surface_config = Some(config);
        self.blit_pipeline = Some(blit_pipeline);
        self.blit_bind_group_layout = Some(blit_bind_group_layout);
        self.renderer = Some(renderer);

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
            }
        }
    }

    fn render(&mut self) -> Result<()> {
        let surface = self.surface.as_ref().unwrap();
        let renderer = self.renderer.as_mut().unwrap();

        let output = surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = renderer
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            });

        if self.use_shader {
            // Render through shader chain
            let config = self.surface_config.as_ref().unwrap();
            let output_size = librashader::runtime::Size::new(config.width, config.height);
            renderer.render_frame(&mut encoder, &view, output_size, config.format)?;
        } else {
            // Clear to black (placeholder for direct blit)
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
            // TODO: Draw input texture directly using blit pipeline
        }

        renderer.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attrs = Window::default_attributes()
            .with_title("Shader Test - wgpu + librashader")
            .with_inner_size(winit::dpi::LogicalSize::new(800, 600));

        let window = Arc::new(event_loop.create_window(window_attrs).unwrap());

        if let Err(e) = self.init_wgpu(Arc::clone(&window)) {
            error!(error = %e, "Failed to initialize wgpu");
            event_loop.exit();
            return;
        }

        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                self.resize(new_size);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state.is_pressed() {
                    if let PhysicalKey::Code(KeyCode::Space) = event.physical_key {
                        self.use_shader = !self.use_shader;
                        info!(shader_enabled = self.use_shader, "Toggled shader");
                    }
                    if let PhysicalKey::Code(KeyCode::Escape) = event.physical_key {
                        event_loop.exit();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.render() {
                    error!(error = %e, "Render error");
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

/// Create a simple blit pipeline for drawing textures directly
fn create_blit_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("blit_shader"),
        source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("blit_bind_group_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("blit_pipeline_layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("blit_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    (pipeline, bind_group_layout)
}

const BLIT_SHADER: &str = r#"
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    // Full-screen triangle
    let x = f32(i32(vertex_index) - 1);
    let y = f32(i32(vertex_index & 1u) * 2 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

@group(0) @binding(0) var t_texture: texture_2d<f32>;
@group(0) @binding(1) var s_sampler: sampler;

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = pos.xy / vec2<f32>(textureDimensions(t_texture));
    return textureSample(t_texture, s_sampler, uv);
}
"#;

fn main() -> Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    info!("Using DX12 backend");

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new();
    event_loop.run_app(&mut app)?;

    info!("Application completed successfully");
    Ok(())
}
