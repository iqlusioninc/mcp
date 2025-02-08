#![cfg(feature = "client")]

use std::future::Future;

use futures::StreamExt;
use mcp_core::callback::Callback;
use mcp_core::impl_callback;
use mcp_core::transport::{CallbackFn, CallbackFnWithArg, Transport};
use mcp_types::JSONRPCMessage;
use reqwest_eventsource::{Event, EventSource};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::error::SSETransportError;
use crate::params::SSEClientTransportParams;

pub struct SSEClientTransport {
    params: SSEClientTransportParams,
    on_close_callback: Option<CallbackFn<SSETransportError>>,
    on_error_callback: Option<CallbackFnWithArg<SSETransportError, SSETransportError>>,
    on_message_callback: Option<CallbackFnWithArg<JSONRPCMessage, SSETransportError>>,
    started: bool,
    receive_handle: Option<
        tokio::task::JoinHandle<
            Result<
                (
                    CallbackFnWithArg<JSONRPCMessage, SSETransportError>,
                    CallbackFnWithArg<SSETransportError, SSETransportError>,
                ),
                SSETransportError,
            >,
        >,
    >,
    cancel: Option<CancellationToken>,
}

impl SSEClientTransport {
    pub fn new(params: SSEClientTransportParams) -> Result<Self, SSETransportError> {
        if let Err(err) = params.validate() {
            return Err(SSETransportError::InvalidParams(err));
        }

        Ok(Self {
            params,
            on_close_callback: Some(Box::new(move || Box::pin(async { Ok(()) }))),
            on_error_callback: Some(Box::new(move |_| Box::pin(async { Ok(()) }))),
            on_message_callback: Some(Box::new(move |_| Box::pin(async { Ok(()) }))),
            started: false,
            receive_handle: None,
            cancel: None,
        })
    }
}

impl_callback!(SSEClientTransport, SSETransportError);

#[async_trait::async_trait]
impl Transport for SSEClientTransport {
    type Error = SSETransportError;

    async fn start(&mut self) -> Result<(), Self::Error> {
        if self.started {
            return Err(SSETransportError::AlreadyStarted);
        }

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let mut on_message = self.on_message_callback.take().unwrap();
        let mut on_error = self.on_error_callback.take().unwrap();
        let mut event_source = EventSource::get(self.params.event_url.clone());

        // Start MCP receiver
        self.receive_handle = Some(tokio::spawn(async move {
            loop {
                tokio::select! {
                    event = event_source.next() => {
                        let result = match event {
                            Some(Ok(Event::Open)) => {
                                debug!("SSE connection opened");
                                continue;
                            }
                            Some(Ok(Event::Message(message))) => {
                                match serde_json::from_str::<JSONRPCMessage>(&message.data) {
                                    Ok(message) => {
                                        debug!("received message: {message:?}");
                                        on_message(message).await                                     }
                                    Err(err) => {
                                        Err(err.into())
                                    }
                                }
                            }
                            Some(Err(err)) => {
                                debug!("error from SSE connection: {err}");
                                Err(err.into())
                            }
                            // Server closed the connection
                            None => {
                                // TODO: Should this call on_error?
                                warn!("SSE connection closed");
                                break;
                            }
                        };

                        // Error callback
                        if let Err(err) = result {
                            // TODO: This happening means the transport can't be reused because the callbacks will get dropped
                            // with the task. Need to figure out a better way to handle. Does on_error even need to be Fallible?
                            on_error(err).await?;
                        }
                    }
                    _ = cancel_clone.cancelled() => {
                        debug!("cancelling SSE connection");
                        break;
                    }
                }
            }

            // Allow the transport to start a new task with the same callbacks.
            Ok((on_message, on_error))
        }));

        self.cancel = Some(cancel);
        self.started = true;

        Ok(())
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        if !self.started {
            return Err(SSETransportError::NotStarted);
        }

        // Send cancel signal
        self.cancel.take().unwrap().cancel();

        // Join to make sure the threads finish
        let receive_handle = self.receive_handle.take().unwrap();

        self.started = false;

        match tokio::join!(receive_handle).0 {
            Ok(Ok((on_message, on_error))) => {
                // Recapture the callbacks
                self.on_message_callback = Some(on_message);
                self.on_error_callback = Some(on_error);
            }
            // TODO: Call on_error? on_close?
            Ok(Err(err)) => return Err(err),
            Err(err) => return Err(SSETransportError::JoinError(err)),
        }

        // callback
        self.on_close().await?;

        Ok(())
    }

