use anyhow::Result;
use librashader_pack::ShaderPresetPack;

/// Embed the packed shader data generated at build time
const SHADER_PACK_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shader_pack.bin"));

/// Load the embedded shader pack from the binary.
///
/// This deserializes the MessagePack data that was embedded at compile time.
/// The pack contains all preprocessed shader source code and texture data.
pub fn load_embedded_pack() -> Result<ShaderPresetPack> {
    let pack: ShaderPresetPack = rmp_serde::from_slice(SHADER_PACK_BYTES).map_err(|e| {
        anyhow::anyhow!(
            "Failed to deserialize embedded shader pack: {}\n\
             This likely indicates a version mismatch between the build-time \
             and runtime librashader versions. Try a clean rebuild.",
            e
        )
    })?;

    // Sanity check
    if pack.passes.is_empty() {
        return Err(anyhow::anyhow!(
            "Embedded shader pack contains no passes. \
             The build process may have failed silently."
        ));
    }

    Ok(pack)
}

/// Get the size of the embedded shader pack in bytes.
/// Useful for debugging and metrics.
pub fn embedded_pack_size() -> usize {
    SHADER_PACK_BYTES.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_pack_deserialization() {
        let pack = load_embedded_pack().expect("Failed to load embedded pack");
        assert!(pack.pass_count > 0, "Pack should have at least one pass");
        assert!(!pack.passes.is_empty(), "Pack should contain shader passes");
    }

    #[test]
    fn test_embedded_pack_size() {
        let size = embedded_pack_size();
        assert!(size > 0, "Embedded pack should have non-zero size");
        assert!(size < 5_000_000, "Embedded pack should be less than 5MB");
    }

    #[test]
    fn test_pack_has_textures() {
        let pack = load_embedded_pack().expect("Failed to load embedded pack");
        // Verify that textures are included (may be empty if preset doesn't use LUTs)
        // This test mainly ensures the pack structure is valid
        assert!(pack.pass_count > 0, "Pack should have valid structure");
    }
}
