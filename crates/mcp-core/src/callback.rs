use std::future::Future;

use mcp_types::JSONRPCMessage;

#[async_trait::async_trait]
pub trait Callback: Send {
    type CallbackError: Into<Box<dyn std::error::Error + Send + Sync>> + Send + Sync + 'static;

    /// Invoked when the connection is closed for any reason.
    /// This should be invoked when close() is called as well.
    async fn on_close(&mut self) -> Result<(), Self::CallbackError>;

    /// Invoked when an error occurs.
    async fn on_error(&mut self, error: Self::CallbackError) -> Result<(), Self::CallbackError>;

    /// Invoked when a message is received.
    async fn on_message(&mut self, message: JSONRPCMessage) -> Result<(), Self::CallbackError>;

    /// Sets the callback for when the connection is closed for any reason.
    async fn set_on_close_callback<F, Fut>(&mut self, callback: F)
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), Self::CallbackError>> + Send + 'static;

    /// Sets the callback for when an error occurs.
    async fn set_on_error_callback<F, Fut>(&mut self, callback: F)
    where
        F: FnMut(Self::CallbackError) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), Self::CallbackError>> + Send + 'static;

    /// Sets the callback for when a message is received.
    async fn set_on_message_callback<F, Fut>(&mut self, callback: F)
    where
        F: FnMut(JSONRPCMessage) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), Self::CallbackError>> + Send + 'static;
}

/// Macro to implement the Callback trait
#[macro_export]
macro_rules! impl_callback {
    ($type:ty, $error:ty) => {
        #[async_trait::async_trait]
        impl $crate::callback::Callback for $type {
            type CallbackError = $error;

            async fn on_close(&mut self) -> Result<(), Self::CallbackError> {
                if let Some(callback) = self.on_close_callback.as_mut() {
                    callback().await
                } else {
                    Ok(())
                }
            }

            async fn on_error(
                &mut self,
                error: Self::CallbackError,
            ) -> Result<(), Self::CallbackError> {
                if let Some(callback) = self.on_error_callback.as_mut() {
                    callback(error).await
                } else {
                    Ok(())
                }
            }

            async fn on_message(
                &mut self,
                message: mcp_types::JSONRPCMessage,
            ) -> Result<(), Self::CallbackError> {
                if let Some(callback) = self.on_message_callback.as_mut() {
                    callback(message).await
                } else {
                    Ok(())
                }
            }

            async fn set_on_close_callback<F, Fut>(&mut self, mut callback: F)
            where
                F: FnMut() -> Fut + Send + 'static,
                Fut: Future<Output = Result<(), Self::CallbackError>> + Send + 'static,
            {
                self.on_close_callback = Some(Box::new(move || Box::pin(callback())));
            }

            async fn set_on_error_callback<F, Fut>(&mut self, mut callback: F)
            where
                F: FnMut(Self::CallbackError) -> Fut + Send + 'static,
                Fut: Future<Output = Result<(), Self::CallbackError>> + Send + 'static,
            {
                self.on_error_callback = Some(Box::new(move |error| Box::pin(callback(error))));
            }

            async fn set_on_message_callback<F, Fut>(&mut self, mut callback: F)
            where
                F: FnMut(mcp_types::JSONRPCMessage) -> Fut + Send + 'static,
                Fut: Future<Output = Result<(), Self::CallbackError>> + Send + 'static,
            {
                self.on_message_callback =
                    Some(Box::new(move |message| Box::pin(callback(message))));
            }
        }
    };
}
