# Design Document: Aseprite Presenter Bridge

## Overview

The Aseprite Presenter Bridge enables real-time streaming of canvas data from Aseprite to an external Rust-based presenter application. The system consists of two main components:

1. **Bridge Script (Lua)**: Runs inside Aseprite, captures sprite changes, renders flattened frames, and sends them via WebSocket
2. **Presenter (Rust)**: Receives frame data via WebSocket, parses it, and renders using custom shaders

The design prioritizes simplicity, reliability, and minimal performance impact on Aseprite's workflow.

## Architecture

```
┌─────────────────────────────────────┐
│         Aseprite Process            │
│                                     │
│  ┌──────────────────────────────┐  │
│  │    Bridge Script (Lua)       │  │
│  │                              │  │
│  │  ┌────────────────────────┐  │  │
│  │  │  Event Listener        │  │  │
│  │  │  (sprite.events:on)    │  │  │
│  │  └──────────┬─────────────┘  │  │
│  │             │                 │  │
│  │             ▼                 │  │
│  │  ┌────────────────────────┐  │  │
│  │  │  Frame Renderer        │  │  │
│  │  │  (Image:drawSprite)    │  │  │
│  │  └──────────┬─────────────┘  │  │
│  │             │                 │  │
│  │             ▼                 │  │
│  │  ┌────────────────────────┐  │  │
│  │  │  Message Encoder       │  │  │
│  │  │  (pack header + bytes) │  │  │
│  │  └──────────┬─────────────┘  │  │
│  │             │                 │  │
│  │             ▼                 │  │
│  │  ┌────────────────────────┐  │  │
│  │  │  WebSocket Client      │  │  │
│  │  │  (IXWebSocket)         │  │  │
│  │  └──────────┬─────────────┘  │  │
│  └─────────────┼─────────────────┘  │
└────────────────┼─────────────────────┘
                 │
                 │ WebSocket (Binary)
                 │ ws://127.0.0.1:9001
                 │
                 ▼
┌─────────────────────────────────────┐
│      Presenter Process (Rust)       │
│                                     │
│  ┌──────────────────────────────┐  │
│  │  WebSocket Server            │  │
│  │  (tokio-tungstenite)         │  │
│  └──────────┬───────────────────┘  │
│             │                       │
│             ▼                       │
│  ┌──────────────────────────────┐  │
│  │  Message Parser              │  │
│  │  (parse header + validate)   │  │
│  └──────────┬───────────────────┘  │
│             │                       │
│             ▼                       │
│  ┌──────────────────────────────┐  │
│  │  Format Converter            │  │
│  │  (to RGBA if needed)         │  │
│  └──────────┬───────────────────┘  │
│             │                       │
│             ▼                       │
│  ┌──────────────────────────────┐  │
│  │  PresentationRequest         │  │
│  │  (width, height, RGBA data)  │  │
│  └──────────┬───────────────────┘  │
│             │                       │
│             │ mpsc::channel         │
│             ▼                       │
│  ┌──────────────────────────────┐  │
│  │  ShaderRenderer              │  │
│  │  (load_presentation + render)│  │
│  └──────────────────────────────┘  │
└─────────────────────────────────────┘
```

## Components and Interfaces

### 1. Bridge Script (Lua)

**File**: `presenter_bridge.lua`

**Responsibilities**:
- Subscribe to sprite change events
- Render flattened sprite frames
- Encode frames into binary messages
- Send messages via WebSocket
- Handle connection lifecycle

**Key Functions**:

```lua
-- Configuration
Config = {
  url = "ws://127.0.0.1:9001",
  minReconnectWait = 1.0,  -- seconds
  maxReconnectWait = 30.0,
  debounceMs = 50
}

-- Initialize the bridge
function init()
  -- Create WebSocket connection
  -- Subscribe to sprite events
  -- Send initial frame
end

-- Handle sprite change events
function onSpriteChange(ev)
  -- Debounce rapid changes
  -- Render current frame
  -- Encode and send
end

-- Render the current sprite frame to a flattened image
function renderFrame(sprite, frame)
  -- Create image with sprite dimensions
  -- Use Image:drawSprite to flatten all layers
  -- Return image
end

-- Encode frame data into binary message
function encodeFrame(image, sprite)
  -- Pack header: width, height, colorMode
  -- Append raw pixel bytes
  -- Return binary string
end

-- Clean up on exit
function exit()
  -- Close WebSocket
  -- Unsubscribe from events
end
```

**WebSocket Client Interface**:
- Uses Aseprite's built-in `WebSocket` class (IXWebSocket)
- Binary message mode
- Automatic reconnection with exponential backoff

### 2. Message Format

**Binary Protocol**:

