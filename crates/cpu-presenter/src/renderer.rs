use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Result};
use engine_core::PresentationRequest;

use crate::blit::BlitPipeline;
use crate::composite::{params::CompositeParams, CompositeProcessor};
use crate::Size;

pub struct ShaderRenderer {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    composite: CompositeProcessor,
    blit: BlitPipeline,
    input_size: Size<u32>,
    start_time: Instant,
    frame_count: usize,
}

impl ShaderRenderer {
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        let params = CompositeParams::default();
        let composite = CompositeProcessor::new(params, 640, 480);
        let blit = BlitPipeline::new(&device);
        Self {
            device,
            queue,
            composite,
            blit,
            input_size: Size::new(0, 0),
            start_time: Instant::now(),
            frame_count: 0,
        }
    }

    pub fn load_default_preset(&mut self) -> Result<()> {
        Ok(())
    }

    pub fn load_presentation(&mut self, request: &PresentationRequest) {
        let expected = (request.width as usize) * (request.height as usize) * 4;
        assert!(
            request.is_valid(),
            "Invalid PresentationRequest: expected {} bytes, got {}",
            expected,
            request.pixel_data.len()
        );
        assert!(
            request.width > 0 && request.height > 0,
            "Invalid PresentationRequest: zero-sized dimensions ({}x{})",
            request.width,
            request.height
        );

        let (rgba, ow, oh) = self.composite.process(
            &request.pixel_data,
            request.width as usize,
            request.height as usize,
        );
        self.blit.upload(&self.queue, rgba, ow, oh);
        self.input_size = Size::new(request.width, request.height);
    }

    pub fn has_input(&self) -> bool {
        self.blit.has_input()
    }

    pub fn input_size(&self) -> Size<u32> {
        self.input_size
    }

    pub fn render_frame(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        output_size: Size<u32>,
        output_format: wgpu::TextureFormat,
    ) -> Result<()> {
        self.render_frame_to_viewport(encoder, output_view, output_size, output_format, 0, 0)
    }

    pub fn render_frame_to_viewport(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        output_size: Size<u32>,
        output_format: wgpu::TextureFormat,
        viewport_x: u32,
        viewport_y: u32,
    ) -> Result<()> {
        if !self.has_input() {
            return Err(anyhow!("No input image loaded"));
        }

        self.blit.render(
            encoder,
            output_view,
            (output_size.width, output_size.height),
            output_format,
            viewport_x,
            viewport_y,
        )?;
        self.frame_count += 1;
        Ok(())
    }

    pub fn frame_count(&self) -> usize {
        self.frame_count
    }

    pub fn reset_frame_count(&mut self) {
        self.frame_count = 0;
    }

    pub fn reset_animation_time(&mut self) {
        self.start_time = Instant::now();
    }

    pub fn calculate_aspect_preserving_viewport(
        window_width: u32,
        window_height: u32,
        content_width: u32,
        content_height: u32,
    ) -> (u32, u32, u32, u32) {
        let window_aspect = window_width as f32 / window_height as f32;
        let content_aspect = content_width as f32 / content_height as f32;

        if window_aspect > content_aspect {
            let scaled_width = window_height as f32 * content_aspect;
            let x_offset = (window_width as f32 - scaled_width) / 2.0;
            (x_offset as u32, 0, scaled_width as u32, window_height)
        } else {
            let scaled_height = window_width as f32 / content_aspect;
            let y_offset = (window_height as f32 - scaled_height) / 2.0;
            (0, y_offset as u32, window_width, scaled_height as u32)
        }
    }
}
