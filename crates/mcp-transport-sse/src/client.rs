#![cfg(feature = "client")]

use std::future::Future;
use std::pin::Pin;

use futures::TryStreamExt;
use mcp_core::transport::Transport;
use mcp_types::JSONRPCMessage;
use reqwest::Client;
use reqwest_websocket::{Message, RequestBuilderExt};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::channel::{MessageChannel, TokioMpscMessageChannel};
use crate::error::SSETransportError;
use crate::params::SSEClientTransportParams;

type TransportLoop = Pin<Box<dyn Future<Output = Result<(), SSETransportError>> + Send>>;

pub struct SSEClientTransport {
    params: SSEClientTransportParams,
    started: bool,
    send_handle: Option<tokio::task::JoinHandle<TransportLoop>>,
    receive_handle: Option<tokio::task::JoinHandle<TransportLoop>>,
    channel: Option<Box<dyn MessageChannel>>,
    cancel: Option<CancellationToken>,
}

impl SSEClientTransport {
    pub fn new(params: SSEClientTransportParams) -> Result<Self, SSETransportError> {
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
impl Transport for SSEClientTransport {
    type Error = SSETransportError;

    async fn start(&mut self) -> Result<(), Self::Error> {
        if self.started {
            return Err(SSETransportError::AlreadyStarted);
        }

        let (out_tx, out_rx) = tokio::sync::mpsc::channel(100);
        let (in_tx, in_rx) = tokio::sync::mpsc::channel(100);
        let channel = TokioMpscMessageChannel::new(in_rx, out_tx);
        let cancel = CancellationToken::new();

        let http_url = self.params.http_url.clone();
        let http_url = http_url.parse::<Url>().unwrap();
        let send_loop = send_loop(out_rx, http_url, cancel.clone());

        let ws_url = self.params.ws_url.clone();
        let ws_url = ws_url.parse::<Url>().unwrap();
        let receive_loop = receive_loop(in_tx, ws_url, cancel.clone());

        // Start MCP sender
        self.send_handle = Some(tokio::spawn(send_loop));
        // Start MCP receiver
        self.receive_handle = Some(tokio::spawn(receive_loop));

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

        // Join to make sure the threads finish
        let (send_handle, receive_handle) = (
            self.send_handle.take().unwrap(),
            self.receive_handle.take().unwrap(),
        );

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

// Constructor for the future responsible for sending messages to the server
async fn send_loop(
    rx: tokio::sync::mpsc::Receiver<JSONRPCMessage>,
    url: Url,
    cancel: CancellationToken,
) -> TransportLoop {
    Box::pin(async move {
        let mut rx = rx;
        let http_client = reqwest::Client::new();
        loop {
            tokio::select! {
                message = rx.recv() => {
                    if message.is_none() {
                        // The channel is closed
                        break;
                    }

                    // Send the message to the server
                    let response = http_client.post(url.clone()).json(&message).send().await?;
                    if !response.status().is_success() {
                        return Err(SSETransportError::HttpError(
                            response.error_for_status().unwrap_err(),
                        ));
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

// Constructor for the future responsible for receiving messages from the server
async fn receive_loop(
    tx: tokio::sync::mpsc::Sender<JSONRPCMessage>,
    url: Url,
    cancel: CancellationToken,
) -> TransportLoop {
    Box::pin(async move {
        let response = Client::default().get(url.as_str()).upgrade().send().await?;
        let mut websocket = response.into_websocket().await?;
        loop {
            tokio::select! {
                message = websocket.try_next() => handle_receive_message(tx.clone(), message).await?,
                _ = cancel.cancelled() => {
                    break;
                }
            }
        }

        Ok(())
    })
}

async fn handle_receive_message(
    tx: tokio::sync::mpsc::Sender<JSONRPCMessage>,
    message: Result<Option<Message>, reqwest_websocket::Error>,
) -> Result<(), SSETransportError> {
    match message {
        Ok(Some(message)) => {
            // Send the message to the client
            let message = parse_message(message)?;
            tx.send(message).await?;
            Ok(())
        }
        Ok(None) => {
            // The websocket connection is closed
            return Err(SSETransportError::ConnectionClosed(
                "websocket connection closed".to_string(),
            ));
        }
        Err(err) => {
            return Err(SSETransportError::WebsocketError(err));
        }
    }
}

fn parse_message(message: Message) -> Result<JSONRPCMessage, SSETransportError> {
    match message {
        Message::Text(message) => {
            serde_json::from_str(&message).map_err(SSETransportError::SerdeJsonError)
        }
        Message::Binary(message) => {
            serde_json::from_slice(&message).map_err(SSETransportError::SerdeJsonError)
        }
        Message::Close { code, reason } => Err(SSETransportError::ConnectionClosed(format!(
            "websocket connection closed. close code: {}, reason: {}",
            code, reason
        ))),
        // TODO: Handle ping and pong messages
        Message::Ping(_) => Err(SSETransportError::InvalidData(
            "invalid message type".to_string(),
        )),
        Message::Pong(_) => Err(SSETransportError::InvalidData(
            "invalid message type".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use mcp_types::JSONRPCNotification;
    use reqwest_websocket::CloseCode;

    use super::*;

    #[test]
    fn test_parse_message() {
        let json = r#"{"jsonrpc":"2.0","method":"test","params":{}}"#;
        let expected = serde_json::from_str::<JSONRPCMessage>(json).unwrap();
        let message = Message::Text(json.to_string());
        let parsed = parse_message(message).unwrap();
        assert_eq!(parsed, expected);

        let message = Message::Binary(json.as_bytes().to_vec());
        let parsed = parse_message(message).unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_parse_message_error() {
        let message = Message::Text("invalid json".to_string());
        let parsed = parse_message(message).unwrap_err();
        assert_eq!(parsed.to_string(), "serde json error".to_string());

        let message = Message::Binary("invalid json".as_bytes().to_vec());
        let parsed = parse_message(message).unwrap_err();
        assert_eq!(parsed.to_string(), "serde json error".to_string());

        let message = Message::Close {
            code: CloseCode::Normal,
            reason: "".to_string(),
        };
        let parsed = parse_message(message).unwrap_err();
        assert_eq!(parsed.to_string(), "connection closed".to_string());

        let message = Message::Ping(vec![]);
        let parsed = parse_message(message).unwrap_err();
        assert_eq!(parsed.to_string(), "invalid data".to_string());

        let message = Message::Pong(vec![]);
        let parsed = parse_message(message).unwrap_err();
        assert_eq!(parsed.to_string(), "invalid data".to_string());
    }

    #[tokio::test]
    async fn test_handle_message() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        let json = r#"{"jsonrpc":"2.0","method":"test","params":{}}"#;
        let message = Message::Text(json.to_string());
        let result = handle_receive_message(tx, Ok(Some(message))).await;
        assert!(result.is_ok());

        let expected = serde_json::from_str::<JSONRPCMessage>(json).unwrap();
        assert_eq!(rx.recv().await.unwrap(), expected);
    }

    #[tokio::test]
    async fn test_handle_message_error() {
        let (tx, _) = tokio::sync::mpsc::channel(100);
        let result = handle_receive_message(tx.clone(), Ok(None)).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "connection closed".to_string()
        );

        let result = handle_receive_message(
            tx.clone(),
            Err(reqwest_websocket::Error::Handshake(
                reqwest_websocket::HandshakeError::ServerRespondedWithDifferentVersion,
            )),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "websocket error".to_string()
        );

        let result = handle_receive_message(tx, Ok(None)).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "connection closed".to_string()
        );
    }

    #[test]
    fn test_client_transport_constructor() {
        let params = SSEClientTransportParams {
            ws_url: "ws://localhost:8080".parse().unwrap(),
            http_url: "http://localhost:8080".parse().unwrap(),
        };

        let transport = SSEClientTransport::new(params);

        assert!(transport.is_ok());
    }

    #[test]
    fn test_client_transport_constructor_error() {
        let params = SSEClientTransportParams {
            ws_url: "invalid".parse().unwrap(),
            http_url: "invalid".parse().unwrap(),
        };

        let transport = SSEClientTransport::new(params);
        assert!(transport.is_err());
    }

    #[tokio::test]
    async fn test_client_transport_start_and_close() {
        let params = SSEClientTransportParams {
            ws_url: "ws://localhost:8080".parse().unwrap(),
            http_url: "http://localhost:8080".parse().unwrap(),
        };

        let mut transport = SSEClientTransport::new(params).unwrap();

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
    async fn test_client_transport_start_error_already_started() {
        let params = SSEClientTransportParams {
            ws_url: "ws://localhost:8080".parse().unwrap(),
            http_url: "http://localhost:8080".parse().unwrap(),
        };

        let mut transport = SSEClientTransport::new(params).unwrap();

        transport.start().await.unwrap();
        assert!(match transport.start().await {
            Err(SSETransportError::AlreadyStarted) => true,
            _ => false,
        });
    }

    #[tokio::test]
    async fn test_client_transport_close_error_not_started() {
        let params = SSEClientTransportParams {
            ws_url: "ws://localhost:8080".parse().unwrap(),
            http_url: "http://localhost:8080".parse().unwrap(),
        };

        let mut transport = SSEClientTransport::new(params).unwrap();
        assert!(match transport.close().await {
            Err(SSETransportError::NotStarted) => true,
            _ => false,
        });
    }

    #[tokio::test]
    async fn test_client_transport_send_error_not_started() {
        let params = SSEClientTransportParams {
            ws_url: "ws://localhost:8080".parse().unwrap(),
            http_url: "http://localhost:8080".parse().unwrap(),
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
    async fn test_client_transport_receive_error_not_started() {
        let params = SSEClientTransportParams {
            ws_url: "ws://localhost:8080".parse().unwrap(),
            http_url: "http://localhost:8080".parse().unwrap(),
        };

        let mut transport = SSEClientTransport::new(params).unwrap();
        assert!(match transport.receive().await {
            Err(SSETransportError::NotStarted) => true,
            _ => false,
        });
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
    async fn test_client_transport_send_receive() {
        let params = SSEClientTransportParams {
            ws_url: "ws://localhost:8080".parse().unwrap(),
            http_url: "http://localhost:8080".parse().unwrap(),
        };

        let mut transport = SSEClientTransport::new(params).unwrap();

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
}