```
┌─────────────────────────────────────────────────┐
│                  Frame Message                  │
├─────────────────────────────────────────────────┤
│  Offset  │  Size  │  Type   │  Description     │
├──────────┼────────┼─────────┼──────────────────┤
│  0       │  4     │  u32le  │  Width           │
│  4       │  4     │  u32le  │  Height          │
│  8       │  1     │  u8     │  Color Mode      │
│  9       │  N     │  bytes  │  Pixel Data      │
└─────────────────────────────────────────────────┘

Color Mode Values:
  0 = RGB (4 bytes per pixel: RGBA)
  1 = Grayscale (2 bytes per pixel: Value + Alpha)
  2 = Indexed (1 byte per pixel: palette index)

Pixel Data Size:
  N = width × height × bytes_per_pixel(color_mode)
```

**Lua Encoding**:
```lua
local header = string.pack("<I4I4B", width, height, colorMode)
local message = header .. image.bytes
```

**Rust Decoding**:
```rust
struct FrameHeader {
    width: u32,
    height: u32,
    color_mode: u8,
}

fn parse_frame(data: &[u8]) -> Result<(FrameHeader, &[u8])> {
    // Parse header (9 bytes)
    // Validate pixel data size
    // Return header and pixel slice
}
```

### 3. Presenter WebSocket Server (Rust)

**Module**: `presenter::bridge`

**Responsibilities**:
- Accept WebSocket connections
- Receive and parse frame messages
- Convert pixel data to `PresentationRequest`
- Send `PresentationRequest` to renderer via channel
- Handle connection errors

**Key Types**:

```rust
use engine_core::PresentationRequest;

pub struct BridgeServer {
    addr: SocketAddr,
    tx: mpsc::Sender<PresentationRequest>,
    frame_counter: Arc<AtomicU64>,
}

pub enum ColorMode {
    Rgb = 0,
    Grayscale = 1,
    Indexed = 2,
}
```

**Key Functions**:

```rust
impl BridgeServer {
    // Start the WebSocket server
    pub async fn start(addr: SocketAddr) -> Result<Self>;
    
    // Get receiver for PresentationRequests
    pub fn presentation_receiver(&self) -> mpsc::Receiver<PresentationRequest>;
    
    // Handle incoming WebSocket connection
    async fn handle_connection(
        ws: WebSocket, 
        tx: mpsc::Sender<PresentationRequest>,
        frame_counter: Arc<AtomicU64>
    );
    
    // Parse frame message and convert to PresentationRequest
    fn parse_frame(data: &[u8], frame_number: u64) -> Result<PresentationRequest>;
    
    // Validate frame data
    fn validate_frame(header: &FrameHeader, data_len: usize) -> Result<()>;
}
```

### 4. Format Converter

**Module**: `presenter::bridge::converter`

**Responsibilities**:
- Convert Aseprite pixel formats to RGBA format required by `PresentationRequest`
- Handle color space conversions
- Ensure pixel data is exactly width × height × 4 bytes

**Key Functions**:

```rust
use engine_core::PresentationRequest;

pub fn to_presentation_request(
    width: u32,
    height: u32,
    color_mode: ColorMode,
    pixel_data: &[u8],
    frame_number: u64,
) -> Result<PresentationRequest> {
    let rgba_data = match color_mode {
        ColorMode::Rgb => convert_rgba(pixel_data)?,
        ColorMode::Grayscale => convert_grayscale(pixel_data)?,
        ColorMode::Indexed => convert_indexed(pixel_data)?,
    };
    
    let request = PresentationRequest::new(rgba_data, width, height, frame_number);
    
    // Validate using built-in validation
    if !request.is_valid() {
        return Err(anyhow!("Invalid PresentationRequest: pixel data size mismatch"));
    }
    
    Ok(request)
}

fn convert_rgba(data: &[u8]) -> Result<Vec<u8>> {
    // Aseprite RGB mode is already RGBA (4 bytes per pixel)
    // Just clone the data
    Ok(data.to_vec())
}

fn convert_grayscale(data: &[u8]) -> Result<Vec<u8>> {
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

fn convert_indexed(data: &[u8]) -> Result<Vec<u8>> {
    // Aseprite indexed mode is 1 byte per pixel (palette index)
    // Without palette data, we can't convert properly
    // For now, treat as grayscale (index as brightness)
    let mut rgba = Vec::with_capacity(data.len() * 4);
    for &index in data {
        rgba.extend_from_slice(&[index, index, index, 255]);
    }
    Ok(rgba)
}
```

## Data Models

### Lua Side

```lua
-- Configuration
Config = {
  url: string,              -- WebSocket URL
  minReconnectWait: number, -- Min seconds between reconnects
  maxReconnectWait: number, -- Max seconds between reconnects
  debounceMs: number        -- Debounce delay in milliseconds
}

-- State
State = {
  ws: WebSocket | nil,      -- WebSocket connection
  sprite: Sprite | nil,     -- Currently tracked sprite
  eventListener: number | nil, -- Event listener reference
  debounceTimer: Timer | nil,  -- Debounce timer
  pendingFrame: boolean     -- Whether a frame send is pending
}
```

