#![cfg(feature = "server")]
use std::io::{BufRead, Write};

use mcp_core::transport::Transport;
use mcp_types::JSONRPCMessage;

pub struct StdioServerTransport {
    started: bool,
}

impl StdioServerTransport {
    pub fn new() -> Self {
        Self { started: false }
    }
}

#[async_trait::async_trait]
impl Transport for StdioServerTransport {
    type Error = std::io::Error;

    async fn start(&mut self) -> Result<(), Self::Error> {
        if self.started {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "transport already started",
            ));
        }

        self.started = true;

        Ok(())
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        if !self.started {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "transport not started",
            ));
        }

        self.started = false;

        Ok(())
    }

    async fn send(&mut self, message: JSONRPCMessage) -> Result<(), Self::Error> {
        if !self.started {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "transport not started",
            ));
        }

        let message = serde_json::to_string(&message).unwrap();
        let mut writer = std::io::BufWriter::new(std::io::stdout());

        writer.write_all(message.as_bytes())?;
        writer.flush()?;

        Ok(())
    }

    async fn receive(&mut self) -> Result<JSONRPCMessage, Self::Error> {
        if !self.started {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "transport not started",
            ));
        }

        let mut buffer = String::new();
        let mut reader = std::io::BufReader::new(std::io::stdin());

        reader.read_line(&mut buffer)?;

        let message = serde_json::from_str(&buffer)?;

        Ok(message)
    }
}
