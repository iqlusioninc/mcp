use std::{any::TypeId, collections::HashMap, future::Future, pin::Pin, sync::Arc};

use mcp_types::{
    v2024_11_05::{error::ErrorCode, request_id::GetRequestId},
    CancelledNotification, JSONRPCError, JSONRPCMessage, JSONRPCNotification,
    JSONRPCNotificationParams, JSONRPCRequest, JSONRPCRequestParams, JSONRPCRequestParamsMeta,
    JSONRPCResponse, MCPError, Notification, ProgressNotification, ProgressToken, Request,
    RequestId, Result as MCPResult,
};
use tokio::sync::Mutex;
use tracing::warn;

use crate::{error::Error, transport::Transport};

type NotificationHandlerResult =
    Pin<Box<dyn Future<Output = Result<(), Error>> + Send + Sync + 'static>>;
type NotificationHandlers =
    HashMap<TypeId, Box<dyn Fn(JSONRPCNotification) -> NotificationHandlerResult + Send + Sync>>;
type RequestHandlerResult = Pin<Box<dyn Future<Output = MCPResult> + Send + Sync + 'static>>;
type RequestHandlers =
    HashMap<TypeId, Box<dyn Fn(JSONRPCRequest) -> RequestHandlerResult + Send + Sync>>;
type ResponseHandlerResult =
    Pin<Box<dyn Future<Output = Result<(), Error>> + Send + Sync + 'static>>;
type ResponseHandlers =
    HashMap<RequestId, Box<dyn Fn(MCPResult) -> ResponseHandlerResult + Send + Sync>>;
type ProgressHandlerResult = Pin<
    Box<
        dyn FnMut(ProgressToken) -> Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>
            + Send
            + Sync
            + 'static,
    >,
>;
type ProgressHandlers =
    HashMap<RequestId, Box<dyn Fn(ProgressNotification) -> ProgressHandlerResult + Send + Sync>>;

pub trait NotificationHandler<N>: Send + Sync + 'static {
    type Future: Future<Output = Result<(), Error>> + Send + Sync + 'static;
    fn handle(&self, notification: N) -> Self::Future;
}

impl<N, F, Fut> NotificationHandler<N> for F
where
    N: 'static,
    F: Fn(N) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), Error>> + Send + Sync + 'static,
{
    type Future = Fut;
    fn handle(&self, notification: N) -> Self::Future {
        self(notification)
    }
}

pub struct Protocol<T, TErr>
where
    T: Transport<Error = TErr>,
    TErr: Into<Box<dyn std::error::Error + Send + Sync>> + Send + Sync + 'static,
{
    notification_handlers: NotificationHandlers,
    progress_handlers: ProgressHandlers,
    request_handlers: RequestHandlers,
    response_handlers: ResponseHandlers,
    #[cfg(not(feature = "uuid"))]
    request_id: i64,
    transport: Option<Box<T>>,
}

