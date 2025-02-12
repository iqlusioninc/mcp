use mcp_types::{Implementation, ServerCapabilities};

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
}

impl<T: Transport> Server<T> {
    pub async fn new(transport: T) -> Self {
        Self {
            protocol: Protocol::new(transport),
            capabilities: ServerCapabilities::default(),
            server_info: Implementation {
                name: "mcpz".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
        }
    }
}
