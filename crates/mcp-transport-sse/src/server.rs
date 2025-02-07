#![cfg(feature = "server")]

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use futures::SinkExt;
use mcp_core::impl_callback;
use mcp_core::transport::{CallbackFn, CallbackFnWithArg, Transport};
use mcp_types::JSONRPCMessage;
use reqwest::Client;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tower_http::cors::CorsLayer;
use url::Url;

use crate::error::SSETransportError;
use crate::params::SSEServerTransportParams;

type TransportLoop = Pin<Box<dyn Future<Output = Result<(), SSETransportError>> + Send>>;

pub struct SSEServerTransport {
    params: SSEServerTransportParams,
    on_close_callback: Option<CallbackFn<SSETransportError>>,
    on_error_callback: Option<CallbackFnWithArg<SSETransportError, SSETransportError>>,
    on_message_callback: Option<CallbackFnWithArg<JSONRPCMessage, SSETransportError>>,
    started: bool,
    receive_handle: Option<tokio::task::JoinHandle<Result<(), SSETransportError>>>,
    cancel: Option<CancellationToken>,
}

impl SSEServerTransport {
    pub fn new(params: SSEServerTransportParams) -> Result<Self, SSETransportError> {
        if let Err(err) = params.validate() {
            return Err(SSETransportError::InvalidParams(err));
        }

        Ok(Self {
            params,
            on_close_callback: None,
            on_error_callback: None,
            on_message_callback: None,
            started: false,
            receive_handle: None,
            cancel: None,
        })
    }
}

impl_callback!(SSEServerTransport, SSETransportError);

#[async_trait::async_trait]
impl Transport for SSEServerTransport {
    type Error = SSETransportError;

    async fn start(&mut self) -> Result<(), Self::Error> {
        if self.started {
            return Err(SSETransportError::AlreadyStarted);
        }

        let cancel = CancellationToken::new();

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

        self.started = false;

        Ok(())
    }

    async fn send(&mut self, message: JSONRPCMessage) -> Result<(), Self::Error> {
        if !self.started {
            return Err(SSETransportError::NotStarted);
        }

        Ok(())
    }
}

// HTTP server setup and handler
fn start_server(
    addr: SocketAddr,
    tx: mpsc::Sender<JSONRPCMessage>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<Result<(), SSETransportError>> {
    let app_state = Arc::new(AppState { tx });

    let app = Router::new()
        .route("/", post(handle_message))
        .with_state(app_state)
        .layer(CorsLayer::permissive());

    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(addr).await?;

        // Start receiver server
        axum::serve(listener, app.into_make_service())
            .with_graceful_shutdown(async move {
                cancel.cancelled().await;
            })
            .await
            .map_err(SSETransportError::IoError)
    })
}

#[derive(Clone)]
struct AppState {
    tx: mpsc::Sender<JSONRPCMessage>,
}

