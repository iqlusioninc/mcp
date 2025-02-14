use super::transport::{Transport, TransportError};
use mcp_types::{JSONRPCResponse, RequestId};
use serde_json::json;
use std::{
    any::TypeId,
    sync::atomic::{AtomicI64, Ordering},
};
use tokio::sync::oneshot;

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("channel receiver error: {0}")]
    ChannelReceiver(#[from] oneshot::error::RecvError),
    #[error("conversion error")]
    Conversion(#[from] serde_json::Error),
    #[error("request timed out")]
    RequestTimedOut,
    #[error("request failed: {0}")]
    RequestFailed(#[from] TransportError),
    #[error("invalid result: expected type {0:?}, got {1:?}")]
    InvalidResult(TypeId, serde_json::Map<String, serde_json::Value>),
}

pub struct Protocol<T: Transport> {
    transport: T,
    next_id: AtomicI64,
}

impl<T: Transport> Protocol<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            next_id: AtomicI64::new(1),
        }
    }

    pub async fn connect(&mut self) -> Result<(), ProtocolError> {
        self.transport.start().await.map_err(Into::into)
    }

    /// Send a request with method and params, handling JSON-RPC details internally
    pub async fn send_request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<JSONRPCResponse, ProtocolError> {
        let id = RequestId::Integer(self.next_id.fetch_add(1, Ordering::SeqCst));

        let request = json!({
            "id": id,
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let (sender, receiver) = oneshot::channel();

        // Send the request
        self.transport.send(request, Some(sender)).await?;

        // Wait for the response
        receiver.await.map_err(Into::into)
    }

    /// Send a notification with method and params, handling JSON-RPC details internally
    pub async fn send_notification(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), ProtocolError> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        self.transport.send(notification, None).await?;

        Ok(())
    }
}
