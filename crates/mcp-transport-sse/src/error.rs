use mcp_types::JSONRPCMessage;

#[derive(Debug, thiserror::Error)]
pub enum SSEClientTransportError {
    #[error("transport already started")]
    AlreadyStarted,

    #[error("transport not started")]
    NotStarted,

    #[error("invalid ws url")]
    InvalidWsUrl,

    #[error("invalid http url")]
    InvalidHttpUrl,

    #[error("join error")]
    JoinError(#[from] tokio::task::JoinError),

    #[error("send error")]
    SendError(#[from] tokio::sync::mpsc::error::SendError<JSONRPCMessage>),

    #[error("channel closed")]
    ChannelClosed(String),
}
