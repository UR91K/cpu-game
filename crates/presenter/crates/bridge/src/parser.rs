//! Frame message parsing for Aseprite Presenter Bridge
//!
//! This module handles parsing binary frame messages sent from Aseprite Lua scripts.

use anyhow::{anyhow, Result};
use engine_core::PresentationRequest;

/// Color mode enumeration for Aseprite frame data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// RGBA color mode (4 bytes per pixel)
    Rgb = 0,
    /// Grayscale with alpha (2 bytes per pixel)
    Grayscale = 1,
    /// Indexed color mode (1 byte per pixel)
    Indexed = 2,
}

impl ColorMode {
    /// Convert from u8 value to ColorMode enum
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(ColorMode::Rgb),
            1 => Ok(ColorMode::Grayscale),
            2 => Ok(ColorMode::Indexed),
            _ => Err(anyhow!("Invalid color mode: {}", value)),
        }
    }

    /// Get bytes per pixel for this color mode
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            ColorMode::Rgb => 4,
            ColorMode::Grayscale => 2,
            ColorMode::Indexed => 1,
        }
    }
}

/// Frame header structure for binary frame messages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    /// Width of the frame in pixels
    pub width: u32,
    /// Height of the frame in pixels
    pub height: u32,
    /// Color mode (0=RGB, 1=Grayscale, 2=Indexed)
    pub color_mode: u8,
}

impl FrameHeader {
    /// Get the color mode as an enum
    pub fn color_mode(&self) -> Result<ColorMode> {
        ColorMode::from_u8(self.color_mode)
    }

    /// Calculate expected pixel data size in bytes
    pub fn expected_pixel_data_size(&self) -> Result<usize> {
        let color_mode = self.color_mode()?;
        Ok(self.width as usize * self.height as usize * color_mode.bytes_per_pixel())
    }
}

/// Parse frame header from binary data
///
/// Expects exactly 9 bytes of data in little-endian format:
/// - Bytes 0-3: width (u32)
/// - Bytes 4-7: height (u32)
/// - Byte 8: color_mode (u8)
pub fn parse_frame_header(data: &[u8]) -> Result<FrameHeader> {
    if data.len() < 9 {
        return Err(anyhow!("Frame header too short: expected 9 bytes, got {}", data.len()));
    }

    // Parse width (little-endian u32)
    let width = u32::from_le_bytes(data[0..4].try_into().unwrap());

    // Parse height (little-endian u32)
    let height = u32::from_le_bytes(data[4..8].try_into().unwrap());

    // Parse color mode
    let color_mode_value = data[8];

    // Validate color mode
    ColorMode::from_u8(color_mode_value)?;

    Ok(FrameHeader {
        width,
        height,
        color_mode: color_mode_value,
    })
}

/// Validate that the frame data size matches the expected size for the header
pub fn validate_frame_size(header: &FrameHeader, total_data_len: usize) -> Result<()> {
    let expected_pixel_data_size = header.expected_pixel_data_size()?;
    let expected_total_size = 9 + expected_pixel_data_size;

    if total_data_len != expected_total_size {
        return Err(anyhow!(
            "Frame data size mismatch: expected {} bytes (9 header + {} pixel), got {} bytes",
            expected_total_size,
            expected_pixel_data_size,
            total_data_len
        ));
    }

    Ok(())
}

/// Convert RGBA pixel data to PresentationRequest format
///
/// Aseprite RGB mode is already in RGBA format (4 bytes per pixel),
/// so this function simply clones the data.
pub fn convert_rgba(data: &[u8]) -> Result<Vec<u8>> {
    // Aseprite RGB mode is already RGBA (4 bytes per pixel)
    // Just clone the data
    Ok(data.to_vec())
}

/// Convert grayscale pixel data to RGBA format
///
/// Aseprite grayscale mode uses 2 bytes per pixel: (Value, Alpha)
/// This function expands it to RGBA format: (Value, Value, Value, Alpha)
pub fn convert_grayscale(data: &[u8]) -> Result<Vec<u8>> {
    // Aseprite grayscale is 2 bytes per pixel: (Value, Alpha)
    // Expand to RGBA: (Value, Value, Value, Alpha)
    let mut rgba = Vec::with_capacity(data.len() * 2);
    for chunk in data.chunks_exact(2) {
        let value = chunk[0];
        let alpha = chunk[1];
        rgba.extend_from_slice(&[value, value, value, alpha]);
    }
    Ok(rgba)
}

