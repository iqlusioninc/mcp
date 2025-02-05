use std::future::Future;
use std::pin::Pin;

use futures::TryStreamExt;
use mcp_core::transport::{ReceiveTransport, Transport};
use mcp_types::JSONRPCMessage;
use reqwest::Client;
use reqwest_websocket::{Message, RequestBuilderExt};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::error::SSEClientTransportError;
use crate::params::SSEClientParams;

type TransportLoop = Pin<Box<dyn Future<Output = Result<(), SSEClientTransportError>> + Send>>;

pub struct SSEClientTransport {
    params: SSEClientParams,
    started: bool,
    send_handle: Option<tokio::task::JoinHandle<TransportLoop>>,
    receive_handle: Option<tokio::task::JoinHandle<TransportLoop>>,
    in_rx: Option<tokio::sync::mpsc::Receiver<JSONRPCMessage>>,
    out_tx: Option<tokio::sync::mpsc::Sender<JSONRPCMessage>>,
    cancel: Option<CancellationToken>,
}

impl SSEClientTransport {
    pub fn new(params: SSEClientParams) -> Self {
        Self {
            params,
            started: false,
            send_handle: None,
            receive_handle: None,
            in_rx: None,
            out_tx: None,
            cancel: None,
        }
    }
}

#[async_trait::async_trait]
impl Transport for SSEClientTransport {
    type Error = SSEClientTransportError;

    async fn start(&mut self) -> Result<(), Self::Error> {
        if self.started {
            return Err(SSEClientTransportError::AlreadyStarted);
        }

        let (out_tx, out_rx) = tokio::sync::mpsc::channel(100);
        let (in_tx, in_rx) = tokio::sync::mpsc::channel(100);
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

        self.in_rx = Some(in_rx);
        self.out_tx = Some(out_tx);
        self.cancel = Some(cancel);
        self.started = true;

        Ok(())
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        if !self.started {
            return Err(SSEClientTransportError::NotStarted);
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
            return Err(SSEClientTransportError::JoinError(err));
        }

        self.in_rx = None;
        self.out_tx = None;

        Ok(())
    }

    async fn send(&mut self, message: JSONRPCMessage) -> Result<(), Self::Error> {
        if !self.started {
            return Err(SSEClientTransportError::NotStarted);
        }

        self.out_tx.as_ref().unwrap().send(message).await?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl ReceiveTransport for SSEClientTransport {
    async fn receive(&mut self) -> Result<JSONRPCMessage, Self::Error> {
        if !self.started {
            return Err(SSEClientTransportError::NotStarted);
        }

        let in_rx = self.in_rx.as_mut().unwrap();

        match in_rx.recv().await {
            Some(message) => Ok(message),
            None => Err(SSEClientTransportError::ChannelClosed(
                "in channel closed unexpectedly".to_string(),
            )),
        }
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
                        return Err(SSEClientTransportError::HttpError(
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
                message = websocket.try_next() => {
                    match message {
                        Ok(Some(message)) => {
                            // Send the message to the client
                            let message = parse_message(message)?;
                            tx.send(message).await?;
                        }
                        Ok(None) => {
                            // The websocket connection is closed
                            return Err(SSEClientTransportError::ConnectionClosed("websocket connection closed".to_string()));
                        }
                        Err(err) => {
                            return Err(SSEClientTransportError::WebsocketError(err));
                        }
                    };
                }
                _ = cancel.cancelled() => {
                    break;
                }
            }
        }

        Ok(())
    })
}

fn parse_message(message: Message) -> Result<JSONRPCMessage, SSEClientTransportError> {
    match message {
        Message::Text(message) => {
            serde_json::from_str(&message).map_err(SSEClientTransportError::SerdeJsonError)
        }
        Message::Binary(message) => {
            serde_json::from_slice(&message).map_err(SSEClientTransportError::SerdeJsonError)
        }
        Message::Close { code, reason } => Err(SSEClientTransportError::ConnectionClosed(format!(
            "websocket connection closed. close code: {}, reason: {}",
            code, reason
        ))),
        // TODO: Handle ping and pong messages
        Message::Ping(_) => Err(SSEClientTransportError::InvalidData(
            "invalid message type".to_string(),
        )),
        Message::Pong(_) => Err(SSEClientTransportError::InvalidData(
            "invalid message type".to_string(),
        )),
    }
}
