use std::future::Future;
use std::pin::Pin;

use mcp_core::transport::{ReceiveTransport, Transport};
use mcp_types::JSONRPCMessage;
use tokio_util::sync::CancellationToken;

use crate::error::SSEClientTransportError;
use crate::params::SSEClientParams;

pub struct SSEClientTransport {
    params: SSEClientParams,
    started: bool,
    send_handle: Option<
        tokio::task::JoinHandle<
            Pin<Box<dyn Future<Output = Result<(), SSEClientTransportError>> + Send>>,
        >,
    >,
    receive_handle: Option<
        tokio::task::JoinHandle<
            Pin<Box<dyn Future<Output = Result<(), SSEClientTransportError>> + Send>>,
        >,
    >,
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
        let send_loop = send_loop(out_rx, cancel.clone());
        let receive_loop = receive_loop(in_tx, cancel.clone());

        self.send_handle = Some(tokio::spawn(send_loop));
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

async fn send_loop(
    rx: tokio::sync::mpsc::Receiver<JSONRPCMessage>,
    cancel: CancellationToken,
) -> Pin<Box<dyn Future<Output = Result<(), SSEClientTransportError>> + Send>> {
    Box::pin(async move {
        loop {
            if cancel.is_cancelled() {
                break;
            }
        }

        Ok(())
    })
}

async fn receive_loop(
    tx: tokio::sync::mpsc::Sender<JSONRPCMessage>,
    cancel: CancellationToken,
) -> Pin<Box<dyn Future<Output = Result<(), SSEClientTransportError>> + Send>> {
    Box::pin(async move {
        loop {
            if cancel.is_cancelled() {
                break;
            }
        }

        Ok(())
    })
}
