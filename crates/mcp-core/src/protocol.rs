use mcp_types::{
    JSONRPCNotification, JSONRPCNotificationParams, JSONRPCRequest, JSONRPCRequestParams,
    JSONRPCRequestParamsMeta, Notification, Request, RequestId,
};

use crate::transport::Transport;

pub struct Protocol<E: Into<Box<dyn std::error::Error + Send + Sync>> + Send + Sync + 'static> {
    #[cfg(not(feature = "uuid"))]
    request_id: i64,
    transport: Box<dyn Transport<Error = E>>,
}

impl<E: Into<Box<dyn std::error::Error + Send + Sync>> + Send + Sync + 'static> Protocol<E> {
    pub fn new(transport: Box<dyn Transport<Error = E>>) -> Self {
        Self {
            #[cfg(not(feature = "uuid"))]
            request_id: 0,
            transport,
        }
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

    pub async fn send_notification(&self, notification: Notification) -> Result<(), E> {
        let notification = JSONRPCNotification {
            jsonrpc: "2.0".to_string(),
            method: notification.method,
            params: notification
                .params
                .map(|params| JSONRPCNotificationParams { meta: params.meta }),
        };

        self.transport.send(notification.into()).await
    }

    pub async fn connect(&self) -> Result<(), E> {
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
