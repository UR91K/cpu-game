//! WebSocket bridge for Aseprite Presenter integration
//!
//! This crate provides WebSocket server functionality to receive frame data
//! from Aseprite Lua scripts and convert it to PresentationRequest objects
//! for the presenter application.

pub mod server;
pub mod parser;

// Re-export key types for convenience
pub use parser::{
    ColorMode, FrameHeader, parse_frame_header, validate_frame_size,
    convert_rgba, convert_grayscale, convert_indexed, to_presentation_request
};
pub use server::BridgeServer;