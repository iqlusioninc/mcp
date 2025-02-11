use mcp_types::{Implementation, MCPError, Request, RequestParams};

use crate::{error::Error, protocol::Protocol, transport::Transport};

pub struct Client<T: Transport> {
    implementation: Implementation,
    protocol: Option<Protocol<T, T::Error>>,
}

impl<T: Transport> Client<T> {
    pub fn new(implementation: Implementation) -> Self {
        Self {
            implementation,
            protocol: None,
        }
    }

    pub async fn connect(&mut self, transport: T) -> Result<(), Error> {
        self.protocol = Some(Protocol::new());
        self.protocol.as_mut().unwrap().connect(transport).await?;

        // Being initialization handshake
        let response = self
            .protocol
            .as_mut()
            .unwrap()
            .send_request(Request {
                method: "initialize".to_string(),
                params: Some(RequestParams { meta: None }),
            })
            .await?;

        Ok(())
    }
}