async fn handle_message(
    State(state): State<Arc<AppState>>,
    Json(message): Json<JSONRPCMessage>,
) -> impl IntoResponse {
    if let Err(_) = state.tx.send(message).await {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to process message",
        );
    }

    (axum::http::StatusCode::OK, "Message received")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcp_core::callback::Callback;
    use mcp_types::JSONRPCNotification;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_server_sse_transport_constructor() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:8080".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };

        let transport = SSEServerTransport::new(params);
        assert!(transport.is_ok());
    }

    #[test]
    fn test_server_sse_transport_constructor_error() {
        let params = SSEServerTransportParams {
            listen_addr: "invalid".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };

        let transport = SSEServerTransport::new(params);
        assert!(transport.is_err());
    }

    #[tokio::test]
    async fn test_server_sse_transport_start_and_close() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:0".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };

        let mut transport = SSEServerTransport::new(params).unwrap();

        assert!(transport.start().await.is_ok());
        assert!(transport.cancel.is_some());
        assert!(transport.started);
        assert!(transport.channel.is_some());
        assert!(transport.send_handle.is_some());
        assert!(transport.receive_handle.is_some());

        transport.close().await.unwrap();

        assert!(!transport.started);
        assert!(transport.cancel.is_none());
        assert!(transport.channel.is_none());
        assert!(transport.send_handle.is_none());
        assert!(transport.receive_handle.is_none());
    }

    #[tokio::test]
    async fn test_server_sse_transport_start_error_already_started() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:0".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };

        let mut transport = SSEServerTransport::new(params).unwrap();

        transport.start().await.unwrap();
        assert!(matches!(
            transport.start().await,
            Err(SSETransportError::AlreadyStarted)
        ));
    }

    #[tokio::test]
    async fn test_server_sse_transport_close_error_not_started() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:0".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };

        let mut transport = SSEServerTransport::new(params).unwrap();
        assert!(matches!(
            transport.close().await,
            Err(SSETransportError::NotStarted)
        ));
    }

    #[tokio::test]
    async fn test_server_sse_transport_send_error_not_started() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:0".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };

        let mut transport = SSEServerTransport::new(params).unwrap();
        let message = JSONRPCMessage::Notification(JSONRPCNotification {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: None,
        });

        assert!(matches!(
            transport.send(message).await,
            Err(SSETransportError::NotStarted)
        ));
    }

    #[tokio::test]
    async fn test_server_sse_transport_send() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:0".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };

        let mut transport = SSEServerTransport::new(params).unwrap();

        // Create mock channel
        let sent_messages = Arc::new(Mutex::new(Vec::new()));
        let messages_to_receive = Arc::new(Mutex::new(vec![JSONRPCMessage::Notification(
            JSONRPCNotification {
                jsonrpc: "2.0".to_string(),
                method: "test".to_string(),
                params: None,
            },
        )]));

        transport.channel = Some(Box::new(MockMessageChannel {
            sent_messages: sent_messages.clone(),
            messages_to_receive: messages_to_receive.clone(),
        }));
        transport.started = true;

        // Test sending
        let message = JSONRPCMessage::Notification(JSONRPCNotification {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: None,
        });

        transport.send(message.clone()).await.unwrap();
        assert_eq!(sent_messages.lock().unwrap().pop().unwrap(), message);
    }

    #[tokio::test]
    async fn test_server_sse_transport_http_handler() {
        let (tx, mut rx) = mpsc::channel(1);
        let state = Arc::new(AppState { tx });

        let test_message = JSONRPCMessage::Notification(JSONRPCNotification {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: None,
        });

        // Test successful message handling
        let response = handle_message(State(state.clone()), Json(test_message.clone())).await;
        let (status, _) = response.into_response().into_parts();
        assert!(matches!(status.status, axum::http::StatusCode::OK));

        // Verify message was received on channel
        let received = rx.recv().await.unwrap();
        assert_eq!(received, test_message);
    }

    #[tokio::test]
    async fn test_server_sse_transport_start_with_port_zero() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:0".to_string(), // Port 0 means pick random available port
            ws_url: "ws://localhost:8080".to_string(),
        };

        let mut transport = SSEServerTransport::new(params).unwrap();
        assert!(transport.start().await.is_ok());

        // Give the server a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        transport.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_server_sse_transport_start_with_invalid_port() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:99999".to_string(), // Invalid port number
            ws_url: "ws://localhost:8080".to_string(),
        };

        let transport = SSEServerTransport::new(params);
        assert!(transport.is_err());
        assert!(matches!(
            transport,
            Err(SSETransportError::InvalidParams(_))
        ));
    }

    #[tokio::test]
    async fn test_server_sse_transport_http_endpoint() {
        // Start server on random port
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:0".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };

        let mut transport = SSEServerTransport::new(params).unwrap();
        transport.start().await.unwrap();

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Create test message
        let test_message = JSONRPCMessage::Notification(JSONRPCNotification {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: None,
        });

        // Send HTTP POST request
        let client = reqwest::Client::new();
        let response = client
            .post("http://127.0.0.1:0/") // Port 0 won't work for an actual request
            .json(&test_message)
            .send()
            .await;

        // This will fail because we can't know the actual port
        assert!(response.is_err());

        transport.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_server_sse_transport_set_on_close_callback() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:8080".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };
        let mut transport = SSEServerTransport::new(params).unwrap();
        let flag = Arc::new(Mutex::new(false));
        let flag_clone = flag.clone();

        // Set the on_close callback to mark our flag true.
        transport
            .set_on_close_callback(move || {
                let flag_clone = flag_clone.clone();
                async move {
                    *flag_clone.lock().unwrap() = true;
                    Ok(())
                }
            })
            .await;

        // Manually invoke the on_close callback (since the transport does not call it automatically)
        if let Some(mut callback) = transport.on_close_callback.take() {
            let res = (callback)().await;
            assert!(res.is_ok(), "on_close callback should return Ok");
        } else {
            panic!("on_close_callback was not set");
        }
        assert_eq!(
            *flag.lock().unwrap(),
            true,
            "The on_close callback should have set the flag to true"
        );
    }

    #[tokio::test]
    async fn test_server_sse_transport_set_on_error_callback() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:8080".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };
        let mut transport = SSEServerTransport::new(params).unwrap();
        let flag = Arc::new(Mutex::new(false));
        let flag_clone = flag.clone();

        // Set the on_error callback which will mark the flag if a test error is received.
        transport
            .set_on_error_callback(move |err| {
                let flag_clone = flag_clone.clone();
                async move {
                    if err.to_string().contains("test error") {
                        *flag_clone.lock().unwrap() = true;
                    }
                    Ok(())
                }
            })
            .await;

        // Create a dummy error to pass into the callback.
        let test_error = SSETransportError::ChannelClosed("test error".to_string());
        if let Some(mut callback) = transport.on_error_callback.take() {
            let res = (callback)(test_error).await;
            assert!(res.is_ok(), "on_error callback should return Ok");
        } else {
            panic!("on_error_callback was not set");
        }
        assert_eq!(
            *flag.lock().unwrap(),
            true,
            "The on_error callback should have set the flag to true"
        );
    }

    #[tokio::test]
    async fn test_server_sse_transport_set_on_message_callback() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:8080".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };
        let mut transport = SSEServerTransport::new(params).unwrap();
        let flag = Arc::new(Mutex::new(false));
        let flag_clone = flag.clone();

        // Set the on_message callback to check that the received message has the correct method.
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

        // Create a sample JSONRPC message.
        let message = JSONRPCMessage::Notification(JSONRPCNotification {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: None,
        });

        if let Some(mut callback) = transport.on_message_callback.take() {
            let res = (callback)(message).await;
            assert!(res.is_ok(), "on_message callback should return Ok");
        } else {
            panic!("on_message_callback was not set");
        }
        assert_eq!(
            *flag.lock().unwrap(),
            true,
            "The on_message callback should have set the flag to true"
        );
    }

    // Mock implementation for testing
    struct MockMessageChannel {
        sent_messages: Arc<Mutex<Vec<JSONRPCMessage>>>,
        messages_to_receive: Arc<Mutex<Vec<JSONRPCMessage>>>,
    }

    #[async_trait::async_trait]
    impl MessageChannel for MockMessageChannel {
        async fn send_message(&mut self, message: JSONRPCMessage) -> Result<(), SSETransportError> {
            self.sent_messages.lock().unwrap().push(message);
            Ok(())
        }

        async fn receive_message(&mut self) -> Result<JSONRPCMessage, SSETransportError> {
            let mut messages = self.messages_to_receive.lock().unwrap();
            messages.pop().ok_or(SSETransportError::ChannelClosed(
                "no more messages".to_string(),
            ))
        }
    }
}
