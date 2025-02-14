//! This module provides the [`Transport`] trait, which is used to send and receive
//! messages between a client and server. Implementors of [`Transport`] are responsible for encoding and
//! decoding messages, as well as transmitting/receiving them.

use async_trait::async_trait;
use mcp_types::{v2024_11_05::request_id::GetRequestId, JSONRPCMessage, JSONRPCResponse};
use serde_json::json;
use std::{error::Error, sync::mpsc};
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
    #[error("Channel closed")]
    ChannelClosed,
}

#[async_trait]
pub trait Transport {
    /// Start communication
    async fn start(&mut self) -> Result<(), TransportError>;

    /// Send a JSON-RPC message with an optional response sender if the message is a request
    async fn send(
        &mut self,
        message: serde_json::Value,
        sender: Option<oneshot::Sender<JSONRPCResponse>>,
    ) -> Result<(), TransportError>;

    /// Receive incoming messages
    async fn recv(&mut self) -> Option<Result<JSONRPCMessage, TransportError>>;

    /// Close the transport connection
    async fn close(self) -> Result<(), TransportError>;
}

pub struct MockTransport {
    pub messages: Vec<JSONRPCMessage>,
    pub sender: mpsc::Sender<JSONRPCMessage>,
    pub current_id: u64,
}

impl MockTransport {
    pub fn new() -> (Self, mpsc::Receiver<JSONRPCMessage>) {
        let (sender, receiver) = mpsc::channel();
        (
            Self {
                messages: vec![],
                sender,
                current_id: 0,
            },
            receiver,
        )
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn start(&mut self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn send(
        &mut self,
        message: serde_json::Value,
        sender: Option<oneshot::Sender<JSONRPCResponse>>,
    ) -> Result<(), TransportError> {
        let message: JSONRPCMessage =
            serde_json::from_value(message).map_err(TransportError::Serialization)?;
        self.messages.push(message.clone());
        self.sender
            .send(message.clone())
            .map_err(|_| TransportError::ChannelClosed)?;
        if let Some(sender) = sender {
            let id = message.get_request_id().unwrap();
            let response = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": "test",
            });
            sender
                .send(serde_json::from_value(response).map_err(TransportError::Serialization)?)
                .map_err(|_| TransportError::ChannelClosed)?;
        }
        Ok(())
    }

    async fn recv(&mut self) -> Option<Result<JSONRPCMessage, TransportError>> {
        let mock_request = json!({
            "jsonrpc": "2.0",
            "id": self.current_id,
            "method": "test",
            "params": {},
        });
        self.current_id += 1;

        Some(
            serde_json::from_value(mock_request)
                .map_err(TransportError::Serialization)
                .map(JSONRPCMessage::Request),
        )
    }

    async fn close(self) -> Result<(), TransportError> {
        Ok(())
    }
}
