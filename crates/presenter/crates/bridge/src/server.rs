//! WebSocket server implementation for receiving frame data from Aseprite
//!
//! The BridgeServer accepts WebSocket connections and processes incoming frame messages,
//! converting them to PresentationRequest objects for the presenter renderer.

use anyhow::Result;
use engine_core::PresentationRequest;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use crate::{parse_frame_header, to_presentation_request};

/// WebSocket server for receiving frame data from Aseprite
///
/// Accepts connections on a specified socket address and processes binary frame messages.
pub struct BridgeServer {
    /// Socket address the server is bound to
    addr: SocketAddr,
    /// Sender channel for PresentationRequest objects
    tx: mpsc::Sender<PresentationRequest>,
    /// Counter for tracking received frames
    frame_counter: Arc<AtomicU64>,
    /// Shutdown flag
    shutdown: Arc<AtomicBool>,
}

impl BridgeServer {
    /// Create a new BridgeServer instance with a channel
    ///
    /// # Arguments
    /// * `addr` - Socket address to bind to
    ///
    /// # Returns
    /// A tuple of (BridgeServer, mpsc::Receiver<PresentationRequest>)
    pub fn new(addr: SocketAddr) -> (Self, mpsc::Receiver<PresentationRequest>) {
        let (tx, rx) = mpsc::channel(100);
        let server = Self {
            addr,
            tx,
            frame_counter: Arc::new(AtomicU64::new(0)),
            shutdown: Arc::new(AtomicBool::new(false)),
        };
        (server, rx)
    }

