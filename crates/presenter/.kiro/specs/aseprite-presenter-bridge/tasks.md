# Implementation Plan: Aseprite Presenter Bridge

## Overview

This implementation plan breaks down the Aseprite Presenter Bridge into discrete coding tasks. The bridge consists of two main components: a Lua script running in Aseprite and a Rust WebSocket server in the Presenter. Tasks are ordered to enable incremental development and testing.

## Tasks

- [x] 1. Create Rust bridge module structure
  - Create `presenter/crates/bridge/` directory
  - Add `bridge` crate to workspace `Cargo.toml`
  - Create `bridge/Cargo.toml` with dependencies: `tokio`, `tokio-tungstenite`, `anyhow`, `engine-core`
  - Create `bridge/src/lib.rs` with module structure
  - _Requirements: 2.5, 8.3_

- [x] 2. Implement frame message parsing
  - [x] 2.1 Define `FrameHeader` struct with packed repr
    - Create struct with `width: u32`, `height: u32`, `color_mode: u8`
    - Add `#[repr(C, packed)]` attribute for binary compatibility
    - Implement `ColorMode` enum (Rgb=0, Grayscale=1, Indexed=2)
    - _Requirements: 3.1, 3.2, 3.3_

  - [x] 2.2 Implement `parse_frame_header` function
    - Parse first 9 bytes as little-endian header
    - Validate color mode is 0, 1, or 2
    - Return `Result<FrameHeader, Error>`
    - _Requirements: 3.1, 3.2, 3.3_

  - [x]* 2.3 Write property test for header parsing round-trip
    - **Property 1: Message Encoding Round-Trip**
    - **Validates: Requirements 3.1, 3.2, 3.3**
    - Generate random width, height, color_mode
    - Encode to bytes, parse back, verify equality
    - Run 100+ iterations

  - [x] 2.4 Implement `validate_frame_size` function
    - Calculate expected pixel data size based on color mode
    - RGB: width × height × 4, Grayscale: width × height × 2, Indexed: width × height × 1
    - Verify actual data length matches expected
    - _Requirements: 3.6, 4.2_

  - [x]* 2.5 Write property test for size validation
    - **Property 2: Message Size Consistency**
    - **Validates: Requirements 3.6**
    - Generate random dimensions and color modes
    - Verify size calculation matches formula
    - Run 100+ iterations

  - [x]* 2.6 Write property test for pixel data boundary
    - **Property 3: Pixel Data Boundary**
    - **Validates: Requirements 3.4**
    - Generate random frames
    - Verify pixel data starts at byte 9
    - Run 100+ iterations

- [x] 3. Implement format conversion to PresentationRequest
  - [x] 3.1 Implement `convert_rgba` function
    - Take RGBA pixel data slice
    - Clone to Vec<u8> (already in correct format)
    - Return Result<Vec<u8>>
    - _Requirements: 4.3_

  - [x] 3.2 Implement `convert_grayscale` function
    - Take grayscale pixel data (2 bytes per pixel: Value, Alpha)
    - Expand to RGBA: (Value, Value, Value, Alpha)
    - Allocate Vec with capacity width × height × 4
    - Return Result<Vec<u8>>
    - _Requirements: 4.3_

  - [x] 3.3 Implement `convert_indexed` function
    - Take indexed pixel data (1 byte per pixel)
    - Convert to grayscale RGBA (index as brightness)
    - Note: Proper conversion requires palette data (future enhancement)
    - Return Result<Vec<u8>>
    - _Requirements: 4.3_

  - [x] 3.4 Implement `to_presentation_request` function
    - Take width, height, color_mode, pixel_data, frame_number
    - Call appropriate conversion function based on color_mode
    - Create PresentationRequest with converted data
    - Validate using `request.is_valid()`
    - Return Result<PresentationRequest>
    - _Requirements: 4.1, 4.2, 4.3_

  - [x]* 3.5 Write property test for RGBA conversion identity
    - **Property 13: RGBA Conversion Identity**
    - **Validates: Requirements 4.3**
    - Generate random RGBA data
    - Convert to PresentationRequest
    - Verify pixel values unchanged
    - Run 100+ iterations

  - [x]* 3.6 Write property test for grayscale expansion
    - **Property 14: Grayscale to RGBA Expansion**
    - **Validates: Requirements 4.3**
    - Generate random grayscale data
    - Convert to PresentationRequest
    - Verify (V, A) → (V, V, V, A) expansion
    - Run 100+ iterations

  - [x]* 3.7 Write property test for PresentationRequest validation
    - **Property 15: PresentationRequest Validation**
    - **Validates: Requirements 4.2**
    - Generate random valid frames
    - Convert to PresentationRequest
    - Verify `is_valid()` returns true
    - Run 100+ iterations

