use std::collections::HashMap;

use mcp_types::{
    JSONRPCError, JSONRPCNotification, JSONRPCNotificationParams, JSONRPCRequest,
    JSONRPCRequestParams, JSONRPCRequestParamsMeta, JSONRPCResponse, Notification, Request,
    RequestId,
};

use crate::transport::Transport;

pub struct Protocol<E: Into<Box<dyn std::error::Error + Send + Sync>> + Send + Sync + 'static> {
    notification_handlers: HashMap<String, Box<dyn Fn(JSONRPCNotification)>>,
    request_handlers:
        HashMap<String, Box<dyn Fn(JSONRPCRequest) -> Result<JSONRPCResponse, JSONRPCError>>>,
    #[cfg(not(feature = "uuid"))]
    request_id: i64,
    transport: Box<dyn Transport<Error = E>>,
}

impl<E> Protocol<E>
where
    E: Into<Box<dyn std::error::Error + Send + Sync>> + Send + Sync + 'static,
{
    pub fn new(transport: Box<dyn Transport<Error = E>>) -> Self {
        Self {
            notification_handlers: HashMap::new(),
            request_handlers: HashMap::new(),
            #[cfg(not(feature = "uuid"))]
            request_id: 0,
            transport,
        }
    }

    pub fn register_notification_handler(
        &mut self,
        method: String,
        handler: Box<dyn Fn(JSONRPCNotification)>,
    ) {
        self.notification_handlers.insert(method, handler);
    }

    pub fn register_request_handler(
        &mut self,
        method: String,
        handler: Box<dyn Fn(JSONRPCRequest) -> Result<JSONRPCResponse, JSONRPCError>>,
    ) {
        self.request_handlers.insert(method, handler);
    }

    pub async fn send_request(&mut self, request: Request) -> Result<(), E> {
        let id = self.get_request_id();
        let request = JSONRPCRequest {
            id,
            jsonrpc: "2.0".to_string(),
            method: request.method,
            params: request.params.map(|params| JSONRPCRequestParams {
                meta: params.meta.map(|meta| JSONRPCRequestParamsMeta {
                    progress_token: meta.progress_token,
                }),
            }),
        };

        self.transport.send(request.into()).await
    }

    pub async fn send_notification(&mut self, notification: Notification) -> Result<(), E> {
        let notification = JSONRPCNotification {
            jsonrpc: "2.0".to_string(),
            method: notification.method,
            params: notification
                .params
                .map(|params| JSONRPCNotificationParams { meta: params.meta }),
        };

        self.transport.send(notification.into()).await
    }

    pub async fn connect(&mut self) -> Result<(), E> {
        self.transport.start().await
    }

    fn get_request_id(&mut self) -> RequestId {
        #[cfg(not(feature = "uuid"))]
        return self.increment_request_id();

        #[cfg(feature = "uuid")]
        return RequestId::String(uuid::Uuid::new_v4().to_string());
    }

    #[cfg(not(feature = "uuid"))]
    fn increment_request_id(&mut self) -> RequestId {
        self.request_id += 1;

        RequestId::Integer(self.request_id)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use mcp_types::{JSONRPCMessage, NotificationParams, RequestParams, RequestParamsMeta};

    use crate::transport::test_utils::MockTransport;

    #[tokio::test]
    async fn test_connect() {
        let sent_messages = Arc::new(Mutex::new(Vec::new()));
        let transport = MockTransport::new(sent_messages.clone());
        let mut protocol = Protocol::new(Box::new(transport));

        assert!(protocol.connect().await.is_ok());
    }

    #[tokio::test]
    async fn test_connect_failure() {
        let sent_messages = Arc::new(Mutex::new(Vec::new()));
        let transport = MockTransport::new(sent_messages.clone());
        transport.set_should_fail(true);
        let mut protocol = Protocol::new(Box::new(transport));

        assert!(protocol.connect().await.is_err());
    }

    #[tokio::test]
    async fn test_send_notification() {
        let sent_messages = Arc::new(Mutex::new(Vec::new()));
        let transport = MockTransport::new(sent_messages.clone());
        let mut protocol = Protocol::new(Box::new(transport));

        let notification = Notification {
            method: "test_method".to_string(),
            params: Some(NotificationParams {
                meta: serde_json::Map::new(),
            }),
        };

        assert!(protocol.send_notification(notification).await.is_ok());

        let messages = sent_messages.lock().unwrap();
        assert_eq!(messages.len(), 1);
        match &messages[0] {
            JSONRPCMessage::Notification(n) => {
                assert_eq!(n.jsonrpc, "2.0");
                assert_eq!(n.method, "test_method");
                assert!(n.params.is_some());
            }
            _ => panic!("Expected notification message"),
        }
    }

    #[tokio::test]
    async fn test_send_request() {
        let sent_messages = Arc::new(Mutex::new(Vec::new()));
        let transport = MockTransport::new(sent_messages.clone());
        let mut protocol = Protocol::new(Box::new(transport));

        let request = Request {
            method: "test_method".to_string(),
            params: Some(RequestParams {
                meta: Some(RequestParamsMeta {
                    progress_token: Some(mcp_types::ProgressToken::Integer(1)),
                }),
            }),
        };

        assert!(protocol.send_request(request).await.is_ok());

        let messages = sent_messages.lock().unwrap();
        assert_eq!(messages.len(), 1);
        match &messages[0] {
            JSONRPCMessage::Request(r) => {
                assert_eq!(r.jsonrpc, "2.0");
                assert_eq!(r.method, "test_method");
                assert!(r.params.is_some());
                #[cfg(not(feature = "uuid"))]
                assert!(match (r.id.clone(), RequestId::Integer(1)) {
                    (RequestId::Integer(a), RequestId::Integer(b)) => a == b,
                    _ => false,
                });
                #[cfg(feature = "uuid")]
                {
                    match &r.id {
                        RequestId::String(s) => assert!(uuid::Uuid::parse_str(s).is_ok()),
                        _ => panic!("Expected UUID string"),
                    }
                }
            }
            _ => panic!("Expected request message"),
        }
    }

    #[cfg(not(feature = "uuid"))]
    #[tokio::test]
    async fn test_request_id_increment() {
        let sent_messages = Arc::new(Mutex::new(Vec::new()));
        let transport = MockTransport::new(sent_messages.clone());
        let mut protocol = Protocol::new(Box::new(transport));

        assert!(match (protocol.get_request_id(), RequestId::Integer(1)) {
            (RequestId::Integer(a), RequestId::Integer(b)) => a == b,
            _ => false,
        });
        assert!(match (protocol.get_request_id(), RequestId::Integer(2)) {
            (RequestId::Integer(a), RequestId::Integer(b)) => a == b,
            _ => false,
        });
        assert!(match (protocol.get_request_id(), RequestId::Integer(3)) {
            (RequestId::Integer(a), RequestId::Integer(b)) => a == b,
            _ => false,
        });
    }
}
