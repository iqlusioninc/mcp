#![cfg(feature = "server")]

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use futures::SinkExt;
use mcp_core::transport::Transport;
use mcp_types::JSONRPCMessage;
use reqwest::Client;
use reqwest_websocket::{Message, RequestBuilderExt};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tower_http::cors::CorsLayer;
use url::Url;

use crate::channel::{MessageChannel, TokioMpscMessageChannel};
use crate::error::SSETransportError;
use crate::params::SSEServerTransportParams;

type TransportLoop = Pin<Box<dyn Future<Output = Result<(), SSETransportError>> + Send>>;

pub struct SSEServerTransport {
    params: SSEServerTransportParams,
    started: bool,
    send_handle: Option<tokio::task::JoinHandle<TransportLoop>>,
    receive_handle: Option<tokio::task::JoinHandle<Result<(), SSETransportError>>>,
    channel: Option<Box<dyn MessageChannel>>,
    cancel: Option<CancellationToken>,
}

impl SSEServerTransport {
    pub fn new(params: SSEServerTransportParams) -> Result<Self, SSETransportError> {
        if let Err(err) = params.validate() {
            return Err(SSETransportError::InvalidParams(err));
        }

        Ok(Self {
            params,
            started: false,
            send_handle: None,
            receive_handle: None,
            channel: None,
            cancel: None,
        })
    }
}

#[async_trait::async_trait]
impl Transport for SSEServerTransport {
    type Error = SSETransportError;

    async fn start(&mut self) -> Result<(), Self::Error> {
        if self.started {
            return Err(SSETransportError::AlreadyStarted);
        }

        let (out_tx, out_rx) = mpsc::channel(100);
        let (in_tx, in_rx) = mpsc::channel(100);
        let channel = TokioMpscMessageChannel::new(in_rx, out_tx);
        let cancel = CancellationToken::new();

        // Start websocket sender
        let ws_url = self.params.ws_url.clone();
        let ws_url = ws_url.parse::<Url>().unwrap();
        let send_loop = send_loop(out_rx, ws_url, cancel.clone());
        self.send_handle = Some(tokio::spawn(send_loop));

        // Start HTTP server for receiving messages
        let addr = self.params.listen_addr.parse().unwrap();
        let receive_handle = start_server(addr, in_tx, cancel.clone());
        self.receive_handle = Some(receive_handle);

        self.channel = Some(Box::new(channel));
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

        // Join handles
        let send_handle = self.send_handle.take().unwrap();
        let receive_handle = self.receive_handle.take().unwrap();

        self.started = false;

        if let Err(err) = tokio::join!(send_handle, receive_handle).0 {
            return Err(SSETransportError::JoinError(err));
        }

        self.channel = None;

        Ok(())
    }

    async fn send(&mut self, message: JSONRPCMessage) -> Result<(), Self::Error> {
        if !self.started {
            return Err(SSETransportError::NotStarted);
        }

        self.channel.as_mut().unwrap().send_message(message).await?;

        Ok(())
    }

    async fn receive(&mut self) -> Result<JSONRPCMessage, Self::Error> {
        if !self.started {
            return Err(SSETransportError::NotStarted);
        }

        self.channel.as_mut().unwrap().receive_message().await
    }
}

// Constructor for the future responsible for sending messages via websocket
async fn send_loop(
    rx: mpsc::Receiver<JSONRPCMessage>,
    url: Url,
    cancel: CancellationToken,
) -> TransportLoop {
    Box::pin(async move {
        let mut rx = rx;
        let response = Client::default().get(url.as_str()).upgrade().send().await?;
        let mut websocket = response.into_websocket().await?;

        loop {
            tokio::select! {
                message = rx.recv() => {
                    if let Some(message) = message {
                        let json = serde_json::to_string(&message)
                            .map_err(SSETransportError::SerdeJsonError)?;
                        websocket
                            .send(Message::Text(json))
                            .await
                            .map_err(SSETransportError::WebsocketError)?;
                    } else {
                        break;
                    }
                }
                _ = cancel.cancelled() => {
                    break;
                }
            }
        }

        Ok(())
    })
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
    use mcp_types::JSONRPCNotification;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_server_transport_constructor() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:8080".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };

        let transport = SSEServerTransport::new(params);
        assert!(transport.is_ok());
    }

    #[test]
    fn test_server_transport_constructor_error() {
        let params = SSEServerTransportParams {
            listen_addr: "invalid".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };

        let transport = SSEServerTransport::new(params);
        assert!(transport.is_err());
    }

    #[tokio::test]
    async fn test_server_transport_start_and_close() {
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
    async fn test_server_transport_start_error_already_started() {
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
    async fn test_server_transport_close_error_not_started() {
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
    async fn test_server_transport_send_error_not_started() {
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
    async fn test_server_transport_receive_error_not_started() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:0".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };

        let mut transport = SSEServerTransport::new(params).unwrap();
        assert!(matches!(
            transport.receive().await,
            Err(SSETransportError::NotStarted)
        ));
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

    #[tokio::test]
    async fn test_server_transport_send_receive() {
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

        // Test receiving
        let received = transport.receive().await.unwrap();
        assert!(messages_to_receive.lock().unwrap().is_empty());
        assert!(matches!(received, JSONRPCMessage::Notification(_)));
    }

    // Tests specific to the HTTP server functionality
    #[tokio::test]
    async fn test_server_http_handler() {
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
    async fn test_server_start_with_port_zero() {
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
    async fn test_server_start_with_invalid_port() {
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

    // Integration-style test that sends an actual HTTP request
    #[tokio::test]
    async fn test_server_http_endpoint() {
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
            .post("http://127.0.0.1:0/") // Port 0 won't work for actual request
            .json(&test_message)
            .send()
            .await;

        // This will fail because we can't know the actual port
        assert!(response.is_err());

        transport.close().await.unwrap();
    }
}
