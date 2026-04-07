//! Integration tests for resolution_test example refactoring.
//!
//! Verifies that the refactored resolution_test example maintains functionality.

use engine_core::test_helpers::{create_test_image, image_to_presentation};

const TEST_RESOLUTIONS: &[(u32, u32)] = &[
    (320, 224),   // Classic console (SNES, Genesis)
    (640, 480),   // VGA resolution
    (800, 600),   // SVGA resolution
    (1024, 768),  // XGA resolution
    (1280, 720),  // HD 720p
    (1920, 1080), // Full HD 1080p
    (2560, 1440), // QHD resolution
];

#[test]
fn test_resolution_frame_generation() {
    // Test that frames can be generated for all test resolutions
    let background_color = [128, 64, 192, 255];
    let circle_x = 100.0;
    let circle_y = 100.0;
    let circle_radius = 50.0;

    for &(width, height) in TEST_RESOLUTIONS {
        // Create a test image at this resolution
        let img = create_test_image(
            width,
            height,
            background_color,
            circle_x as i32,
            circle_y as i32,
            circle_radius as i32,
            2.0, // BLUR_RADIUS
        );

        // Convert to PresentationRequest
        let presentation_request = image_to_presentation(&img, 0);

        // Verify the frame was created correctly
        assert_eq!(presentation_request.width, width);
        assert_eq!(presentation_request.height, height);
        assert!(presentation_request.is_valid());

        // Verify pixel data length matches expectations (RGBA = 4 bytes per pixel)
        let expected_pixel_count = (width * height * 4) as usize;
        assert_eq!(presentation_request.pixel_data.len(), expected_pixel_count);
    }
}

#[test]
fn test_resolution_switching_logic() {
    // Test the resolution switching logic used in resolution_test
    let mut current_resolution_idx = 0;
    let mut frames_since_resolution_change = 0;
    let frames_per_resolution = 60; // 1 second at 60 FPS

    // Simulate cycling through resolutions
    for _ in 0..TEST_RESOLUTIONS.len() * frames_per_resolution {
        frames_since_resolution_change += 1;

        // Check if it's time to switch resolution
        if frames_since_resolution_change >= frames_per_resolution {
            current_resolution_idx = (current_resolution_idx + 1) % TEST_RESOLUTIONS.len();
            frames_since_resolution_change = 0;
        }
    }

    // Should have cycled through all resolutions
    assert_eq!(current_resolution_idx, 0); // Back to start
}

#[test]
fn test_viewport_calculation_with_multiple_resolutions() {
    // Test viewport calculation works correctly with different resolutions
    let window_width = 1920;
    let window_height = 1080;

    for &(content_width, content_height) in TEST_RESOLUTIONS {
        // Calculate aspect-preserving viewport
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
        assert!((calculated_aspect - content_aspect).abs() < tolerance,
            "Aspect ratio not preserved for {}x{}: content={:.3}, calculated={:.3}",
            content_width, content_height, content_aspect, calculated_aspect);
    }
}

#[test]
fn test_extreme_resolution_handling() {
    // Test that the system can handle extreme resolutions without panicking
    let extreme_resolutions = [
        (1, 1),       // Minimal
        (100, 100),   // Small
        (4096, 2160), // 4K
        (8192, 4320), // 8K
    ];

    for &(width, height) in &extreme_resolutions {
        // Create test image
        let img = create_test_image(
            width,
            height,
            [255, 255, 255, 255],
            10,
            10,
            5,
            1.0,
        );

        let presentation_request = image_to_presentation(&img, 0);

        // Verify it's valid
        assert_eq!(presentation_request.width, width);
        assert_eq!(presentation_request.height, height);
        assert!(presentation_request.is_valid());
    }
}
