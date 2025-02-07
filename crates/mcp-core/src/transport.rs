//! This module provides the [`Transport`] trait, which is used to send and receive
//! messages between a client and server. Implementors of [`Transport`] are responsible for encoding and
//! decoding messages, as well as transmitting/receiving them.

use std::{future::Future, pin::Pin};

use mcp_types::JSONRPCMessage;

use crate::callback::Callback;

pub type CallbackFn<E> = Box<
    dyn FnMut() -> Pin<Box<dyn Future<Output = Result<(), E>> + Send + 'static>> + Send + 'static,
>;
pub type CallbackFnWithArg<T, E> = Box<
    dyn FnMut(T) -> Pin<Box<dyn Future<Output = Result<(), E>> + Send + 'static>> + Send + 'static,
>;

/// Represents the Transport layer which handles communication
#[async_trait::async_trait]
pub trait Transport: Callback<CallbackError = Self::Error> + Send {
    type Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + Sync + 'static;

    /// Starts the transport, including any connection steps that might need to be taken.
    async fn start(&mut self) -> Result<(), Self::Error>;

    /// Sends a message
    async fn send(&mut self, message: JSONRPCMessage) -> Result<(), Self::Error>;

    /// Closes the connection
    async fn close(&mut self) -> Result<(), Self::Error>;
}

#[cfg(test)]
pub mod test_utils {
    use crate::impl_callback;

    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    pub struct MockTransport {
        sent_messages: Arc<Mutex<Vec<JSONRPCMessage>>>,
        should_fail: Mutex<bool>,
        on_close_callback: Option<CallbackFn<std::io::Error>>,
        on_error_callback: Option<CallbackFnWithArg<std::io::Error, std::io::Error>>,
        on_message_callback: Option<CallbackFnWithArg<JSONRPCMessage, std::io::Error>>,
    }

    impl MockTransport {
        pub fn new(sent_messages: Arc<Mutex<Vec<JSONRPCMessage>>>) -> Self {
            Self {
                sent_messages,
                should_fail: Mutex::new(false),
                on_close_callback: None,
                on_error_callback: None,
                on_message_callback: None,
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

            self.on_close().await?;

            Ok(())
        }

        async fn send(&mut self, message: JSONRPCMessage) -> Result<(), Self::Error> {
            if *self.should_fail.lock().unwrap() {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Mock error"));
            }
            self.sent_messages.lock().unwrap().push(message);
            Ok(())
        }
    }

    impl_callback!(MockTransport, std::io::Error);
}
