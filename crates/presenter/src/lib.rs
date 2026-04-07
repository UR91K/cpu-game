//! Shader test library - wgpu + librashader integration

pub mod renderer;

#[cfg(feature = "embedded-shaders")]
pub mod embedded_shaders;

// Shared utilities for examples
pub mod examples_common;

pub use renderer::ShaderRenderer;
