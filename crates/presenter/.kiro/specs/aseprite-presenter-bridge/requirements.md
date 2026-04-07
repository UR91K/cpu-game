# Requirements Document

## Introduction

This feature enables live preview of Aseprite canvas changes in an external Rust-based presenter application using a shader program. The integration uses a Lua script running inside Aseprite to stream canvas data over WebSocket to the presenter, which renders the frames using a custom shader.

## Glossary

- **Aseprite**: The pixel art editor application
- **Presenter**: The external Rust application that receives and renders canvas data with shaders
- **Bridge_Script**: The Lua script running inside Aseprite that captures and sends canvas data
- **Canvas_Data**: The flattened, rendered pixel data of the current sprite frame
- **WebSocket_Server**: The WebSocket server running in the Presenter that receives canvas data
- **Frame_Message**: A binary message containing canvas metadata and pixel data
- **Sprite_Event**: An Aseprite event triggered when the sprite content changes

## Requirements

### Requirement 1: Live Canvas Streaming

**User Story:** As a user, I want Aseprite to automatically send canvas updates to the Presenter, so that I can see my artwork rendered with custom shaders in real-time without manual saves.

#### Acceptance Criteria

1. WHEN the user makes any change to the sprite (drawing, erasing, layer changes, etc.), THE Bridge_Script SHALL capture the change event
2. WHEN a change event is captured, THE Bridge_Script SHALL render the current frame to a flattened image
3. WHEN the flattened image is ready, THE Bridge_Script SHALL send it to the Presenter via WebSocket
4. THE Bridge_Script SHALL send updates within 100ms of a sprite change
5. WHEN the Presenter receives canvas data, THE Presenter SHALL update the display without crashing

### Requirement 2: WebSocket Communication

**User Story:** As a developer, I want a reliable WebSocket connection between Aseprite and the Presenter, so that canvas data can be transmitted efficiently.

#### Acceptance Criteria

1. WHEN the Bridge_Script initializes, THE Bridge_Script SHALL attempt to connect to the WebSocket_Server at a configurable address
2. WHEN the WebSocket connection fails, THE Bridge_Script SHALL log an error and continue running without crashing Aseprite
3. WHEN the WebSocket connection is established, THE Bridge_Script SHALL send an initial frame immediately
4. WHEN the WebSocket connection is lost, THE Bridge_Script SHALL attempt to reconnect automatically
5. THE WebSocket_Server SHALL accept connections on a configurable port (default: 9001)

### Requirement 3: Frame Message Format

**User Story:** As a developer, I want a well-defined binary protocol for frame messages, so that the Presenter can correctly parse and render canvas data.

#### Acceptance Criteria

1. THE Frame_Message SHALL contain a header with sprite width (4 bytes, little-endian uint32)
2. THE Frame_Message SHALL contain a header with sprite height (4 bytes, little-endian uint32)
3. THE Frame_Message SHALL contain a header with color mode (1 byte: 0=RGB, 1=Grayscale, 2=Indexed)
4. THE Frame_Message SHALL contain raw pixel data immediately following the header
5. THE pixel data SHALL be in the format specified by the color mode (RGBA for RGB, etc.)
6. THE total message size SHALL be: 9 bytes (header) + (width × height × bytes_per_pixel)

### Requirement 4: Presenter Integration

**User Story:** As a user, I want the Presenter to receive and render canvas data, so that I can see my artwork with custom shader effects.

#### Acceptance Criteria

1. WHEN the Presenter receives a Frame_Message, THE Presenter SHALL parse the header to extract dimensions and color mode
2. WHEN the header is parsed, THE Presenter SHALL validate that the pixel data size matches the expected size
3. WHEN pixel data is validated, THE Presenter SHALL convert it to the format required by the shader program
4. WHEN data is converted, THE Presenter SHALL update the shader input and trigger a re-render
5. IF the Frame_Message is malformed, THEN THE Presenter SHALL log an error and continue running

