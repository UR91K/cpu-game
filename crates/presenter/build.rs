use anyhow::Result;
use librashader::presets::{ShaderFeatures, ShaderPreset};
use librashader_pack::ShaderPresetPack;
use std::env;
use std::fs;
use std::path::Path;

fn main() -> Result<()> {
    // Declare dependencies for cargo rebuild tracking
    println!("cargo:rerun-if-changed=shaders");
    println!("cargo:rerun-if-changed=include");

    // Load and pack the preset
    let preset_path = "shaders/ntsc-composite.slangp";
    let pack = create_shader_pack(preset_path)?;

    // Serialize to MessagePack
    let packed_bytes = rmp_serde::to_vec(&pack)?;

    // Write to output directory
    let out_dir = env::var("OUT_DIR")?;
    let dest_path = Path::new(&out_dir).join("shader_pack.bin");
    fs::write(&dest_path, &packed_bytes)?;

    println!(
        "cargo:warning=Generated shader pack: {} bytes",
        packed_bytes.len()
    );

    Ok(())
}

fn create_shader_pack(preset_path: &str) -> Result<ShaderPresetPack> {
    // Parse the preset, resolving all paths relative to the preset location
    let preset = ShaderPreset::try_parse(preset_path, ShaderFeatures::empty()).map_err(|e| {
        anyhow::anyhow!(
            "Failed to parse shader preset '{}': {}\n\
             Ensure the preset file exists and all referenced shaders are present.",
            preset_path,
            e
        )
    })?;

    // Load all shaders and textures into memory
    // This resolves all #includes and loads all texture files
    let pack = ShaderPresetPack::load_from_preset::<
        librashader::runtime::wgpu::error::FilterChainError,
    >(preset)
    .map_err(|e| {
        anyhow::anyhow!(
            "Failed to load shader resources: {}\n\
             Check that all shader files and textures referenced by the preset exist.",
            e
        )
    })?;

    Ok(pack)
}