/// Convert indexed color pixel data to RGBA format
///
/// Aseprite indexed mode uses 1 byte per pixel (palette index).
/// Without palette data, we can't convert properly, so we treat
/// the index as grayscale brightness for now.
pub fn convert_indexed(data: &[u8]) -> Result<Vec<u8>> {
    // Aseprite indexed mode is 1 byte per pixel (palette index)
    // Without palette data, we can't convert properly
    // For now, treat as grayscale (index as brightness)
    let mut rgba = Vec::with_capacity(data.len() * 4);
    for &index in data {
        rgba.extend_from_slice(&[index, index, index, 255]);
    }
    Ok(rgba)
}

/// Convert frame data to PresentationRequest based on color mode
///
/// This function takes the parsed frame data and converts it to the
/// format required by the PresentationRequest, which expects RGBA pixel data.
pub fn to_presentation_request(
    width: u32,
    height: u32,
    color_mode: ColorMode,
    pixel_data: &[u8],
    frame_number: u64,
) -> Result<PresentationRequest> {
    // Convert pixel data based on color mode
    let rgba_data = match color_mode {
        ColorMode::Rgb => convert_rgba(pixel_data)?,
        ColorMode::Grayscale => convert_grayscale(pixel_data)?,
        ColorMode::Indexed => convert_indexed(pixel_data)?,
    };

    // Create PresentationRequest
    let request = PresentationRequest::new(rgba_data, width, height, frame_number);

    // Validate the request
    if !request.is_valid() {
        return Err(anyhow!("Invalid PresentationRequest: pixel data size mismatch"));
    }

    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Property 1: Message Encoding Round-Trip
        /// Validates: Requirements 3.1, 3.2, 3.3
        #[test]
        fn test_header_parsing_round_trip(width in 1u32..65536, height in 1u32..65536, color_mode in 0u8..3) {
            // Create a frame header
            let original = FrameHeader { width, height, color_mode };

            // Encode to bytes
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&width.to_le_bytes());
            bytes.extend_from_slice(&height.to_le_bytes());
            bytes.push(color_mode);

            // Parse back
            let parsed = parse_frame_header(&bytes).unwrap();

            // Verify equality
            prop_assert_eq!(parsed.width, original.width);
            prop_assert_eq!(parsed.height, original.height);
            prop_assert_eq!(parsed.color_mode, original.color_mode);
        }

        /// Property 2: Message Size Consistency
        /// Validates: Requirements 3.6
        #[test]
        fn test_size_calculation_consistency(width in 1u32..1000, height in 1u32..1000, color_mode in 0u8..3) {
            let header = FrameHeader { width, height, color_mode };
            let color_mode_enum = ColorMode::from_u8(color_mode).unwrap();
            let expected_size = width as usize * height as usize * color_mode_enum.bytes_per_pixel();

            prop_assert_eq!(header.expected_pixel_data_size().unwrap(), expected_size);
        }

        /// Property 3: Pixel Data Boundary
        /// Validates: Requirements 3.4
        #[test]
        fn test_pixel_data_boundary(width in 1u32..100, height in 1u32..100, color_mode in 0u8..3) {
            let header = FrameHeader { width, height, color_mode };
            let pixel_data_size = header.expected_pixel_data_size().unwrap();
            let total_size = 9 + pixel_data_size;

            // Create a frame with pixel data
            let mut frame_data = Vec::new();
            frame_data.extend_from_slice(&width.to_le_bytes());
            frame_data.extend_from_slice(&height.to_le_bytes());
            frame_data.push(color_mode);

            // Add pixel data (dummy data)
            frame_data.resize(total_size, 0);

            // Validate size
            validate_frame_size(&header, frame_data.len()).unwrap();

            // Pixel data should start at byte 9
            prop_assert_eq!(frame_data.len(), total_size);
            prop_assert_eq!(&frame_data[9..], &vec![0u8; pixel_data_size]);
        }

        /// Property 13: RGBA Conversion Identity
        /// Validates: Requirements 4.3
        /// For any frame data in RGB color mode with dimensions W×H, converting to PresentationRequest
        /// should produce pixel_data of length W×H×4 with identical pixel values.
        #[test]
        fn test_rgba_conversion_identity(
            width in 1u32..100,
            height in 1u32..100,
            pixel_values in prop::collection::vec(0u8..=255, 0..10000)
        ) {
            // Only test if we have enough data for the frame dimensions
            let expected_size = (width * height) as usize * 4;
            if pixel_values.len() < expected_size {
                return Ok(());
            }

            let pixel_data = &pixel_values[..expected_size];
            let frame_number = 42u64;

            // Convert to PresentationRequest
            let result = to_presentation_request(width, height, ColorMode::Rgb, pixel_data, frame_number);
            prop_assert!(result.is_ok());

            let request = result.unwrap();

            // Verify dimensions are preserved
            prop_assert_eq!(request.width, width);
            prop_assert_eq!(request.height, height);
            prop_assert_eq!(request.frame_number, frame_number);

            // Verify pixel values are unchanged (identity)
            prop_assert_eq!(request.pixel_data.len(), expected_size);
            prop_assert_eq!(&request.pixel_data[..], pixel_data);

            // Verify request is valid
            prop_assert!(request.is_valid());
        }

        /// Property 14: Grayscale to RGBA Expansion
        /// Validates: Requirements 4.3
        /// For any frame data in Grayscale color mode with dimensions W×H, converting to PresentationRequest
        /// should expand each (Value, Alpha) pair to (Value, Value, Value, Alpha) and produce pixel_data
        /// of length W×H×4.
        #[test]
        fn test_grayscale_expansion(
            width in 1u32..100,
            height in 1u32..100,
            pixel_values in prop::collection::vec(0u8..=255, 0..20000)
        ) {
            // Only test if we have enough data for the frame dimensions
            let expected_input_size = (width * height) as usize * 2;
            if pixel_values.len() < expected_input_size {
                return Ok(());
            }

            let pixel_data = &pixel_values[..expected_input_size];
            let frame_number = 42u64;

            // Convert to PresentationRequest
            let result = to_presentation_request(width, height, ColorMode::Grayscale, pixel_data, frame_number);
            prop_assert!(result.is_ok());

            let request = result.unwrap();

            // Verify dimensions are preserved
            prop_assert_eq!(request.width, width);
            prop_assert_eq!(request.height, height);
            prop_assert_eq!(request.frame_number, frame_number);

            // Verify pixel data is expanded correctly
            let expected_output_size = (width * height) as usize * 4;
            prop_assert_eq!(request.pixel_data.len(), expected_output_size);

            // Verify each (Value, Alpha) pair is expanded to (Value, Value, Value, Alpha)
            for chunk_idx in 0..((width * height) as usize) {
                let input_idx = chunk_idx * 2;
                let output_idx = chunk_idx * 4;

                let value = pixel_data[input_idx];
                let alpha = pixel_data[input_idx + 1];

                prop_assert_eq!(request.pixel_data[output_idx], value, "R component mismatch");
                prop_assert_eq!(request.pixel_data[output_idx + 1], value, "G component mismatch");
                prop_assert_eq!(request.pixel_data[output_idx + 2], value, "B component mismatch");
                prop_assert_eq!(request.pixel_data[output_idx + 3], alpha, "A component mismatch");
            }

            // Verify request is valid
            prop_assert!(request.is_valid());
        }

        /// Property 15: PresentationRequest Validation
        /// Validates: Requirements 4.2
        /// For any PresentationRequest created by the bridge, calling is_valid() should return true
        /// (pixel_data.len() == width × height × 4).
        #[test]
        fn test_presentation_request_validation(
            width in 1u32..100,
            height in 1u32..100,
            color_mode in 0u8..3,
            pixel_values in prop::collection::vec(0u8..=255, 0..50000)
        ) {
            // Determine the expected pixel data size based on color mode
            let color_mode_enum = match ColorMode::from_u8(color_mode) {
                Ok(cm) => cm,
                Err(_) => return Ok(()), // Skip invalid color modes
            };

            let expected_input_size = (width as usize) * (height as usize) * color_mode_enum.bytes_per_pixel();

            // Only test if we have enough data
            if pixel_values.len() < expected_input_size {
                return Ok(());
            }

            let pixel_data = &pixel_values[..expected_input_size];
            let frame_number = 42u64;

            // Try to convert to PresentationRequest
            let result = to_presentation_request(width, height, color_mode_enum, pixel_data, frame_number);

            // The conversion should succeed
            prop_assert!(result.is_ok(), "Conversion failed: {:?}", result.err());

            let request = result.unwrap();

            // The resulting PresentationRequest should be valid
            prop_assert!(request.is_valid(), "PresentationRequest is invalid");

            // Verify the pixel_data length matches the formula: width × height × 4
            let expected_rgba_size = (width as usize) * (height as usize) * 4;
            prop_assert_eq!(request.pixel_data.len(), expected_rgba_size);
        }
    }

    #[test]
    fn test_color_mode_from_u8() {
        assert_eq!(ColorMode::from_u8(0).unwrap(), ColorMode::Rgb);
        assert_eq!(ColorMode::from_u8(1).unwrap(), ColorMode::Grayscale);
        assert_eq!(ColorMode::from_u8(2).unwrap(), ColorMode::Indexed);
        assert!(ColorMode::from_u8(3).is_err());
        assert!(ColorMode::from_u8(255).is_err());
    }

    #[test]
    fn test_color_mode_bytes_per_pixel() {
        assert_eq!(ColorMode::Rgb.bytes_per_pixel(), 4);
        assert_eq!(ColorMode::Grayscale.bytes_per_pixel(), 2);
        assert_eq!(ColorMode::Indexed.bytes_per_pixel(), 1);
    }

    #[test]
    fn test_parse_frame_header_valid() {
        let mut data = Vec::new();
        data.extend_from_slice(&100u32.to_le_bytes()); // width
        data.extend_from_slice(&200u32.to_le_bytes()); // height
        data.push(1); // color_mode (grayscale)

        let header = parse_frame_header(&data).unwrap();
        assert_eq!(header.width, 100);
        assert_eq!(header.height, 200);
        assert_eq!(header.color_mode, 1);
        assert_eq!(header.color_mode().unwrap(), ColorMode::Grayscale);
    }

    #[test]
    fn test_parse_frame_header_too_short() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8]; // only 8 bytes
        assert!(parse_frame_header(&data).is_err());
    }

    #[test]
    fn test_parse_frame_header_invalid_color_mode() {
        let mut data = Vec::new();
        data.extend_from_slice(&100u32.to_le_bytes());
        data.extend_from_slice(&200u32.to_le_bytes());
        data.push(5); // invalid color mode

        assert!(parse_frame_header(&data).is_err());
    }

    #[test]
    fn test_validate_frame_size_valid() {
        let header = FrameHeader { width: 10, height: 10, color_mode: 0 }; // RGB
        let total_size = 9 + (10 * 10 * 4); // header + pixel data
        assert!(validate_frame_size(&header, total_size).is_ok());
    }

    #[test]
    fn test_validate_frame_size_invalid() {
        let header = FrameHeader { width: 10, height: 10, color_mode: 0 }; // RGB
        let total_size = 9 + (10 * 10 * 2); // wrong size
        assert!(validate_frame_size(&header, total_size).is_err());
    }

    #[test]
    fn test_convert_rgba() {
        let input = vec![255, 0, 0, 255, 0, 255, 0, 255]; // Two RGBA pixels: red and green
        let result = convert_rgba(&input).unwrap();
        assert_eq!(result, input); // Should be identical
    }

    #[test]
    fn test_convert_grayscale() {
        let input = vec![128, 255, 64, 128]; // Two grayscale pixels: (128,255) and (64,128)
        let result = convert_grayscale(&input).unwrap();
        let expected = vec![128, 128, 128, 255, 64, 64, 64, 128]; // Expanded to RGBA
        assert_eq!(result, expected);
    }

    #[test]
    fn test_convert_indexed() {
        let input = vec![0, 128, 255]; // Three indexed pixels
        let result = convert_indexed(&input).unwrap();
        let expected = vec![
            0, 0, 0, 255,      // index 0 -> black
            128, 128, 128, 255, // index 128 -> gray
            255, 255, 255, 255  // index 255 -> white
        ];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_to_presentation_request_rgba() {
        let width = 2u32;
        let height = 1u32;
        let pixel_data = vec![255, 0, 0, 255, 0, 255, 0, 255]; // Two RGBA pixels
        let frame_number = 42u64;

        let result = to_presentation_request(width, height, ColorMode::Rgb, &pixel_data, frame_number).unwrap();

        assert_eq!(result.width, width);
        assert_eq!(result.height, height);
        assert_eq!(result.frame_number, frame_number);
        assert_eq!(result.pixel_data, pixel_data);
        assert!(result.is_valid());
    }

    #[test]
    fn test_to_presentation_request_grayscale() {
        let width = 2u32;
        let height = 1u32;
        let pixel_data = vec![128, 255, 64, 128]; // Two grayscale pixels
        let frame_number = 42u64;

        let result = to_presentation_request(width, height, ColorMode::Grayscale, &pixel_data, frame_number).unwrap();

        assert_eq!(result.width, width);
        assert_eq!(result.height, height);
        assert_eq!(result.frame_number, frame_number);
        let expected_rgba = vec![128, 128, 128, 255, 64, 64, 64, 128];
        assert_eq!(result.pixel_data, expected_rgba);
        assert!(result.is_valid());
    }

    #[test]
    fn test_to_presentation_request_indexed() {
        let width = 2u32;
        let height = 1u32;
        let pixel_data = vec![0, 255]; // Two indexed pixels
        let frame_number = 42u64;

        let result = to_presentation_request(width, height, ColorMode::Indexed, &pixel_data, frame_number).unwrap();

        assert_eq!(result.width, width);
        assert_eq!(result.height, height);
        assert_eq!(result.frame_number, frame_number);
        let expected_rgba = vec![0, 0, 0, 255, 255, 255, 255, 255];
        assert_eq!(result.pixel_data, expected_rgba);
        assert!(result.is_valid());
    }
}
