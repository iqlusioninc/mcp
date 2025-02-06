//! This module provides the [`Transport`] trait, which is used to send and receive
//! messages between a client and server. Implementors of [`Transport`] are responsible for encoding and
//! decoding messages, as well as transmitting/receiving them.

use mcp_types::JSONRPCMessage;

/// Represents the Transport layer which handles communication
#[async_trait::async_trait]
pub trait Transport: Send + Sync {
    type Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + Sync + 'static;

    /// Starts the transport, including any connection steps that might need to be taken.
    async fn start(&mut self) -> Result<(), Self::Error>;

    /// Closes the connection
    async fn close(&mut self) -> Result<(), Self::Error>;

    /// Sends a message
    async fn send(&mut self, message: JSONRPCMessage) -> Result<(), Self::Error>;

    /// Receives a message
    async fn receive(&mut self) -> Result<JSONRPCMessage, Self::Error>;
}

#[cfg(test)]
pub mod test_utils {
    use mcp_types::JSONRPCNotification;

    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    pub struct MockTransport {
        sent_messages: Arc<Mutex<Vec<JSONRPCMessage>>>,
        should_fail: Mutex<bool>,
    }

    impl MockTransport {
        pub fn new(sent_messages: Arc<Mutex<Vec<JSONRPCMessage>>>) -> Self {
            Self {
                sent_messages,
                should_fail: Mutex::new(false),
            }
        }

        pub fn set_should_fail(&self, should_fail: bool) {
            *self.should_fail.lock().unwrap() = should_fail;
        }

        pub fn get_sent_messages(&self) -> Vec<JSONRPCMessage> {
            self.sent_messages.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl Transport for MockTransport {
        type Error = std::io::Error;

        async fn start(&mut self) -> Result<(), Self::Error> {
            if *self.should_fail.lock().unwrap() {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Mock error"));
            }
            Ok(())
        }

        async fn close(&mut self) -> Result<(), Self::Error> {
            if *self.should_fail.lock().unwrap() {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Mock error"));
            }
            Ok(())
        }

        async fn send(&mut self, message: JSONRPCMessage) -> Result<(), Self::Error> {
            if *self.should_fail.lock().unwrap() {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Mock error"));
            }
            self.sent_messages.lock().unwrap().push(message);
            Ok(())
        }

        async fn receive(&mut self) -> Result<JSONRPCMessage, Self::Error> {
            if *self.should_fail.lock().unwrap() {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Mock error"));
            }

            Ok(JSONRPCMessage::Notification(JSONRPCNotification {
                jsonrpc: "2.0".to_string(),
                method: "test".to_string(),
                params: None,
            }))
        }
    }
}
