//! This module provides the [`Transport`] trait, which is used to send and receive
//! messages between a client and server. Implementors of [`Transport`] are responsible for encoding and
//! decoding messages, as well as transmitting/receiving them.

use async_trait::async_trait;
use mcp_types::{JSONRPCError, JSONRPCMessage, JSONRPCResponse};
use std::error::Error;
use tokio::sync::oneshot;

/// Core transport error type
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(Box<dyn Error + Send + Sync>),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Protocol error: {0}")]
    Protocol(String),
}

#[async_trait]
pub trait Transport {
    /// Start communication
    async fn start(&mut self) -> Result<(), TransportError>;

    /// Send a JSON-RPC request and wait for response
    async fn send_request(
        &mut self,
        request: serde_json::Value,
        sender: oneshot::Sender<JSONRPCResponse>,
    ) -> Result<(), TransportError>;

    /// Send a JSON-RPC notification (fire-and-forget)
    async fn send_notification(
        &mut self,
        notification: serde_json::Value,
    ) -> Result<(), TransportError>;

    /// Send a JSON-RPC response to a request
    async fn send_response(&mut self, response: JSONRPCResponse) -> Result<(), TransportError>;

    /// Send a JSON-RPC error response
    async fn send_error(&mut self, error: JSONRPCError) -> Result<(), TransportError>;

    /// Receive incoming messages
    async fn recv(&mut self) -> Option<Result<JSONRPCMessage, TransportError>>;

    /// Close the transport connection
    async fn close(self) -> Result<(), TransportError>;
}
