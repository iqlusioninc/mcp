use super::transport::{Transport, TransportError};
use mcp_types::{v2024_11_05::convert::assert_v2024_11_05_type, RequestId};
use serde::de::DeserializeOwned;
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
    pub async fn send_request<R: DeserializeOwned + 'static>(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<R, ProtocolError> {
        let id = RequestId::Integer(self.next_id.fetch_add(1, Ordering::SeqCst));

        let request = json!({
            "id": id,
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let (sender, receiver) = oneshot::channel();

        // Send the request
        self.transport.send_request(request, sender).await?;

        // Wait for the response
        let result = receiver.await?.result.meta;

        // Validate and deserialize the result
        match assert_v2024_11_05_type::<R>(serde_json::to_value(result.clone()).unwrap()) {
            Some(result) => Ok(result),
            None => Err(ProtocolError::InvalidResult(TypeId::of::<R>(), result)),
        }
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

        self.transport.send_notification(notification).await?;

        Ok(())
    }
}
