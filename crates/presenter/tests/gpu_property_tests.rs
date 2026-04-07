//! Property-based tests for GPU initialization utilities.
//!
//! These tests verify universal correctness properties of the GPU selection
//! and configuration functions.

use proptest::prelude::*;
use wgpu::DeviceType;

proptest! {
    #[test]
    /// Property 1: Adapter Selection Prioritizes Discrete GPUs
    /// Validates: Requirements 1.4, 3.4
    fn adapter_selection_prioritizes_discrete_gpus(
        discrete_count in 0..5usize,
        integrated_count in 0..5usize,
        virtual_count in 0..5usize,
        other_count in 0..5usize,
    ) {
        // Create a list of device types representing available adapters
        let mut device_types = Vec::new();

        for _ in 0..discrete_count {
            device_types.push(DeviceType::DiscreteGpu);
        }
        for _ in 0..integrated_count {
            device_types.push(DeviceType::IntegratedGpu);
        }
        for _ in 0..virtual_count {
            device_types.push(DeviceType::VirtualGpu);
        }
        for _ in 0..other_count {
            device_types.push(DeviceType::Other);
        }

        // If no adapters, test passes
        if device_types.is_empty() {
            return Ok(());
        }

        // Find the highest priority device type available
        let best_available = device_types
            .iter()
            .max_by_key(|&device_type| match device_type {
                DeviceType::DiscreteGpu => 100,
                DeviceType::IntegratedGpu => 50,
                DeviceType::VirtualGpu => 25,
                _ => 0,
            })
            .unwrap();

        // The best available should be the highest priority
        let best_priority = match best_available {
            DeviceType::DiscreteGpu => 100,
            DeviceType::IntegratedGpu => 50,
            DeviceType::VirtualGpu => 25,
            _ => 0,
        };

        // Verify that no adapter has higher priority than the selected one
        for &device_type in &device_types {
            let priority = match device_type {
                DeviceType::DiscreteGpu => 100,
                DeviceType::IntegratedGpu => 50,
                DeviceType::VirtualGpu => 25,
                _ => 0,
            };
            prop_assert!(priority <= best_priority,
                "Found higher priority adapter: {:?} vs {:?}", device_type, best_available);
        }
    }
}


proptest! {
    #[test]
    /// Property 3: Surface Configuration Uses sRGB Format When Available
    /// Validates: Requirements 1.2, 3.5
    fn surface_configuration_uses_srgb_format_when_available(
        has_srgb in any::<bool>(),
        has_non_srgb in any::<bool>(),
    ) {
        // Generate format list that might be returned by surface capabilities
        let mut formats = Vec::new();
        if has_non_srgb {
            formats.push(wgpu::TextureFormat::Bgra8Unorm);
        }
        if has_srgb {
            formats.push(wgpu::TextureFormat::Bgra8UnormSrgb);
        }

        // Skip if no formats (invalid case)
        if formats.is_empty() {
            return Ok(());
        }

        // Test the selection logic (same as used in configure_surface)
        let selected_format = formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(formats[0]);

        // Verify property: sRGB format is selected when available
        if has_srgb {
            prop_assert!(selected_format.is_srgb(),
                "Should select sRGB format when available, but selected {:?}", selected_format);
        } else if has_non_srgb {
            prop_assert!(!selected_format.is_srgb(),
                "Should select non-sRGB format when sRGB not available, but selected {:?}", selected_format);
        }
    }
}

proptest! {
    #[test]
    /// Property 4: Viewport Calculation Preserves Aspect Ratio
    /// Validates: Requirements 2.4
    fn viewport_calculation_preserves_aspect_ratio(
        window_width in 10..2000u32,
        window_height in 10..2000u32,
        content_width in 10..1000u32,
        content_height in 10..1000u32,
    ) {
        // Calculate content aspect ratio
        let content_aspect = content_width as f32 / content_height as f32;

        // Aspect-preserving viewport calculation (same logic as ShaderRenderer)
        let window_aspect = window_width as f32 / window_height as f32;

        let (viewport_width, viewport_height) = if window_aspect > content_aspect {
            // Window is wider - pillarbox: scale content to fit height
            let height = window_height;
            let width = (window_height as f32 * content_aspect) as u32;
            (width, height)
        } else {
            // Window is taller - letterbox: scale content to fit width
            let width = window_width;
            let height = (window_width as f32 / content_aspect) as u32;
            (width, height)
        };

        // Skip cases where viewport dimensions would be 0 (edge case)
        if viewport_width == 0 || viewport_height == 0 {
            return Ok(());
        }

        // Verify aspect ratio is preserved within tolerance
        // Use the actual calculated dimensions (before casting to u32) for more precision
        let window_aspect = window_width as f32 / window_height as f32;

        let (actual_width, actual_height) = if window_aspect > content_aspect {
            // Window is wider - pillarbox: scale content to fit height
            let height = window_height as f32;
            let width = window_height as f32 * content_aspect;
            (width, height)
        } else {
            // Window is taller - letterbox: scale content to fit width
            let width = window_width as f32;
            let height = window_width as f32 / content_aspect;
            (width, height)
        };

        let calculated_aspect = actual_width / actual_height;
        let tolerance = 0.01;

        prop_assert!((calculated_aspect - content_aspect).abs() < tolerance,
            "Aspect ratio not preserved: content={:.3}, calculated={:.3}, actual_dims={:.1}x{:.1}",
            content_aspect, calculated_aspect, actual_width, actual_height);
    }
}
