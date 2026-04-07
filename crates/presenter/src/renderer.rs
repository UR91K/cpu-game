//! Core renderer using wgpu and librashader

use anyhow::{anyhow, Result};
use engine_core::PresentationRequest;
use librashader::runtime::wgpu::{FilterChain, FilterChainOptions, FrameOptions, WgpuOutputView};
use librashader::runtime::{Size, Viewport};
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

    /// Load the default embedded shader preset.
    pub fn load_default_preset(&mut self) -> Result<()> {
        self.load_preset_embedded()
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
