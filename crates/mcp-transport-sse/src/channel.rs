//! Channel implementation for SSE transport. Primarily exists for testing.

use mcp_types::JSONRPCMessage;

use crate::error::SSEClientTransportError;

#[async_trait::async_trait]
pub(crate) trait MessageChannel: Send + Sync {
    async fn send_message(
        &mut self,
        message: JSONRPCMessage,
    ) -> Result<(), SSEClientTransportError>;
    async fn receive_message(&mut self) -> Result<JSONRPCMessage, SSEClientTransportError>;
}

pub(crate) struct ClientTokioMpscMessageChannel {
    out_tx: tokio::sync::mpsc::Sender<JSONRPCMessage>,
    in_rx: tokio::sync::mpsc::Receiver<JSONRPCMessage>,
}

impl ClientTokioMpscMessageChannel {
    pub(crate) fn new(
        in_rx: tokio::sync::mpsc::Receiver<JSONRPCMessage>,
        out_tx: tokio::sync::mpsc::Sender<JSONRPCMessage>,
    ) -> Self {
        Self { in_rx, out_tx }
    }
}

#[async_trait::async_trait]
impl MessageChannel for ClientTokioMpscMessageChannel {
    async fn send_message(
        &mut self,
        message: JSONRPCMessage,
    ) -> Result<(), SSEClientTransportError> {
        self.out_tx.send(message).await?;
        Ok(())
    }

    async fn receive_message(&mut self) -> Result<JSONRPCMessage, SSEClientTransportError> {
        match self.in_rx.recv().await {
            Some(message) => Ok(message),
            None => Err(SSEClientTransportError::ChannelClosed(
                "in channel closed unexpectedly".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use mcp_types::JSONRPCNotification;

    use super::*;

    #[test]
    fn test_client_tokio_mpsc_channel_new() {
        let (out_tx, in_rx) = tokio::sync::mpsc::channel(1);
        let channel = ClientTokioMpscMessageChannel::new(in_rx, out_tx);
        assert_eq!(channel.in_rx.capacity(), 1);
        assert_eq!(channel.out_tx.capacity(), 1);

        let (out_tx, in_rx) = tokio::sync::mpsc::channel(50);
        let channel = ClientTokioMpscMessageChannel::new(in_rx, out_tx);
        assert_eq!(channel.in_rx.capacity(), 50);
        assert_eq!(channel.out_tx.capacity(), 50);
    }

    #[tokio::test]
    async fn test_client_tokio_mpsc_channel_send_message() {
        let (out_tx, in_rx) = tokio::sync::mpsc::channel(1);
        let mut channel = ClientTokioMpscMessageChannel::new(in_rx, out_tx);
        let message = JSONRPCMessage::Notification(JSONRPCNotification {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: None,
        });

        channel.send_message(message.clone()).await.unwrap();
        let received_message = channel.receive_message().await.unwrap();
        assert_eq!(received_message, message);
    }
}
