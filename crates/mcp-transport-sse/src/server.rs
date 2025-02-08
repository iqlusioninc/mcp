#![cfg(feature = "server")]

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::response::sse::Event;
use axum::routing::get;
use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use futures::Stream;
use mcp_core::callback::Callback;
use mcp_core::impl_callback;
use mcp_core::transport::{CallbackFn, CallbackFnWithArg, Transport};
use mcp_types::JSONRPCMessage;
use tokio_util::sync::CancellationToken;
use tower_http::cors::CorsLayer;

use crate::error::SSETransportError;
use crate::params::SSEServerTransportParams;

pub struct SSEServerTransport {
    params: SSEServerTransportParams,
    on_close_callback: Option<CallbackFn<SSETransportError>>,
    on_error_callback: Option<CallbackFnWithArg<SSETransportError, SSETransportError>>,
    on_message_callback: Option<CallbackFnWithArg<JSONRPCMessage, SSETransportError>>,
    started: bool,
    receive_handle: Option<tokio::task::JoinHandle<Result<(), SSETransportError>>>,
    cancel: Option<CancellationToken>,
    broadcast_tx: Option<tokio::sync::broadcast::Sender<JSONRPCMessage>>,
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
            broadcast_tx: None,
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

        let (broadcast_tx, _) = tokio::sync::broadcast::channel(100);
        self.broadcast_tx = Some(broadcast_tx);

        let addr = self.params.listen_addr.parse().map_err(|e| {
            SSETransportError::InvalidParams(format!("Invalid listen address: {}", e))
        })?;

        self.receive_handle = Some(start_server(
            addr,
            self.broadcast_tx.as_ref().unwrap().clone(),
            cancel.clone(),
        ));

        self.cancel = Some(cancel);
        self.started = true;

        Ok(())
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        if !self.started {
            return Err(SSETransportError::NotStarted);
        }

        self.cancel.take().unwrap().cancel();

        if let Some(handle) = self.receive_handle.take() {
            if let Err(err) = handle.await {
                return Err(SSETransportError::JoinError(err));
            }
        }

        self.started = false;
        self.broadcast_tx = None;

        self.on_close().await?;

        Ok(())
    }

    async fn send(&mut self, message: JSONRPCMessage) -> Result<(), Self::Error> {
        if !self.started {
            return Err(SSETransportError::NotStarted);
        }

        if let Some(tx) = &self.broadcast_tx {
            tx.send(message.clone())
                .map_err(|e| SSETransportError::ChannelClosed(e.to_string()))?;
        }

        self.on_message(message).await?;

        Ok(())
    }
}

fn start_server(
    addr: SocketAddr,
    broadcast_tx: tokio::sync::broadcast::Sender<JSONRPCMessage>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<Result<(), SSETransportError>> {
    let app_state = Arc::new(AppState { broadcast_tx });

    let app = Router::new()
        .route("/", post(handle_message))
        .route("/events", get(handle_sse))
        .with_state(app_state)
        .layer(CorsLayer::permissive());

    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(addr).await?;

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
    broadcast_tx: tokio::sync::broadcast::Sender<JSONRPCMessage>,
}

async fn handle_sse(
    State(state): State<Arc<AppState>>,
) -> axum::response::Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let mut rx = state.broadcast_tx.subscribe();

    let stream = async_stream::stream! {
        while let Ok(msg) = rx.recv().await {
            if let Ok(data) = serde_json::to_string(&msg) {
                yield Ok(Event::default().data(data));
            }
        }
    };

    axum::response::Sse::new(stream)
}

async fn handle_message(
    State(state): State<Arc<AppState>>,
    Json(message): Json<JSONRPCMessage>,
) -> impl IntoResponse {
    if let Err(_) = state.broadcast_tx.send(message) {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to broadcast message",
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
            events_addr: "http://localhost:8080".to_string(),
        };

        let transport = SSEServerTransport::new(params);
        assert!(transport.is_ok());
    }

    #[test]
    fn test_server_sse_transport_constructor_error() {
        let params = SSEServerTransportParams {
            listen_addr: "invalid".to_string(),
            events_addr: "http://localhost:8080".to_string(),
        };

        let transport = SSEServerTransport::new(params);
        assert!(transport.is_err());
    }

    #[tokio::test]
    async fn test_server_sse_transport_start_and_close() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:0".to_string(),
            events_addr: "http://localhost:8080".to_string(),
        };

        let mut transport = SSEServerTransport::new(params).unwrap();

        assert!(transport.start().await.is_ok());
        assert!(transport.cancel.is_some());
        assert!(transport.started);
        assert!(transport.receive_handle.is_some());
        assert!(transport.broadcast_tx.is_some());

        transport.close().await.unwrap();

        assert!(!transport.started);
        assert!(transport.cancel.is_none());
        assert!(transport.receive_handle.is_none());
        assert!(transport.broadcast_tx.is_none());
    }

    #[tokio::test]
    async fn test_server_sse_transport_start_error_already_started() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:0".to_string(),
            events_addr: "http://localhost:8080".to_string(),
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
            events_addr: "http://localhost:8080".to_string(),
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
            events_addr: "http://localhost:8080".to_string(),
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
            events_addr: "http://localhost:8080".to_string(),
        };

        let mut transport = SSEServerTransport::new(params).unwrap();
        transport.start().await.unwrap();

        let message = JSONRPCMessage::Notification(JSONRPCNotification {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: None,
        });

        // Create a subscriber to verify broadcast
        let mut rx = transport.broadcast_tx.as_ref().unwrap().subscribe();

        // Send message
        transport.send(message.clone()).await.unwrap();

        // Verify message was broadcast
        let received = rx.try_recv().unwrap();
        assert_eq!(received, message);

        transport.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_server_sse_transport_set_on_close_callback() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:0".to_string(),
            events_addr: "http://localhost:8080".to_string(),
        };
        let mut transport = SSEServerTransport::new(params).unwrap();
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
    async fn test_server_sse_transport_set_on_error_callback() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:8080".to_string(),
            events_addr: "http://localhost:8080".to_string(),
        };
        let mut transport = SSEServerTransport::new(params).unwrap();
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
    async fn test_server_sse_transport_set_on_message_callback() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:0".to_string(),
            events_addr: "http://localhost:8080".to_string(),
        };
        let mut transport = SSEServerTransport::new(params).unwrap();
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

    #[tokio::test]
    async fn test_server_sse_transport_http_handlers() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:8080".to_string(),
            events_addr: "http://localhost:8080".to_string(),
        };

        let mut transport = SSEServerTransport::new(params).unwrap();
        transport.start().await.unwrap();

        // Create app state for testing handlers directly
        let (tx, _rx) = tokio::sync::broadcast::channel(100);
        let state = Arc::new(AppState { broadcast_tx: tx });

        // Test message handler
        let test_message = JSONRPCMessage::Notification(JSONRPCNotification {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: None,
        });

        let response = handle_message(State(state.clone()), Json(test_message.clone())).await;
        let (status, _) = response.into_response().into_parts();
        assert_eq!(status.status, axum::http::StatusCode::OK);

        transport.close().await.unwrap();
    }
}
