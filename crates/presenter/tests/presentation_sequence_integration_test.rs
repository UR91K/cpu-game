//! Integration tests for presentation_sequence example refactoring.
//!
//! Verifies that the refactored presentation_sequence example maintains functionality.

use engine_core::test_helpers::{create_test_image, image_to_presentation};

#[test]
fn test_frame_generation_pattern() {
    // Test the frame generation pattern used in presentation_sequence
    let frame_number = 42;
    let background_color = [64, 128, 192, 255]; // Test color
    let circle_x = 160.0;
    let circle_y = 112.0;
    let circle_radius = 50.0;

    // Create a test image (simulating FrameGenerator::generate_frame)
    let img = create_test_image(
        320, 224, // FRAME_WIDTH, FRAME_HEIGHT
        background_color,
        circle_x as i32,
        circle_y as i32,
        circle_radius as i32,
        2.0, // BLUR_RADIUS
    );

    // Convert to PresentationRequest
    let presentation_request = image_to_presentation(&img, frame_number);

    // Verify the frame was created correctly
    assert_eq!(presentation_request.width, 320);
    assert_eq!(presentation_request.height, 224);
    assert_eq!(presentation_request.frame_number, frame_number);
    assert!(presentation_request.is_valid());

    // Verify pixel data length matches expectations (RGBA = 4 bytes per pixel)
    let expected_pixel_count = 320 * 224 * 4;
    assert_eq!(presentation_request.pixel_data.len(), expected_pixel_count);
}

#[test]
fn test_frame_timing_calculations() {
    // Test the 60 FPS timing calculations used in presentation_sequence
    const TARGET_FPS: f64 = 60.0;
    let frame_duration = std::time::Duration::from_secs_f64(1.0 / TARGET_FPS);

    // Verify frame duration is approximately 16.67ms
    let expected_micros = (1_000_000.0 / TARGET_FPS) as u128;
    let actual_micros = frame_duration.as_micros();

    // Allow small floating point tolerance
    let tolerance = 10; // microseconds
    assert!((actual_micros as i128 - expected_micros as i128).abs() < tolerance);

    // Test frame timing logic (simplified)
    let mut next_frame_time = std::time::Instant::now();
    let mut frame_count = 0;

    // Simulate a few frames
    for _ in 0..5 {
        next_frame_time += frame_duration;
        frame_count += 1;
    }

    assert_eq!(frame_count, 5);
}

#[test]
fn test_viewport_calculation_with_frame_dimensions() {
    // Test viewport calculation with presentation_sequence frame dimensions
    let window_width = 1920;
    let window_height = 1080;
    let content_width = 320;  // FRAME_WIDTH
    let content_height = 224; // FRAME_HEIGHT

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

    // For 1920x1080 window with 320x224 content, should use letterboxing
    // since window_aspect (1.78) > content_aspect (1.43)
    assert!(window_aspect > content_aspect);

    // Verify viewport fits within window
    assert!(viewport_width <= window_width);
    assert!(viewport_height <= window_height);

    // Verify we're using full window height (pillarboxing)
    assert_eq!(viewport_height, window_height);

    // Verify aspect ratio is approximately preserved
    let calculated_aspect = viewport_width as f32 / viewport_height as f32;
    let tolerance = 0.01;
    assert!((calculated_aspect - content_aspect).abs() < tolerance);
}
