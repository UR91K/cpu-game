//! CPU-based NTSC presenter compatible with the existing ShaderRenderer API.
//! Reference shader logic: ../../full-shader.md

pub mod blit;
pub mod composite;
pub mod renderer;

pub use renderer::ShaderRenderer;
