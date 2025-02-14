use mcp_types::{Implementation, PingRequestParams, PingRequestParamsMeta, ServerCapabilities};

use crate::{protocol::{Protocol, ProtocolError}, transport::{Transport, TransportError}};

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("transport error: {0}")]
    Transport(#[from] TransportError),
}

pub struct Server<T: Transport> {
    protocol: Protocol<T>,
    capabilities: ServerCapabilities,
    server_info: Implementation,
    client_info: Option<Implementation>,
    client_capabilities: Option<ClientCapabilities>,
}

impl<T: Transport> Server<T> {
    pub async fn new(transport: T) -> Self {
        Self {
            protocol: Protocol::new(transport),
            capabilities: ServerCapabilities::default(),
            // TODO: Pass this in
            server_info: Implementation {
                name: "mcpz".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            client_info: None,
            client_capabilities: None,
        }
    }

    pub async fn start(&mut self) -> Result<(), ServerError> {
        self.protocol.connect().await?;
        Ok(())
    }

    pub async fn initialize(&mut self) -> Result<(), ServerError> {
        let params = InitializeResult {
            server_info: self.server_info.clone(),
            capabilities: self.capabilities.clone(),
        };

        self.protocol
            .send_request::<()>("initialize", serde_json::to_value(params).unwrap())
            .await?;
    }

    pub async fn ping(&mut self) -> Result<(), ServerError> {
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
}
