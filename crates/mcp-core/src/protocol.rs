use super::transport::{Transport, TransportError};
use async_trait::async_trait;
use mcp_types::{
    JSONRPCError, JSONRPCMessage, JSONRPCNotification, JSONRPCRequest, JSONRPCResponse, MCPError,
    MCPResult, RequestId,
};
use std::{
    collections::HashMap,
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
}

pub struct Protocol<T: Transport> {
    transport: T,
    request_handlers: HashMap<&'static str, Box<dyn RequestHandler>>,
    notification_handlers: HashMap<&'static str, Box<dyn NotificationHandler>>,
    next_id: AtomicI64,
}

impl<T: Transport> Protocol<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            request_handlers: HashMap::new(),
            notification_handlers: HashMap::new(),
            next_id: AtomicI64::new(1),
        }
    }

    /// Main message processing loop
    pub async fn run(mut self) -> Result<(), ProtocolError> {
        while let Some(message) = self.transport.recv().await {
            match message? {
                JSONRPCMessage::Request(req) => self.handle_request(req).await,
                JSONRPCMessage::Notification(notif) => todo!(),
                JSONRPCMessage::Response(resp) => todo!(),
                JSONRPCMessage::Error(err) => todo!(),
            }
        }
        Ok(())
    }

    /// Send a request with method and params, handling JSON-RPC details internally
    pub async fn send_request(
        &mut self,
        method: String,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, ProtocolError> {
        let id = RequestId::Integer(self.next_id.fetch_add(1, Ordering::SeqCst));

        // TODO: Should type conversion be handled differently instead of dealing with serde_json in Protocol?
        let params = match params {
            serde_json::Value::Null => None,
            _ => Some(serde_json::from_value(params)?),
        };

        let request = JSONRPCRequest {
            id: id.clone(),
            method,
            params,
            jsonrpc: "2.0".into(),
        };

        let (sender, receiver) = oneshot::channel();

        self.transport.send_request(request, sender).await?;
        let response = receiver.await?;

        Ok(serde_json::to_value(response.result).unwrap())
    }

    async fn handle_request(&mut self, req: JSONRPCRequest) {
        if let Some(handler) = self.request_handlers.get(req.method.as_str()) {
            // Dispatch to handler
            let result = handler
                .handle(serde_json::to_value(req.params).unwrap())
                .await;
            let response = JSONRPCResponse {
                id: req.id,
                result: MCPResult {
                    meta: serde_json::from_value(result).unwrap(),
                },
                jsonrpc: "2.0".into(),
            };
            self.transport.send_response(response).await.unwrap();
        } else {
            // Send method not found error
            let error = JSONRPCError {
                error: MCPError {
                    code: -32601,
                    data: None,
                    message: "Method not found".into(),
                },
                id: req.id,
                jsonrpc: "2.0".into(),
            };
            self.transport.send_error(error).await.unwrap();
        }
    }
}

/// Trait for handling incoming requests
#[async_trait]
trait RequestHandler: Send + Sync {
    async fn handle(&self, params: serde_json::Value) -> serde_json::Value;
}

/// Trait for handling notifications
#[async_trait]
trait NotificationHandler: Send + Sync {
    async fn handle(&self, params: serde_json::Value);
}
