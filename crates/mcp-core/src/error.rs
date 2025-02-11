use mcp_types::MCPError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("notification conversion error")]
    NotificationConversion(MCPError),

    #[error("transport error")]
    Transport(#[from] Box<dyn std::error::Error + Send + Sync>),
}