### Rust Side

```rust
use engine_core::PresentationRequest;

// Frame header (9 bytes)
#[repr(C, packed)]
struct FrameHeader {
    width: u32,      // Little-endian
    height: u32,     // Little-endian
    color_mode: u8,  // 0=RGB, 1=Grayscale, 2=Indexed
}

// Color mode enum
#[derive(Debug, Clone, Copy)]
enum ColorMode {
    Rgb = 0,
    Grayscale = 1,
    Indexed = 2,
}

// Server state
struct BridgeServer {
    addr: SocketAddr,
    tx: mpsc::Sender<PresentationRequest>,
    frame_counter: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
}
```

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid executions of a system—essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees.*

### Property 1: Message Encoding Round-Trip

*For any* valid sprite dimensions (width, height) and color mode, encoding a frame message and then parsing it should produce equivalent dimensions and color mode values.

**Validates: Requirements 3.1, 3.2, 3.3**

### Property 2: Message Size Consistency

*For any* frame message with width W, height H, and color mode C, the total message size should equal 9 + (W × H × bytes_per_pixel(C)).

**Validates: Requirements 3.6**

### Property 3: Pixel Data Boundary

*For any* valid frame message, the pixel data should start at byte offset 9 and extend to the end of the message.

**Validates: Requirements 3.4**

### Property 4: Parse Validation Rejects Invalid Sizes

*For any* frame message where the pixel data size does not match (width × height × bytes_per_pixel), parsing should return an error.

**Validates: Requirements 4.2**

### Property 5: Malformed Message Handling

*For any* malformed frame message (truncated header, invalid color mode, etc.), the Presenter should log an error and continue running without panicking.

**Validates: Requirements 4.5, 6.3**

### Property 6: WebSocket Reconnection

*For any* WebSocket connection that is lost, the Bridge Script should attempt to reconnect with exponential backoff between minReconnectWait and maxReconnectWait.

**Validates: Requirements 2.4**

### Property 7: Connection Failure Resilience

*For any* WebSocket connection failure during initialization, the Bridge Script should log an error and continue running without crashing Aseprite.

**Validates: Requirements 2.2**

### Property 8: Debouncing Rapid Changes

*For any* sequence of sprite changes occurring within the debounce window (< 50ms apart), only the final frame should be sent.

**Validates: Requirements 7.2**

### Property 9: Sprite Switch Event Handling

*For any* sprite switch event, the Bridge Script should unsubscribe from the old sprite's events and subscribe to the new sprite's events.

**Validates: Requirements 9.2, 9.3**

### Property 10: Immediate Frame on Sprite Switch

*For any* sprite switch event, the Bridge Script should send the new sprite's current frame immediately after subscribing.

**Validates: Requirements 9.4**

### Property 11: Configuration Fallback

*For any* invalid configuration value, the system should use the default value and log a warning.

**Validates: Requirements 5.5**

### Property 12: Port Release on Shutdown

*For any* WebSocket server that is started and then stopped, the port should be released and available for reuse.

**Validates: Requirements 6.5**

### Property 13: RGBA Conversion Identity

*For any* frame data in RGB color mode with dimensions W×H, converting to `PresentationRequest` should produce pixel_data of length W×H×4 with identical pixel values.

**Validates: Requirements 4.3**

### Property 14: Grayscale to RGBA Expansion

*For any* frame data in Grayscale color mode with dimensions W×H, converting to `PresentationRequest` should expand each (Value, Alpha) pair to (Value, Value, Value, Alpha) and produce pixel_data of length W×H×4.

**Validates: Requirements 4.3**

### Property 15: PresentationRequest Validation

*For any* `PresentationRequest` created by the bridge, calling `is_valid()` should return true (pixel_data.len() == width × height × 4).

**Validates: Requirements 4.2**

### Property 16: Event Subscription Cleanup

*For any* sprite that has event listeners attached, when the sprite is switched or closed, all event listeners should be removed.

**Validates: Requirements 9.2**

## Error Handling

### Bridge Script (Lua)