- [x] 4. Implement WebSocket server
  - [x] 4.1 Create `BridgeServer` struct
    - Add fields: `addr: SocketAddr`, `tx: mpsc::Sender<PresentationRequest>`, `frame_counter: Arc<AtomicU64>`
    - Implement `new()` constructor
    - _Requirements: 2.5_

  - [x] 4.2 Implement `start()` async function
    - Bind to socket address
    - Spawn tokio task to accept connections
    - Return BridgeServer instance
    - _Requirements: 2.1, 2.5_

  - [x] 4.3 Implement `handle_connection()` async function
    - Accept WebSocket upgrade
    - Loop to receive binary messages
    - Parse each message with `parse_frame_header` and `to_presentation_request`
    - Send PresentationRequest through channel
    - Handle errors gracefully (log and continue)
    - _Requirements: 2.3, 4.1, 4.4_

  - [x] 4.4 Implement error handling for malformed messages
    - Catch parse errors
    - Log error details
    - Continue listening for next message (don't crash)
    - _Requirements: 4.5, 6.3_

  - [x]* 4.5 Write property test for malformed message handling
    - **Property 5: Malformed Message Handling**
    - **Validates: Requirements 4.5, 6.3**
    - Generate corrupted messages (truncated, wrong size, invalid color mode)
    - Verify parser returns error without panicking
    - Run 100+ iterations

  - [x] 4.6 Add graceful shutdown support
    - Add `shutdown: Arc<AtomicBool>` field
    - Check shutdown flag in connection loop
    - Close all connections on shutdown
    - _Requirements: 6.5, 10.4_

  - [x]* 4.7 Write property test for port release
    - **Property 12: Port Release on Shutdown**
    - **Validates: Requirements 6.5**
    - Start server on random port
    - Stop server
    - Verify port can be reused
    - Run 100+ iterations

- [ ] 5. Integrate bridge with presenter application
  - [ ] 5.1 Add bridge module to presenter binary
    - Import `bridge` crate in `presenter/src/main.rs`
    - Add command-line argument `--bridge-port <PORT>`
    - Parse port with default 9001
    - _Requirements: 5.4_

  - [ ] 5.2 Start bridge server in presenter
    - Create `mpsc::channel` for PresentationRequests
    - Call `BridgeServer::start()` with configured address
    - Spawn task to receive PresentationRequests from channel
    - _Requirements: 2.5, 4.4_

  - [ ] 5.3 Connect bridge to ShaderRenderer
    - When PresentationRequest received, call `renderer.load_presentation()`
    - Trigger window redraw
    - Handle errors (log and continue)
    - _Requirements: 4.4, 1.5_

  - [ ]* 5.4 Write integration test for end-to-end flow
    - Start bridge server
    - Send test frame message
    - Verify PresentationRequest received
    - Verify renderer updated

- [ ] 6. Create Lua bridge script
  - [ ] 6.1 Create `presenter_bridge.lua` file
    - Add configuration table at top (url, reconnect settings, debounce)
    - Add state variables (ws, sprite, eventListener, etc.)
    - _Requirements: 5.1, 5.2, 5.3_

  - [ ] 6.2 Implement `init()` function
    - Create WebSocket with configured URL
    - Set up onreceive callback (log connection events)
    - Call `ws:connect()`
    - Subscribe to app.events for site changes
    - _Requirements: 2.1, 2.3, 9.1_

  - [ ] 6.3 Implement `renderFrame()` function
    - Get active sprite and frame
    - Create Image with sprite dimensions
    - Call `image:drawSprite(sprite, frame)` to flatten layers
    - Return image
    - _Requirements: 1.2_

  - [ ] 6.4 Implement `encodeFrame()` function
    - Get image dimensions and color mode
    - Pack header: `string.pack("<I4I4B", width, height, colorMode)`
    - Append `image.bytes` to header
    - Return binary string
    - _Requirements: 3.1, 3.2, 3.3, 3.4_

  - [ ] 6.5 Implement `onSpriteChange()` event handler
    - Check if WebSocket is connected
    - Call `renderFrame()` to get flattened image
    - Call `encodeFrame()` to create binary message
    - Call `ws:sendBinary(message)`
    - Handle errors (log and continue)
    - _Requirements: 1.1, 1.2, 1.3_

  - [ ] 6.6 Implement debouncing logic
    - Add timer to delay sends by configured debounceMs
    - Cancel previous timer if new change occurs
    - Only send latest frame after debounce period
    - _Requirements: 7.2_

  - [ ]* 6.7 Write property test for debouncing
    - **Property 8: Debouncing Rapid Changes**
    - **Validates: Requirements 7.2**
    - Simulate rapid sprite changes
    - Verify only final frame sent
    - Run 100+ iterations

  - [ ] 6.8 Implement sprite switch handling
    - Listen for app.events "sitechange"
    - Unsubscribe from old sprite events
    - Subscribe to new sprite events
    - Send new sprite's current frame immediately
    - _Requirements: 9.1, 9.2, 9.3, 9.4_

  - [ ]* 6.9 Write property test for sprite switch
    - **Property 9: Sprite Switch Event Handling**
    - **Validates: Requirements 9.2, 9.3**
    - Simulate sprite switches
    - Verify old subscriptions removed, new added
    - Run 100+ iterations

  - [ ]* 6.10 Write property test for immediate frame on switch
    - **Property 10: Immediate Frame on Sprite Switch**
    - **Validates: Requirements 9.4**
    - Simulate sprite switch
    - Verify frame sent immediately
    - Run 100+ iterations

  - [ ] 6.11 Implement `exit()` function
    - Close WebSocket connection
    - Unsubscribe from all events
    - Clean up timers
    - _Requirements: 10.1, 10.2_

- [ ] 7. Add reconnection logic
  - [ ] 7.1 Implement exponential backoff in Lua script
    - Track reconnection attempts
    - Calculate wait time: min(minWait * 2^attempts, maxWait)
    - Use timer to delay reconnection
    - _Requirements: 2.4_

  - [ ] 7.2 Handle connection loss in onreceive callback
    - Detect WebSocketMessageType.CLOSE and ERROR
    - Log connection loss
    - Trigger reconnection with backoff
    - _Requirements: 2.4, 6.2_

  - [ ]* 7.3 Write property test for reconnection
    - **Property 6: WebSocket Reconnection**
    - **Validates: Requirements 2.4**
    - Simulate connection loss
    - Verify reconnection attempts with backoff
    - Run 100+ iterations

  - [ ]* 7.4 Write property test for connection failure resilience
    - **Property 7: Connection Failure Resilience**
    - **Validates: Requirements 2.2**
    - Simulate connection failures
    - Verify script continues without crashing
    - Run 100+ iterations

- [ ] 8. Add configuration and error handling
  - [ ] 8.1 Implement configuration validation in Lua
    - Check URL format
    - Validate reconnect wait times
    - Use defaults for invalid values
    - Log warnings for invalid config
    - _Requirements: 5.5_

  - [ ]* 8.2 Write property test for configuration fallback
    - **Property 11: Configuration Fallback**
    - **Validates: Requirements 5.5**
    - Generate invalid configs
    - Verify defaults used
    - Run 100+ iterations

  - [ ] 8.3 Add error handling for render failures
    - Wrap `renderFrame()` in pcall
    - Log errors to Aseprite console
    - Skip problematic frame
    - Continue listening for changes
    - _Requirements: 6.1_

  - [ ] 8.4 Add error handling for send failures
    - Detect send errors
    - Log error
    - Attempt reconnection if needed
    - _Requirements: 6.2_

- [ ] 9. Create documentation and examples
  - [ ] 9.1 Write README for bridge script
    - Installation instructions
    - Configuration options
    - Troubleshooting guide
    - _Requirements: 8.4_

  - [ ] 9.2 Write README for Rust bridge module
    - API documentation
    - Integration examples
    - Configuration options
    - _Requirements: 8.4_

  - [ ] 9.3 Create example presenter application
    - Simple app that receives frames and displays them
    - Demonstrates full integration
    - _Requirements: 8.4_

- [ ] 10. Final integration and testing
  - [ ] 10.1 Test with real Aseprite instance
    - Load script in Aseprite
    - Make sprite changes
    - Verify frames received by presenter
    - Verify shader renders correctly

  - [ ] 10.2 Test reconnection scenarios
    - Stop/start presenter while Aseprite running
    - Verify automatic reconnection
    - Verify frames resume after reconnect

  - [ ] 10.3 Test sprite switching
    - Open multiple sprites
    - Switch between them
    - Verify correct frames sent

  - [ ] 10.4 Performance testing
    - Test with large sprites (2048×2048)
    - Test with rapid changes
    - Verify < 20ms overhead per change
    - _Requirements: 7.5_

  - [ ] 10.5 Ensure all tests pass
    - Run all unit tests
    - Run all property tests
    - Run integration tests
    - Fix any failures

## Notes

- Tasks marked with `*` are optional property-based tests that can be skipped for faster MVP
- Each task references specific requirements for traceability
- Property tests should run minimum 100 iterations
- Integration tests require both Aseprite and Presenter running
- The Lua script can be developed and tested independently using mock WebSocket
- The Rust bridge can be tested independently using mock frame messages