impl<T, TE> Protocol<T, TE>
where
    T: Transport<Error = TE>,
    TE: Into<Box<dyn std::error::Error + Send + Sync>> + Send + Sync + 'static,
{
    pub fn new() -> Self {
        let mut protocol = Self {
            notification_handlers: HashMap::new(),
            progress_handlers: HashMap::new(),
            request_handlers: HashMap::new(),
            response_handlers: HashMap::new(),
            #[cfg(not(feature = "uuid"))]
            request_id: 0,
            transport: None,
        };

        protocol.set_notification_handler::<CancelledNotification>(|_notification| async {
            todo!("implement abort system")
        });

        protocol.set_notification_handler::<ProgressNotification>(|_notification| async {
            todo!("implement progress system")
        });

        protocol
    }

    /// Set a handler for a specific notification type
    pub fn set_notification_handler<N>(&mut self, handler: impl NotificationHandler<N>)
    where
        N: 'static,
    {
        let key = TypeId::of::<N>();
        let handler = Box::new(
            move |notification: JSONRPCNotification| -> NotificationHandlerResult {
                let value = serde_json::to_value(&notification).unwrap();
                let notification = match mcp_types::v2024_11_05::convert::infer_notification(&value)
                {
                    Some(boxed) => match mcp_types::v2024_11_05::convert::downcast::<N>(boxed) {
                        Some(n) => n,
                        None => {
                            return Box::pin(async move {
                                Err(Error::NotificationConversion(MCPError {
                                    code: ErrorCode::InternalError as i64,
                                    message: "Failed to read notification".into(),
                                    data: None,
                                }))
                            })
                        }
                    },
                    None => {
                        return Box::pin(async move {
                            Err(Error::NotificationConversion(MCPError {
                                code: -32000,
                                message: "Invalid notification type".into(),
                                data: None,
                            }))
                        })
                    }
                };
                Box::pin(handler.handle(notification))
            },
        );
        self.notification_handlers.insert(key, handler);
    }

    pub fn set_request_handler<M: 'static>(
        &mut self,
        handler: Box<dyn Fn(JSONRPCRequest) -> RequestHandlerResult + Send + Sync>,
    ) {
        let key = TypeId::of::<M>();
        self.request_handlers.insert(key, handler);
    }

    pub async fn send_request(&mut self, request: Request) -> Result<MCPResult, Error> {
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

        if let Err(_) = self.transport.as_mut().unwrap().send(request.into()).await {
            todo!()
        }

        Ok(MCPResult::default())
    }

    pub async fn send_notification(&mut self, notification: Notification) -> Result<(), Error> {
        let notification = JSONRPCNotification {
            jsonrpc: "2.0".to_string(),
            method: notification.method,
            params: notification
                .params
                .map(|params| JSONRPCNotificationParams { meta: params.meta }),
        };
        todo!();
        //self.transport.as_mut().unwrap().send(notification.into()).await
    }

    pub async fn connect(&mut self, transport: T) -> Result<(), Error> {
        self.transport = Some(Box::new(transport));

        todo!();
        //self.transport.as_mut().unwrap().start().await?;

        Ok(())
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
    use serde::{Deserialize, Serialize};

    use super::*;

    struct TestTransport {
        messages: Vec<JSONRPCMessage>,
        called_start: bool,
        called_close: bool,
    }
    impl TestTransport {
        fn new() -> Self {
            Self {
                messages: Vec::new(),
                called_start: false,
                called_close: false,
            }
        }
    }

    #[async_trait::async_trait]
    impl Transport for TestTransport {
        type Error = String;

        async fn send(&mut self, message: JSONRPCMessage) -> Result<(), Self::Error> {
            self.messages.push(message);
            Ok(())
        }

        async fn start(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn close(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
    struct TestNotification {
        method: String,
        params: Option<TestNotificationParams>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
    struct TestNotificationParams {
        test: String,
    }

    struct ProtocolFixture {
        protocol: Protocol<TestTransport, String>,
        called_notification_callback: bool,
    }

    impl ProtocolFixture {
        fn new(protocol: Protocol<TestTransport, String>) -> Self {
            Self {
                protocol,
                called_notification_callback: false,
            }
        }

        async fn set_notification_handler<N: 'static>(
            &mut self,
            handler: impl NotificationHandler<N>,
        ) {
            self.protocol.set_notification_handler(handler);
        }

        async fn connect(&mut self, transport: TestTransport) {
            self.protocol.connect(transport).await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_protocol() {
        let mut protocol = Protocol::new();
        let mut fixture = ProtocolFixture::new(protocol);
        let notification = TestNotification {
            method: "test".to_string(),
            params: Some(TestNotificationParams {
                test: "test".to_string(),
            }),
        };
        fixture.set_notification_handler::<TestNotification>(|notification| async move {
            println!("Received notification: {:?}", notification);
            Ok(())
        });

        let transport = TestTransport::new();
        fixture.connect(transport).await;
    }
}
