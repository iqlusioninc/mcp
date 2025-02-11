//! This module provides the [`Transport`] trait, which is used to send and receive
//! messages between a client and server. Implementors of [`Transport`] are responsible for encoding and
//! decoding messages, as well as transmitting/receiving them.

use mcp_types::JSONRPCMessage;

/// Represents the Transport layer which handles communication
#[async_trait::async_trait]
pub trait Transport: Send + Sync {
    type Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + Sync + 'static;

    /// Starts the transport, including any connection steps that might need to be taken.
    async fn start(&mut self) -> Result<(), Self::Error>;

    /// Sends a message
    async fn send(&mut self, message: JSONRPCMessage) -> Result<(), Self::Error>;

    /// Closes the connection
    async fn close(&mut self) -> Result<(), Self::Error>;
}
