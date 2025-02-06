use mcp_core::protocol::Protocol;
use mcp_transport_sse::{client::SSEClientTransport, error::SSETransportError};

pub struct SSEClient {
    protocol: Protocol<SSETransportError>,
}

impl SSEClient {
    pub fn new(transport: SSEClientTransport) -> Self {
        Self {
            protocol: Protocol::new(Box::new(transport)),
        }
    }
}
