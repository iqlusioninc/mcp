use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};

pub trait IntoResult: Serialize + DeserializeOwned {
    fn into_result(&self) -> mcp_types::Result;
}

impl<T: Serialize + DeserializeOwned> IntoResult for T {
    fn into_result(&self) -> mcp_types::Result {
        let value = serde_json::to_value(self).unwrap_or(Value::Null);

        // Currently this ignores non-map values. I don't currently know if
        // MCP supports non-map values in the result or not.
        let meta = match value {
            Value::Object(obj) => obj,
            _ => Map::new(),
        };

        mcp_types::Result { meta }
    }
}

pub enum ErrorCode {
    // Standard JSON-RPC error codes
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,
}

/// Represents an Error response to an MCP request. Enforced at the Handler trait level to
/// ensure that all errors are able to be converted to the generated [`JSONRPCInnerError`]
/// type.
pub trait Error {
    /// Error codes for applications should be above -32000
    fn code(&self) -> i64;

    /// A human-readable description of the error
    fn message(&self) -> String;

    /// Additional data that may help the caller understand the error
    fn data(&self) -> Option<Value>;
}
