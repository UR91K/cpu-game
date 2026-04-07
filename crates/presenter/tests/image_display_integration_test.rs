//! Integration tests for image_display example refactoring.
//!
//! Verifies that the refactored image_display example maintains functionality.

use engine_core::test_helpers::image_to_presentation;

#[test]
fn test_image_loading_pattern() {
    // Create a simple test image
    let img = image::ImageBuffer::from_pixel(320, 240, image::Rgba([255, 0, 0, 255]));

    // Convert to PresentationRequest using the new pattern
    let presentation_request = image_to_presentation(&img, 0);

    // Verify the conversion worked
    assert_eq!(presentation_request.width, 320);
    assert_eq!(presentation_request.height, 240);
    assert_eq!(presentation_request.frame_number, 0);
    assert!(presentation_request.is_valid());

    // Verify pixel data length matches expectations (RGBA = 4 bytes per pixel)
    let expected_pixel_count = 320 * 240 * 4;
    assert_eq!(presentation_request.pixel_data.len(), expected_pixel_count);
}

#[test]
fn test_viewport_calculation_with_image_dimensions() {
    // Test that viewport calculation works with typical image dimensions
    // This simulates the new render method logic

    let window_width = 800;
    let window_height = 600;
    let content_width = 320;
    let content_height = 240;

    // Calculate aspect-preserving viewport (same logic as ShaderRenderer)
    let window_aspect = window_width as f32 / window_height as f32;
    let content_aspect = content_width as f32 / content_height as f32;

    let (viewport_width, viewport_height) = if window_aspect > content_aspect {
        // Window is wider - pillarbox
        let height = window_height;
        let width = (window_height as f32 * content_aspect) as u32;
        (width, height)
    } else {
        // Window is taller - letterbox
        let width = window_width;
        let height = (window_width as f32 / content_aspect) as u32;
        (width, height)
    };

    // Verify viewport fits within window
    assert!(viewport_width <= window_width);
    assert!(viewport_height <= window_height);

    // Verify aspect ratio is approximately preserved
    let calculated_aspect = viewport_width as f32 / viewport_height as f32;
    let tolerance = 0.01;
    assert!((calculated_aspect - content_aspect).abs() < tolerance);
}
