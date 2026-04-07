//! Core renderer using wgpu and librashader

use anyhow::{anyhow, Result};
use engine_core::PresentationRequest;
#[cfg(not(feature = "embedded-shaders"))]
use librashader::presets::{ShaderFeatures, ShaderPreset};
use librashader::runtime::wgpu::{FilterChain, FilterChainOptions, FrameOptions, WgpuOutputView};
use librashader::runtime::{Size, Viewport};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use wgpu::{Device, Queue, Texture, TextureFormat};

#[cfg(feature = "embedded-shaders")]
use crate::embedded_shaders::load_embedded_pack;

/// Manages wgpu device, queue, and librashader filter chain
pub struct ShaderRenderer {
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    filter_chain: Option<FilterChain>,
    input_texture: Option<Arc<Texture>>,
    input_size: Size<u32>,
    frame_count: usize,
    start_time: Instant,
}

impl ShaderRenderer {
    /// Create a new renderer with the given wgpu device and queue
    pub fn new(device: Arc<Device>, queue: Arc<Queue>) -> Self {
        Self {
            device,
            queue,
            filter_chain: None,
            input_texture: None,
            input_size: Size::new(0, 0),
            frame_count: 0,
            start_time: Instant::now(),
        }
    }

    /// Load a shader preset from embedded data (when feature is enabled)
    #[cfg(feature = "embedded-shaders")]
    pub fn load_preset_embedded(&mut self) -> Result<()> {
        let pack = load_embedded_pack()?;

        let chain = FilterChain::load_from_pack(
            pack,
            &self.device,
            &self.queue,
            Some(&FilterChainOptions {
                force_no_mipmaps: false,
                enable_cache: false,
                adapter_info: None,
            }),
        )?;

        self.filter_chain = Some(chain);
        Ok(())
    }

    /// Load a shader preset (.slangp file)
    #[cfg(not(feature = "embedded-shaders"))]
    pub fn load_preset(&mut self, preset_path: impl AsRef<Path>) -> Result<()> {
        let preset = ShaderPreset::try_parse(preset_path.as_ref(), ShaderFeatures::empty())?;

        let chain = FilterChain::load_from_preset(
            preset,
            &self.device,
            &self.queue,
            Some(&FilterChainOptions {
                force_no_mipmaps: false,
                enable_cache: false,
                adapter_info: None,
            }),
        )?;

        self.filter_chain = Some(chain);
        Ok(())
    }

    /// Load the default shader preset using the appropriate method.
    ///
    /// When the `embedded-shaders` feature is enabled (default), this loads
    /// shaders from the embedded binary data. When the feature is disabled,
    /// this loads shaders from the filesystem at runtime.
    pub fn load_default_preset(&mut self) -> Result<()> {
        #[cfg(feature = "embedded-shaders")]
        {
            self.load_preset_embedded()
        }

        #[cfg(not(feature = "embedded-shaders"))]
        {
            self.load_preset("shaders/ntsc-composite.slangp")
        }
    }

    /// Load an image file as the input texture
    pub fn load_image(&mut self, path: impl AsRef<Path>) -> Result<Size<u32>> {
        let img = image::open(path.as_ref())?.to_rgba8();
        let (width, height) = img.dimensions();
        let size = Size::new(width, height);

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("input_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[TextureFormat::Rgba8Unorm],
        });

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &img,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.input_texture = Some(Arc::new(texture));
        self.input_size = size;

