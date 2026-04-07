//! GPU initialization utilities for presenter examples.
//!
//! Provides common patterns for wgpu setup including adapter selection,
//! device creation, and surface configuration.

use std::sync::Arc;
use wgpu::{Adapter, Device, Instance, Queue, Surface};
use tracing::info;

/// Context containing initialized GPU resources.
#[derive(Clone)]
pub struct GpuContext {
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    pub adapter: Arc<Adapter>,
    pub adapter_info: wgpu::AdapterInfo,
}

/// Builder for GPU context initialization.
pub struct GpuContextBuilder<'a> {
    instance: &'a Instance,
    backends: wgpu::Backends,
    required_features: wgpu::Features,
    surface: Option<&'a Surface<'a>>,
}

impl<'a> GpuContextBuilder<'a> {
    /// Create a new builder with an existing instance.
    pub fn new(instance: &'a Instance) -> Self {
        Self {
            instance,
            backends: wgpu::Backends::PRIMARY,
            required_features: wgpu::Features::empty(),
            surface: None,
        }
    }

    /// Set the backends to use for adapter enumeration.
    pub fn with_backends(mut self, backends: wgpu::Backends) -> Self {
        self.backends = backends;
        self
    }

    /// Set the required features for device creation.
    pub fn with_features(mut self, features: wgpu::Features) -> Self {
        self.required_features = features;
        self
    }

    /// Set the surface for adapter compatibility checking.
    pub fn with_surface(mut self, surface: &'a Surface<'a>) -> Self {
        self.surface = Some(surface);
        self
    }

    /// Build the GPU context.
    pub fn build(self) -> anyhow::Result<GpuContext> {
        // Select adapter
        let adapter = select_best_adapter(self.instance, self.backends, self.surface)?;

        let adapter_info = adapter.get_info();
        info!(
            "Selected GPU: {} ({:?})",
            adapter_info.name, adapter_info.device_type
        );

        // Request device and queue
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: self.required_features,
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
                ..Default::default()
            },
        ))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);
        let adapter = Arc::new(adapter);

        Ok(GpuContext {
            device,
            queue,
            adapter,
            adapter_info,
        })
    }
}

/// Select the best available adapter from the given backends.
///
/// Prioritizes discrete GPUs over integrated GPUs over virtual GPUs.
/// If a surface is provided, only adapters that support the surface are considered.
pub fn select_best_adapter(
    instance: &Instance,
    backends: wgpu::Backends,
    surface: Option<&Surface>,
) -> anyhow::Result<Adapter> {
    // Enumerate adapters for given backends
    let adapters = instance.enumerate_adapters(backends);

    // Filter by surface compatibility if surface provided
    let compatible_adapters: Vec<Adapter> = if let Some(surface) = surface {
        adapters
            .into_iter()
            .filter(|adapter| adapter.is_surface_supported(surface))
            .collect()
    } else {
        adapters
    };

    // Score adapters by device type
    let best_adapter = compatible_adapters
        .into_iter()
        .max_by_key(|adapter| {
            let info = adapter.get_info();
            match info.device_type {
                wgpu::DeviceType::DiscreteGpu => 100,
                wgpu::DeviceType::IntegratedGpu => 50,
                wgpu::DeviceType::VirtualGpu => 25,
                _ => 0,
            }
        })
        .ok_or_else(|| anyhow::anyhow!("No compatible adapter found"))?;

    Ok(best_adapter)
}

/// Configure a surface for rendering.
///
/// Prefers sRGB formats when available and uses standard settings for examples.
pub fn configure_surface(
    surface: &Surface,
    adapter: &Adapter,
    _device: &Device,
    width: u32,
    height: u32,
) -> wgpu::SurfaceConfiguration {
    let surface_caps = surface.get_capabilities(adapter);

    // Select sRGB format if available, otherwise use first format
    let surface_format = surface_caps
        .formats
        .iter()
        .find(|f| f.is_srgb())
        .copied()
        .unwrap_or(surface_caps.formats[0]);

    wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width: width.max(1),
        height: height.max(1),
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode: surface_caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    }
}