### Requirement 5: Configuration and Setup

**User Story:** As a user, I want easy configuration of the bridge connection, so that I can customize the WebSocket address and port.

#### Acceptance Criteria

1. THE Bridge_Script SHALL read configuration from a Lua table at the top of the script
2. THE configuration SHALL include WebSocket URL (default: "ws://127.0.0.1:9001")
3. THE configuration SHALL include reconnection settings (min wait, max wait)
4. THE Presenter SHALL read WebSocket port from command-line arguments or configuration file
5. WHEN configuration is invalid, THE system SHALL use default values and log a warning

### Requirement 6: Error Handling and Resilience

**User Story:** As a user, I want the bridge to handle errors gracefully, so that neither Aseprite nor the Presenter crashes during operation.

#### Acceptance Criteria

1. WHEN the Bridge_Script encounters an error rendering a frame, THE Bridge_Script SHALL log the error and skip that frame
2. WHEN the WebSocket connection fails during send, THE Bridge_Script SHALL queue the frame and retry on reconnection
3. WHEN the Presenter receives corrupted data, THE Presenter SHALL discard the frame and log an error
4. WHEN Aseprite closes, THE Bridge_Script SHALL cleanly close the WebSocket connection
5. WHEN the Presenter closes, THE WebSocket_Server SHALL cleanly shutdown and release the port

### Requirement 7: Performance Optimization

**User Story:** As a user, I want the bridge to operate efficiently, so that it doesn't slow down my Aseprite workflow.

#### Acceptance Criteria

1. THE Bridge_Script SHALL reuse image buffers when possible to minimize allocations
2. WHEN multiple changes occur rapidly (< 50ms apart), THE Bridge_Script SHALL debounce and send only the latest frame
3. THE WebSocket_Server SHALL use binary frames (not text) for efficient transmission
4. THE Presenter SHALL process frames asynchronously to avoid blocking the WebSocket receiver
5. THE system SHALL support sprites up to 4096×4096 pixels without performance degradation

### Requirement 8: Installation and Deployment

**User Story:** As a user, I want simple installation steps, so that I can start using the bridge quickly.

#### Acceptance Criteria

1. THE Bridge_Script SHALL be a single `.lua` file that can be placed in Aseprite's scripts folder
2. THE Bridge_Script SHALL be loadable via Aseprite's "File > Scripts > Open Scripts Folder" menu
3. THE Presenter SHALL be a standalone executable that can run independently
4. THE installation documentation SHALL include step-by-step instructions for both Windows and other platforms
5. THE system SHALL work with official Aseprite releases without requiring custom builds

### Requirement 9: Active Sprite Tracking

**User Story:** As a user, I want the bridge to automatically track the active sprite, so that switching between sprites updates the preview.

#### Acceptance Criteria

1. WHEN the user switches to a different sprite, THE Bridge_Script SHALL detect the change
2. WHEN a sprite switch is detected, THE Bridge_Script SHALL unsubscribe from the old sprite's events
3. WHEN a sprite switch is detected, THE Bridge_Script SHALL subscribe to the new sprite's events
4. WHEN a sprite switch is detected, THE Bridge_Script SHALL send the new sprite's current frame immediately
5. WHEN no sprite is active, THE Bridge_Script SHALL send a blank/empty frame or stop sending

### Requirement 10: Graceful Shutdown

**User Story:** As a user, I want the bridge to clean up resources properly, so that I can restart it without issues.

#### Acceptance Criteria

1. WHEN the Bridge_Script's exit() function is called, THE Bridge_Script SHALL close the WebSocket connection
2. WHEN the Bridge_Script's exit() function is called, THE Bridge_Script SHALL unsubscribe from all sprite events
3. WHEN the Presenter receives a WebSocket close event, THE Presenter SHALL display a "disconnected" state
4. WHEN the Presenter is terminated, THE WebSocket_Server SHALL close all active connections
5. THE system SHALL not leave zombie processes or locked ports after shutdown
