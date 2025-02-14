pub mod convert;
pub mod error;
pub mod request_id;
pub mod types;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{
        JsonrpcError, JsonrpcMessage, JsonrpcNotification, JsonrpcRequest, JsonrpcResponse,
    };

    #[test]
    fn test_serialize_request_as_message() {
        let value = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "test",
            "params": {},
        });

        let request: JsonrpcRequest =
            serde_json::from_value(value.clone()).expect("failed to convert value to request");
        let message: JsonrpcMessage =
            serde_json::from_value(value).expect("failed to convert value to message");
        assert_eq!(message, JsonrpcMessage::Request(request));
    }

    #[test]
    fn test_serialize_response_as_message() {
        let value = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "test": "test",
            },
        });

        let response: JsonrpcResponse =
            serde_json::from_value(value.clone()).expect("failed to convert value to response");
        let message: JsonrpcMessage =
            serde_json::from_value(value).expect("failed to convert value to message");
        assert_eq!(message, JsonrpcMessage::Response(response));
    }

    #[test]
    fn test_serialize_error_as_message() {
        let value = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32000,
                "message": "test",
                "data": "test",
            },
        });

        let error: JsonrpcError =
            serde_json::from_value(value.clone()).expect("failed to convert value to error");
        let message: JsonrpcMessage =
            serde_json::from_value(value).expect("failed to convert value to message");
        assert_eq!(message, JsonrpcMessage::Error(error));
    }

    #[test]
    fn test_serialize_notification_as_message() {
        let value = json!({
            "jsonrpc": "2.0",
            "method": "test",
            "params": {},
        });

        let notification: JsonrpcNotification =
            serde_json::from_value(value.clone()).expect("failed to convert value to notification");
        let message: JsonrpcMessage =
            serde_json::from_value(value).expect("failed to convert value to message");
        assert_eq!(message, JsonrpcMessage::Notification(notification));
    }
}
