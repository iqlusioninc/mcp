//! This module provides the [`Transport`] trait, which is used to send and receive
//! messages between a client and server. Implementors of [`Transport`] are responsible for encoding and
//! decoding messages, as well as transmitting/receiving them.

use async_trait::async_trait;
use mcp_types::{
    ClientCapabilities, Implementation, InitializeRequestParams, InitializeResult, JSONRPCError,
    JSONRPCMessage, JSONRPCNotification, JSONRPCRequest, JSONRPCResponse, RequestId,
    LATEST_PROTOCOL_VERSION,
};
use std::error::Error;
use tokio::sync::{mpsc, oneshot};

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
    /// Send a JSON-RPC request and wait for response
    async fn send_request(
        &mut self,
        request: JSONRPCRequest,
        sender: oneshot::Sender<JSONRPCResponse>,
    ) -> Result<(), TransportError>;

    /// Send a JSON-RPC notification (fire-and-forget)
    async fn send_notification(
        &mut self,
        notification: JSONRPCNotification,
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

/// Extension trait for common transport operations
#[async_trait]
pub trait TransportExt: Transport {
    async fn initialize(
        &mut self,
        client_info: Implementation,
        capabilities: ClientCapabilities,
    ) -> Result<InitializeResult, TransportError> {
        todo!()
    }
}

impl<T: Transport + ?Sized> TransportExt for T {}
