//! Shader test library - wgpu + librashader integration

pub mod renderer;

#[cfg(feature = "embedded-shaders")]
pub mod embedded_shaders;

pub use renderer::ShaderRenderer;
