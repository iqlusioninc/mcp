use serde::de::DeserializeOwned;

pub trait MessageSchema: DeserializeOwned {
    fn method(&self) -> String;
    fn params(&self) -> Option<serde_json::Map<String, serde_json::Value>>;
}

/// Macro for implementing the `MessageSchema` trait for a given type that has
/// a `method` field and a `params` field.
macro_rules! impl_message_schema {
    ($type:ty) => {
        impl MessageSchema for $type {
            fn method(&self) -> String {
                self.method.clone()
            }

            fn params(&self) -> Option<serde_json::Map<String, serde_json::Value>> {
                serde_json::to_value(&self.params)
                    .ok()
                    .and_then(|v| v.as_object().cloned())
            }
        }
    };
}

// Implement the `MessageSchema` trait for all generated message types
use super::types::*;

impl_message_schema!(CancelledNotification);
impl_message_schema!(InitializedNotification);
impl_message_schema!(LoggingMessageNotification);
impl_message_schema!(ProgressNotification);
impl_message_schema!(PromptListChangedNotification);
impl_message_schema!(ResourceListChangedNotification);
impl_message_schema!(ResourceUpdatedNotification);
impl_message_schema!(ToolListChangedNotification);

// Implement the `MessageSchema` trait for all generated request types
impl_message_schema!(CallToolRequest);
impl_message_schema!(CompleteRequest);
impl_message_schema!(CreateMessageRequest);
impl_message_schema!(GetPromptRequest);
impl_message_schema!(InitializeRequest);
impl_message_schema!(ListPromptsRequest);
impl_message_schema!(ListResourcesRequest);
impl_message_schema!(ListResourceTemplatesRequest);
impl_message_schema!(ListRootsRequest);
impl_message_schema!(ListToolsRequest);
impl_message_schema!(PingRequest);
impl_message_schema!(ReadResourceRequest);
impl_message_schema!(SetLevelRequest);
impl_message_schema!(SubscribeRequest);
impl_message_schema!(UnsubscribeRequest);

#[cfg(test)]
mod tests {
    use crate::{InitializeRequest, InitializeRequestParams, LATEST_PROTOCOL_VERSION};

    use super::*;

    #[test]
    fn test_message_schema() {
        let request = InitializeRequest {
            method: "initialize".to_string(),
            params: InitializeRequestParams {
                capabilities: ClientCapabilities::default(),
                client_info: Implementation {
                    name: "test".to_string(),
                    version: "1.0.0".to_string(),
                },
                protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
            },
        };
        assert_eq!(request.method(), "initialize");
        assert_eq!(
            request.params().unwrap().get("protocolVersion").unwrap(),
            LATEST_PROTOCOL_VERSION
        );
    }
}