    async fn send(&mut self, message: JSONRPCMessage) -> Result<(), Self::Error> {
        if !self.started {
            return Err(SSETransportError::NotStarted);
        }

        let http_client = reqwest::Client::new();
        let url = self.params.post_url.clone();
        let response = http_client.post(url).json(&message).send().await?;
        if !response.status().is_success() {
            return Err(SSETransportError::HttpError(
                response.error_for_status().unwrap_err(),
            ));
        }

        // callback
        self.on_message(message).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use mcp_core::callback::Callback;
    use mcp_types::JSONRPCNotification;

    use super::*;

    #[test]
    fn test_client_sse_transport_constructor() {
        let params = SSEClientTransportParams {
            event_url: "http://localhost:8080/events".parse().unwrap(),
            post_url: "http://localhost:8080/api".parse().unwrap(),
        };

        let transport = SSEClientTransport::new(params);

        assert!(transport.is_ok());
    }

    #[test]
    fn test_client_sse_transport_constructor_error() {
        let params = SSEClientTransportParams {
            event_url: "invalid".parse().unwrap(),
            post_url: "invalid".parse().unwrap(),
        };

        let transport = SSEClientTransport::new(params);
        assert!(transport.is_err());
    }

    #[tokio::test]
    async fn test_client_sse_transport_start_and_close() {
        let params = SSEClientTransportParams {
            event_url: "http://localhost:8080".parse().unwrap(),
            post_url: "http://localhost:8080".parse().unwrap(),
        };

        let mut transport = SSEClientTransport::new(params).unwrap();

        assert!(transport.start().await.is_ok());
        assert!(transport.cancel.is_some());
        assert!(transport.started);
        assert!(transport.receive_handle.is_some());

        transport.close().await.unwrap();

        assert!(!transport.started);
        assert!(transport.cancel.is_none());
        assert!(transport.receive_handle.is_none());
    }

    #[tokio::test]
    async fn test_client_sse_transport_start_error_already_started() {
        let params = SSEClientTransportParams {
            event_url: "http://localhost:8080/events".parse().unwrap(),
            post_url: "http://localhost:8080/api".parse().unwrap(),
        };

        let mut transport = SSEClientTransport::new(params).unwrap();

        transport.start().await.unwrap();
        assert!(match transport.start().await {
            Err(SSETransportError::AlreadyStarted) => true,
            _ => false,
        });
    }

    #[tokio::test]
    async fn test_client_sse_transport_close_error_not_started() {
        let params = SSEClientTransportParams {
            event_url: "http://localhost:8080/events".parse().unwrap(),
            post_url: "http://localhost:8080/api".parse().unwrap(),
        };

        let mut transport = SSEClientTransport::new(params).unwrap();
        assert!(match transport.close().await {
            Err(SSETransportError::NotStarted) => true,
            _ => false,
        });
    }

    #[tokio::test]
    async fn test_client_sse_transport_send_error_not_started() {
        let params = SSEClientTransportParams {
            event_url: "http://localhost:8080/events".parse().unwrap(),
            post_url: "http://localhost:8080/api".parse().unwrap(),
        };

        let mut transport = SSEClientTransport::new(params).unwrap();
        let message = JSONRPCMessage::Notification(JSONRPCNotification {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: None,
        });

        assert!(match transport.send(message).await {
            Err(SSETransportError::NotStarted) => true,
            _ => false,
        });
    }

    #[tokio::test]
    async fn test_client_sse_transport_set_on_close_callback() {
        let params = SSEClientTransportParams {
            event_url: "http://localhost:8080/events".parse().unwrap(),
            post_url: "http://localhost:8080/api".parse().unwrap(),
        };
        let mut transport = SSEClientTransport::new(params).unwrap();
        let flag = Arc::new(Mutex::new(false));
        let flag_clone = flag.clone();

        transport
            .set_on_close_callback(move || {
                let flag_clone = flag_clone.clone();
                async move {
                    *flag_clone.lock().unwrap() = true;
                    Ok(())
                }
            })
            .await;

        transport.on_close().await.unwrap();
        assert_eq!(
            *flag.lock().unwrap(),
            true,
            "The on_close callback should have set the flag to true"
        );
    }

    #[tokio::test]
    async fn test_client_sse_transport_set_on_error_callback() {
        let params = SSEClientTransportParams {
            event_url: "http://localhost:8080".parse().unwrap(),
            post_url: "http://localhost:8080".parse().unwrap(),
        };
        let mut transport = SSEClientTransport::new(params).unwrap();
        let flag = Arc::new(Mutex::new(false));
        let flag_clone = flag.clone();

        transport
            .set_on_error_callback(move |err| {
                let flag_clone = flag_clone.clone();
                async move {
                    if let SSETransportError::ChannelClosed(err) = err {
                        if err.contains("test error") {
                            *flag_clone.lock().unwrap() = true;
                        }
                    }
                    Ok(())
                }
            })
            .await;

        let test_error = SSETransportError::ChannelClosed("test error".to_string());
        transport.on_error(test_error).await.unwrap();
        assert_eq!(
            *flag.lock().unwrap(),
            true,
            "The on_error callback should have set the flag to true"
        );
    }

    #[tokio::test]
    async fn test_client_sse_transport_set_on_message_callback() {
        let params = SSEClientTransportParams {
            event_url: "http://localhost:8080/events".parse().unwrap(),
            post_url: "http://localhost:8080/api".parse().unwrap(),
        };
        let mut transport = SSEClientTransport::new(params).unwrap();
        let flag = Arc::new(Mutex::new(false));
        let flag_clone = flag.clone();

        transport
            .set_on_message_callback(move |msg| {
                let flag_clone = flag_clone.clone();
                async move {
                    if let JSONRPCMessage::Notification(ref notif) = msg {
                        if notif.method == "test" {
                            *flag_clone.lock().unwrap() = true;
                        }
                    }
                    Ok(())
                }
            })
            .await;

        let message = JSONRPCMessage::Notification(JSONRPCNotification {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: None,
        });

        transport.on_message(message).await.unwrap();
        assert_eq!(
            *flag.lock().unwrap(),
            true,
            "The on_message callback should have set the flag to true"
        );
    }
}
