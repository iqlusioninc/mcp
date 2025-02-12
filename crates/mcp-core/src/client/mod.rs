use crate::protocol::ProtocolError;

use super::{
    protocol::Protocol,
    transport::{Transport, TransportError},
};
use mcp_types::{
    ClientCapabilities, Implementation, InitializeRequestParams, JSONRPCNotification,
    JSONRPCRequest, ListToolsResult, RequestId, ServerCapabilities,
};

pub mod handlers;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("transport error: {0}")]
    Transport(#[from] TransportError),
}

pub struct Client<T: Transport> {
    protocol: Protocol<T>,
    capabilities: ClientCapabilities,
    client_info: Implementation,
    server_info: Option<Implementation>,
    server_capabilities: Option<ServerCapabilities>,
}

impl<T: Transport> Client<T> {
    pub async fn new(transport: T) -> Self {
        Self {
            protocol: Protocol::new(transport),
            capabilities: ClientCapabilities::default(),
            client_info: Implementation {
                name: "mcp-core".into(),
                version: "0.1.0".into(),
            },
            server_info: None,
            server_capabilities: None,
        }
    }

    pub async fn connect(&mut self) -> Result<(), TransportError> {
        Ok(())
    }

    pub async fn ping(&mut self) -> Result<(), ClientError> {
        self.protocol
            .send_request("ping".into(), serde_json::Value::Null)
            .await?;

        Ok(())
    }

    pub async fn list_tools(&mut self) -> Result<ListToolsResult, ClientError> {
        let result = self
            .protocol
            .send_request("tools/list".into(), serde_json::Value::Null)
            .await?;

        Ok(serde_json::from_value(result)?)
    }
}
