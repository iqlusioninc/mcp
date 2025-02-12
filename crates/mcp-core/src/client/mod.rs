use crate::protocol::ProtocolError;

use super::{
    protocol::Protocol,
    transport::{Transport, TransportError},
};
use mcp_types::{
    ClientCapabilities, Implementation, InitializeRequestParams, InitializeResult,
    InitializedNotificationParams, ListToolsRequestParams, ListToolsResult, PingRequestParams,
    PingRequestParamsMeta, ServerCapabilities, LATEST_PROTOCOL_VERSION,
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
                version: env!("CARGO_PKG_VERSION").into(),
            },
            server_info: None,
            server_capabilities: None,
        }
    }

    pub async fn connect(&mut self) -> Result<(), ClientError> {
        self.protocol.connect().await.map_err(Into::into)
    }

    pub async fn initialize(&mut self) -> Result<(), ClientError> {
        let params = InitializeRequestParams {
            client_info: self.client_info.clone(),
            capabilities: self.capabilities.clone(),
            protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
        };

        let result = self
            .protocol
            .send_request::<InitializeResult>("initialize", serde_json::to_value(params).unwrap())
            .await?;

        self.server_info = Some(result.server_info);
        self.server_capabilities = Some(result.capabilities);

        let params = InitializedNotificationParams {
            meta: Default::default(),
        };

        self.protocol
            .send_notification(
                "notification/initialized",
                serde_json::to_value(params).unwrap(),
            )
            .await?;

        Ok(())
    }

    pub async fn ping(&mut self) -> Result<(), ClientError> {
        let params = PingRequestParams {
            meta: Some(PingRequestParamsMeta {
                progress_token: None,
            }),
        };

        self.protocol
            .send_request::<()>("ping", serde_json::to_value(params).unwrap())
            .await?;

        Ok(())
    }

    pub async fn list_tools(
        &mut self,
        cursor: Option<String>,
    ) -> Result<ListToolsResult, ClientError> {
        let params = ListToolsRequestParams { cursor };

        self.protocol
            .send_request::<ListToolsResult>(
                "tools/list".into(),
                serde_json::to_value(params).unwrap(),
            )
            .await
            .map_err(Into::into)
    }
}
