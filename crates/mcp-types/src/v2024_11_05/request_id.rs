use std::hash::{Hash, Hasher};

use crate::{JsonrpcMessage, RequestId};

impl Hash for RequestId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_string().hash(state);
    }
}

impl Eq for RequestId {}

pub trait GetRequestId {
    fn get_request_id(&self) -> Option<RequestId>;
}

impl GetRequestId for JsonrpcMessage {
    fn get_request_id(&self) -> Option<RequestId> {
        match self {
            JsonrpcMessage::Request(request) => Some(request.id.clone()),
            JsonrpcMessage::Response(response) => Some(response.id.clone()),
            JsonrpcMessage::Error(error) => Some(error.id.clone()),
            _ => None,
        }
    }
}