    /// Create a new BridgeServer with a provided sender
    ///
    /// # Arguments
    /// * `addr` - Socket address to bind to
    /// * `tx` - Channel sender for PresentationRequest objects
    ///
    /// # Returns
    /// A new BridgeServer instance
    pub fn with_sender(addr: SocketAddr, tx: mpsc::Sender<PresentationRequest>) -> Self {
        Self {
            addr,
            tx,
            frame_counter: Arc::new(AtomicU64::new(0)),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the WebSocket server
    ///
    /// This is an async function that binds to the configured socket address
    /// and spawns a background task to accept connections.
    ///
    /// # Returns
    /// Result with the BridgeServer instance with updated bound address or an error
    pub async fn start(mut self) -> Result<Self> {
        let listener = TcpListener::bind(self.addr).await?;
        let local_addr = listener.local_addr()?;
        
        // Update the address with the actual bound address (port might have been 0)
        self.addr = local_addr;
        
        info!("WebSocket bridge server listening on {}", local_addr);

        // Clone values for the spawned task
        let tx = self.tx.clone();
        let frame_counter = self.frame_counter.clone();
        let shutdown = self.shutdown.clone();

        // Spawn the connection acceptance loop
        tokio::spawn(async move {
            loop {
                // Check shutdown flag
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }

                match listener.accept().await {
                    Ok((stream, addr)) => {
                        let tx = tx.clone();
                        let frame_counter = frame_counter.clone();
                        let shutdown = shutdown.clone();

                        // Spawn a task to handle this connection
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_connection(stream, addr, tx, frame_counter, shutdown).await {
                                error!("Error handling connection from {}: {}", addr, e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Error accepting connection: {}", e);
                        if shutdown.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                }
            }
        });

        Ok(self)
    }

    /// Handle an incoming WebSocket connection
    ///
    /// Accepts a WebSocket upgrade, then loops to receive and process binary messages.
    /// Each message is parsed as a frame message and converted to a PresentationRequest.
    ///
    /// # Arguments
    /// * `stream` - The TCP stream for this connection
    /// * `addr` - The peer address
    /// * `tx` - Channel sender for PresentationRequest objects
    /// * `frame_counter` - Atomic counter for frame numbering
    /// * `shutdown` - Shutdown flag
    async fn handle_connection(
        stream: TcpStream,
        addr: std::net::SocketAddr,
        tx: mpsc::Sender<PresentationRequest>,
        frame_counter: Arc<AtomicU64>,
        shutdown: Arc<AtomicBool>,
    ) -> Result<()> {
        // Accept WebSocket upgrade
        let ws_stream = match accept_async(stream).await {
            Ok(stream) => stream,
            Err(e) => {
                error!("WebSocket upgrade error from {}: {}", addr, e);
                return Err(e.into());
            }
        };

        info!("WebSocket connection established from {}", addr);

        let (_ws_sender, mut ws_receiver) = ws_stream.split();

        // Loop to receive messages
        loop {
            // Check shutdown flag
            if shutdown.load(Ordering::Relaxed) {
                break;
            }

            match tokio::time::timeout(std::time::Duration::from_secs(60), ws_receiver.next()).await {
                Ok(Some(Ok(Message::Binary(data)))) => {
                    // Process binary message
                    match Self::parse_and_send_frame(&data, &tx, &frame_counter).await {
                        Ok(_) => {
                            // Frame processed successfully
                        }
                        Err(e) => {
                            // Log error but continue listening
                            error!("Error processing frame from {}: {}", addr, e);
                        }
                    }
                }
                Ok(Some(Ok(Message::Close(_)))) => {
                    info!("Client {} closed connection", addr);
                    break;
                }
                Ok(Some(Ok(_))) => {
                    // Ignore other message types
                    debug!("Ignoring non-binary message from {}", addr);
                }
                Ok(Some(Err(e))) => {
                    error!("WebSocket error from {}: {}", addr, e);
                    break;
                }
                Ok(None) => {
                    info!("Connection from {} closed", addr);
                    break;
                }
                Err(_) => {
                    // Timeout
                    warn!("Timeout waiting for message from {}", addr);
                    break;
                }
            }
        }

        info!("Closing connection from {}", addr);
        Ok(())
    }

    /// Parse a frame message and send it as a PresentationRequest
    ///
    /// # Arguments
    /// * `data` - The raw frame message bytes
    /// * `tx` - Channel sender for PresentationRequest objects
    /// * `frame_counter` - Atomic counter for frame numbering
    async fn parse_and_send_frame(
        data: &[u8],
        tx: &mpsc::Sender<PresentationRequest>,
        frame_counter: &Arc<AtomicU64>,
    ) -> Result<()> {
        // Parse header
        let header = parse_frame_header(data)?;

        // Validate frame size
        crate::validate_frame_size(&header, data.len())?;

        // Extract pixel data (everything after the 9-byte header)
        let pixel_data = &data[9..];

        // Get frame number
        let frame_number = frame_counter.fetch_add(1, Ordering::Relaxed);

        // Convert to PresentationRequest
        let request = to_presentation_request(
            header.width,
            header.height,
            header.color_mode()?,
            pixel_data,
            frame_number,
        )?;

        // Send through channel
        tx.send(request).await.map_err(|e| anyhow::anyhow!("Failed to send frame: {}", e))?;

        Ok(())
    }

    /// Initiate graceful shutdown
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    /// Check if server is shutting down
    pub fn is_shutting_down(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }

    /// Get the bound socket address
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

// Add missing import
use futures::stream::StreamExt;

#[cfg(test)]
mod tests {
    use super::*;

    /// Property 5: Malformed Message Handling
    /// Validates: Requirements 4.5, 6.3
    /// For any malformed frame message (truncated, wrong size, invalid color mode),
    /// the parser should return an error without panicking.
    #[test]
    fn test_malformed_message_handling() {
        // Test various malformed messages - they should all fail gracefully
        let test_cases = vec![
            (vec![], "empty message"),
            (vec![1, 2, 3], "too short (3 bytes)"),
            (vec![1, 2, 3, 4, 5, 6, 7, 8], "header too short (8 bytes)"),
        ];

        for (malformed_data, desc) in test_cases {
            // Try to parse each malformed message
            let result = crate::parse_frame_header(&malformed_data);
            // All should fail gracefully without panicking
            assert!(
                result.is_err(),
                "Expected error for {} but got success",
                desc
            );
        }

        // Test invalid color mode
        let mut v = vec![10, 0, 0, 0, 10, 0, 0, 0, 5]; // color_mode = 5 (invalid)
        v.resize(9 + (10 * 10 * 4), 0);
        let result = crate::parse_frame_header(&v);
        assert!(
            result.is_err(),
            "Should fail for invalid color mode (5)"
        );

        // Test size mismatch
        let mut v = vec![10, 0, 0, 0, 10, 0, 0, 0, 0]; // RGB, 10x10, expects 400 pixels
        v.resize(9 + (10 * 10 * 2), 0); // But only provide grayscale amount
        let header = crate::parse_frame_header(&v).unwrap();
        let size_result = crate::validate_frame_size(&header, v.len());
        assert!(
            size_result.is_err(),
            "Should fail for size mismatch"
        );
    }

    /// Property 12: Port Release on Shutdown
    /// Validates: Requirements 6.5
    /// For any WebSocket server that is started and then stopped,
    /// the port should be released and available for reuse.
    #[tokio::test]
    async fn test_port_release_on_shutdown() {
        // Use a fixed port to test reuse
        let port = 19999;
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();

        // Start first server
        let (server1, _rx1) = BridgeServer::new(addr);
        let server1 = server1.start().await.unwrap();
        info!("Server 1 started on {}", server1.addr());

        // Server is running, trying to bind to same port should fail
        let listener_result = TcpListener::bind(addr).await;
        assert!(listener_result.is_err(), "Port should still be in use");

        // Shutdown the server
        server1.shutdown();

        // Give it a moment to clean up
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Now we should be able to bind to the same port
        let listener_result = TcpListener::bind(addr).await;
        assert!(listener_result.is_ok(), "Port should be released after shutdown");

        // Clean up
        drop(listener_result);
    }

    #[tokio::test]
    async fn test_bridge_server_creation() {
        let addr = "127.0.0.1:0".parse().unwrap();
        let (server, _rx) = BridgeServer::new(addr);

        // Should be able to access the address
        let created_addr = server.addr();
        assert_eq!(created_addr.ip().to_string(), "127.0.0.1");
        // Port 0 means "any available port" - it will be assigned when we bind
        assert_eq!(created_addr.port(), 0);

        // Should not be shutting down initially
        assert!(!server.is_shutting_down());
    }

    #[tokio::test]
    async fn test_bridge_server_start() {
        let addr = "127.0.0.1:0".parse().unwrap();
        let (server, _rx) = BridgeServer::new(addr);

        // Should be able to start
        let started = server.start().await;
        assert!(started.is_ok());

        let server = started.unwrap();
        let bound_addr = server.addr();
        assert_ne!(bound_addr.port(), 0, "Port should be assigned after binding");

        server.shutdown();
    }

    #[tokio::test]
    async fn test_graceful_shutdown() {
        let addr = "127.0.0.1:0".parse().unwrap();
        let (server, _rx) = BridgeServer::new(addr);
        let server = server.start().await.unwrap();

        assert!(!server.is_shutting_down());

        server.shutdown();

        assert!(server.is_shutting_down());
    }
}