**Connection Errors**:
- Log error to Aseprite console
- Continue running (don't crash Aseprite)
- Attempt reconnection with exponential backoff

**Rendering Errors**:
- Log error to console
- Skip the problematic frame
- Continue listening for future changes

**Send Errors**:
- Queue the frame for retry
- Attempt to reconnect if connection is lost
- Drop oldest queued frames if queue exceeds limit (e.g., 10 frames)

### Presenter (Rust)

**Parse Errors**:
- Log error with details (expected vs actual size, etc.)
- Discard the malformed message
- Continue listening for next message

**Connection Errors**:
- Log connection close/error
- Update UI to show "disconnected" state
- Continue listening for new connections

**Conversion Errors**:
- Log error with frame details
- Skip the frame
- Continue processing next frame

## Testing Strategy

### Unit Tests

**Bridge Script (Lua)**:
- Test message encoding with known dimensions
- Test configuration parsing with valid/invalid values
- Test debounce timer logic

**Presenter (Rust)**:
- Test frame parsing with valid messages
- Test frame parsing with malformed messages (truncated, wrong size, etc.)
- Test color mode conversion functions
- Test configuration loading

### Property-Based Tests

**Message Protocol**:
- Property 1: Encoding round-trip (generate random dimensions, encode, parse, verify)
- Property 2: Message size consistency (generate random frames, verify size calculation)
- Property 3: Pixel data boundary (generate random frames, verify data starts at byte 9)
- Property 4: Parse validation (generate mismatched sizes, verify rejection)

**Error Handling**:
- Property 5: Malformed message handling (generate corrupted messages, verify no panic)
- Property 7: Connection failure resilience (simulate connection failures, verify no crash)

**Debouncing**:
- Property 8: Rapid changes (simulate rapid events, verify only last frame sent)

**Configuration**:
- Property 11: Configuration fallback (generate invalid configs, verify defaults used)

**Resource Cleanup**:
- Property 12: Port release (start/stop server, verify port available)

**Format Conversion**:
- Property 13: RGBA identity (generate random RGBA data, verify preservation)
- Property 14: Grayscale expansion (generate random grayscale data, verify expansion)

### Integration Tests

**End-to-End Flow**:
1. Start Presenter WebSocket server
2. Run Bridge Script in test Aseprite instance
3. Make sprite changes
4. Verify frames received by Presenter
5. Verify shader updates triggered

**Reconnection**:
1. Start Presenter, connect Bridge Script
2. Stop Presenter
3. Verify Bridge Script attempts reconnection
4. Restart Presenter
5. Verify connection re-established

### Testing Configuration

- Minimum 100 iterations per property test
- Each property test tagged with: **Feature: aseprite-presenter-bridge, Property N: [property text]**
- Use `proptest` crate for Rust property tests
- Use manual property testing for Lua (generate random inputs in loops)

## Performance Considerations

### Bridge Script

**Optimization Strategies**:
- Reuse image buffers across frames (avoid repeated allocations)
- Debounce rapid changes (50ms window)
- Use binary WebSocket frames (not text/JSON)
- Avoid unnecessary sprite re-renders

**Expected Performance**:
- Frame encoding: < 5ms for 1920×1080 sprite
- WebSocket send: < 10ms for typical frame sizes
- Total overhead per change: < 20ms

### Presenter

**Optimization Strategies**:
- Process frames asynchronously (don't block WebSocket receiver)
- Use zero-copy parsing where possible
- Batch shader updates if multiple frames arrive rapidly
- Use efficient pixel format conversions

**Expected Performance**:
- Frame parsing: < 2ms
- Format conversion: < 5ms
- Shader update: depends on existing shader implementation

## Deployment

### Bridge Script Installation

1. Save `presenter_bridge.lua` to Aseprite scripts folder
2. Access via: File > Scripts > Open Scripts Folder
3. Run script: File > Scripts > presenter_bridge.lua
4. Script runs in background until Aseprite closes

### Presenter Setup

1. Build Rust presenter with bridge feature enabled
2. Run with: `presenter --bridge-port 9001`
3. Presenter listens for connections automatically

### Configuration

**Bridge Script** (edit top of `.lua` file):
```lua
Config = {
  url = "ws://127.0.0.1:9001",
  minReconnectWait = 1.0,
  maxReconnectWait = 30.0,
  debounceMs = 50
}
```

**Presenter** (command-line or config file):
```bash
presenter --bridge-port 9001
```

Or in `presenter.toml`:
```toml
[bridge]
port = 9001
```

## Future Enhancements

### Potential Improvements

1. **Palette Support**: Send palette data for indexed color mode
2. **Selective Layer Streaming**: Stream only specific layers
3. **Frame Range**: Stream animation frame ranges
4. **Compression**: Add optional zlib/lz4 compression for large sprites
5. **Bidirectional Communication**: Allow Presenter to send commands back to Aseprite
6. **Multiple Clients**: Support multiple Presenter instances connected simultaneously
7. **Frame Metadata**: Include layer info, blend modes, etc.

### Not Planned (Out of Scope)

- Modifying Aseprite C++ code
- Building custom Aseprite binaries
- Supporting Aseprite versions < 1.3 (WebSocket support required)
- Real-time collaborative editing
- Cloud-based streaming