        Ok(size)
    }

    /// Load pixel data from a PresentationRequest as the input texture
    ///
    /// This replaces any existing input texture with the composited frame data.
    ///
    /// # Panics
    ///
    /// Panics if the PresentationRequest is invalid (pixel_data length doesn't match
    /// width × height × 4, or zero dimensions). An invalid PresentationRequest
    /// indicates a bug in the Compositor and must be caught immediately.
    pub fn load_presentation(&mut self, request: &PresentationRequest) {
        let expected = (request.width as usize) * (request.height as usize) * 4;
        assert!(
            request.is_valid(),
            "Invalid PresentationRequest: Compositor bug - expected {} bytes, got {}",
            expected,
            request.pixel_data.len()
        );
        assert!(
            request.width > 0 && request.height > 0,
            "Invalid PresentationRequest: Compositor bug - zero-sized dimensions ({}x{})",
            request.width,
            request.height
        );

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("presentation_input_texture"),
            size: wgpu::Extent3d {
                width: request.width,
                height: request.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[TextureFormat::Rgba8Unorm],
        });

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &request.pixel_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * request.width),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: request.width,
                height: request.height,
                depth_or_array_layers: 1,
            },
        );

        self.input_texture = Some(Arc::new(texture));
        self.input_size = Size::new(request.width, request.height);
    }

    /// Check if the renderer has valid input data loaded
    ///
    /// Returns true if an input texture is currently loaded and available for rendering.
    pub fn has_input(&self) -> bool {
        self.input_texture.is_some()
    }

    /// Get the input image size
    pub fn input_size(&self) -> Size<u32> {
        self.input_size
    }

    /// Render a frame through the shader chain to the given output texture view
    pub fn render_frame(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        output_size: Size<u32>,
        output_format: TextureFormat,
    ) -> Result<()> {
        self.render_frame_to_viewport(encoder, output_view, output_size, output_format, 0, 0)
    }

    /// Render a frame through the shader chain to a specific viewport region
    ///
    /// This method allows rendering to a specific region of the output texture,
    /// which is useful for aspect ratio preservation with letterboxing/pillarboxing.
    /// The input texture can be any resolution - the shader preset will handle
    /// resolution-independent scaling.
    ///
    /// # Arguments
    /// * `viewport_x`, `viewport_y` - The top-left corner of the viewport region
    /// * `output_size` - The size of the viewport region (not the full output texture)
    pub fn render_frame_to_viewport(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        output_size: Size<u32>,
        output_format: TextureFormat,
        viewport_x: u32,
        viewport_y: u32,
    ) -> Result<()> {
        let filter_chain = self
            .filter_chain
            .as_mut()
            .ok_or_else(|| anyhow!("No shader preset loaded"))?;

        let input_texture = self
            .input_texture
            .as_ref()
            .ok_or_else(|| anyhow!("No input image loaded"))?;

        let output = WgpuOutputView::new_from_raw(output_view, output_size, output_format);

        // Create viewport with custom origin
        let mut viewport = Viewport::new_render_target_sized_origin(output, None)?;
        viewport.x = viewport_x as f32;
        viewport.y = viewport_y as f32;

        // Calculate virtual frame count at 30 Hz for consistent shader animation
        // This ensures effects like dot crawl run at 30 Hz regardless of actual framerate
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let virtual_frame_count = (elapsed * 41.0) as usize;

        filter_chain.frame(
            input_texture,
            &viewport,
            encoder,
            virtual_frame_count,
            Some(&FrameOptions::default()),
        )?;

        self.frame_count += 1;
        Ok(())
    }

    /// Get current frame count
    pub fn frame_count(&self) -> usize {
        self.frame_count
    }

    /// Reset frame count
    pub fn reset_frame_count(&mut self) {
        self.frame_count = 0;
    }

    /// Reset the animation start time
    ///
    /// This resets the time-based frame counter used for shader animations,
    /// causing effects like dot crawl to restart from the beginning.
    pub fn reset_animation_time(&mut self) {
        self.start_time = Instant::now();
    }

    /// Calculate viewport coordinates for aspect-ratio-preserving rendering
    ///
    /// This calculates the viewport region within a window/surface that maintains
    /// the content's aspect ratio, adding letterboxing (horizontal bars) or
    /// pillarboxing (vertical bars) as needed.
    ///
    /// # Arguments
    /// * `window_width`, `window_height` - The dimensions of the target surface
    /// * `content_width`, `content_height` - The dimensions of the content to display
    ///
    /// # Returns
    /// A tuple of (x, y, width, height) representing the viewport region
    pub fn calculate_aspect_preserving_viewport(
        window_width: u32,
        window_height: u32,
        content_width: u32,
        content_height: u32,
    ) -> (u32, u32, u32, u32) {
        let window_aspect = window_width as f32 / window_height as f32;
        let content_aspect = content_width as f32 / content_height as f32;

        if window_aspect > content_aspect {
            // Window is wider than content - pillarbox (add vertical bars)
            let scaled_width = window_height as f32 * content_aspect;
            let x_offset = (window_width as f32 - scaled_width) / 2.0;
            (x_offset as u32, 0, scaled_width as u32, window_height)
        } else {
            // Window is taller than content - letterbox (add horizontal bars)
            let scaled_height = window_width as f32 / content_aspect;
            let y_offset = (window_height as f32 - scaled_height) / 2.0;
            (0, y_offset as u32, window_width, scaled_height as u32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device_and_queue() -> (Arc<wgpu::Device>, Arc<wgpu::Queue>) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("Failed to find an appropriate adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: Default::default(),
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        }))
        .expect("Failed to create device");

        (Arc::new(device), Arc::new(queue))
    }

    #[cfg(feature = "embedded-shaders")]
    #[test]
    fn test_load_embedded_preset() {
        let (device, queue) = create_test_device_and_queue();
        let mut renderer = ShaderRenderer::new(device, queue);

        renderer
            .load_preset_embedded()
            .expect("Failed to load embedded preset");
        assert!(
            renderer.filter_chain.is_some(),
            "Filter chain should be loaded"
        );
    }

    #[test]
    fn test_load_default_preset() {
        let (device, queue) = create_test_device_and_queue();
        let mut renderer = ShaderRenderer::new(device, queue);

        renderer
            .load_default_preset()
            .expect("Failed to load default preset");
        assert!(
            renderer.filter_chain.is_some(),
            "Filter chain should be loaded"
        );
    }

    #[test]
    #[should_panic(expected = "Invalid PresentationRequest: Compositor bug - expected 16 bytes, got 8")]
    fn test_load_presentation_panics_on_mismatched_pixel_data_length() {
        let (device, queue) = create_test_device_and_queue();
        let mut renderer = ShaderRenderer::new(device, queue);

        // Create PresentationRequest with pixel_data length that doesn't match width * height * 4
        // Expected: 2 * 2 * 4 = 16 bytes, but we provide 8 bytes
        let request = PresentationRequest::new(vec![0; 8], 2, 2, 0);

        renderer.load_presentation(&request);
    }

    #[test]
    #[should_panic(expected = "Invalid PresentationRequest: Compositor bug - zero-sized dimensions")]
    fn test_load_presentation_panics_on_zero_width() {
        let (device, queue) = create_test_device_and_queue();
        let mut renderer = ShaderRenderer::new(device, queue);

        // Create PresentationRequest with zero width
        let request = PresentationRequest::new(vec![], 0, 2, 0);

        renderer.load_presentation(&request);
    }

    #[test]
    #[should_panic(expected = "Invalid PresentationRequest: Compositor bug - zero-sized dimensions")]
    fn test_load_presentation_panics_on_zero_height() {
        let (device, queue) = create_test_device_and_queue();
        let mut renderer = ShaderRenderer::new(device, queue);

        // Create PresentationRequest with zero height
        let request = PresentationRequest::new(vec![], 2, 0, 0);

        renderer.load_presentation(&request);
    }

    #[test]
    fn test_aspect_preserving_viewport_pillarbox() {
        // Wide window (16:9), narrow content (4:3) -> pillarbox
        let (x, y, w, h) =
            ShaderRenderer::calculate_aspect_preserving_viewport(1600, 900, 320, 240);

        assert_eq!(y, 0, "Pillarbox should not offset vertically");
        assert_eq!(h, 900, "Pillarbox should use full height");
        assert!(x > 0, "Pillarbox should offset horizontally");
        assert!(w < 1600, "Pillarbox width should be less than window width");

        // Check aspect ratio is preserved
        let content_aspect = 320.0 / 240.0;
        let viewport_aspect = w as f32 / h as f32;
        assert!(
            (content_aspect - viewport_aspect).abs() < 0.01,
            "Aspect ratio should be preserved"
        );
    }

    #[test]
    fn test_aspect_preserving_viewport_letterbox() {
        // Tall window (4:3), wide content (16:9) -> letterbox
        let (x, y, w, h) =
            ShaderRenderer::calculate_aspect_preserving_viewport(800, 600, 1920, 1080);

        assert_eq!(x, 0, "Letterbox should not offset horizontally");
        assert_eq!(w, 800, "Letterbox should use full width");
        assert!(y > 0, "Letterbox should offset vertically");
        assert!(h < 600, "Letterbox height should be less than window height");

        // Check aspect ratio is preserved
        let content_aspect = 1920.0 / 1080.0;
        let viewport_aspect = w as f32 / h as f32;
        assert!(
            (content_aspect - viewport_aspect).abs() < 0.01,
            "Aspect ratio should be preserved"
        );
    }

    #[test]
    fn test_aspect_preserving_viewport_exact_match() {
        // Window and content have same aspect ratio
        let (x, y, w, h) =
            ShaderRenderer::calculate_aspect_preserving_viewport(1920, 1080, 320, 180);

        assert_eq!(x, 0, "No offset needed when aspects match");
        assert_eq!(y, 0, "No offset needed when aspects match");
        assert_eq!(w, 1920, "Should use full width");
        assert_eq!(h, 1080, "Should use full height");
    }
}
