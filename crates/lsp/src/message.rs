//! JSON-RPC 2.0 messages: requests, responses and notifications.

use serde_json::{Value, json};

/// A request's id: the client's are numbers, a server's may be strings too.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RequestId {
    Number(i64),
    Text(String),
}

impl RequestId {
    fn to_json(&self) -> Value {
        match self {
            Self::Number(number) => json!(number),
            Self::Text(text) => json!(text),
        }
    }

    fn from_json(value: &Value) -> Option<Self> {
        match value {
            Value::Number(number) => number.as_i64().map(Self::Number),
            Value::String(text) => Some(Self::Text(text.clone())),
            _ => None,
        }
    }
}

/// The error of a response.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message} ({code})")]
pub struct ResponseError {
    pub code: i64,
    pub message: String,
}

impl ResponseError {
    /// The method is not one the receiver handles.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// The request was cancelled, or the document changed under it.
    pub const REQUEST_CANCELLED: i64 = -32800;
    pub const CONTENT_MODIFIED: i64 = -32801;

    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// A message either side sends.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Request {
        id: RequestId,
        method: String,
        params: Value,
    },
    Response {
        id: RequestId,
        result: Result<Value, ResponseError>,
    },
    Notification {
        method: String,
        params: Value,
    },
}

impl Message {
    /// The message as JSON.
    pub fn to_json(&self) -> Value {
        match self {
            Self::Request { id, method, params } => json!({
                "jsonrpc": "2.0",
                "id": id.to_json(),
                "method": method,
                "params": params,
            }),
            Self::Response { id, result } => match result {
                Ok(result) => json!({ "jsonrpc": "2.0", "id": id.to_json(), "result": result }),
                Err(error) => json!({
                    "jsonrpc": "2.0",
                    "id": id.to_json(),
                    "error": { "code": error.code, "message": error.message },
                }),
            },
            Self::Notification { method, params } => json!({
                "jsonrpc": "2.0",
                "method": method,
                "params": params,
            }),
        }
    }

    /// The message's bytes, as the base protocol carries them.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&self.to_json()).unwrap_or_default()
    }

    /// Reads a message from its JSON; `None` for JSON that is no message (a batch, a response
    /// without an id, a request without a method).
    pub fn from_json(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        match (object.get("method"), object.get("id")) {
            (Some(Value::String(method)), Some(id)) => Some(Self::Request {
                id: RequestId::from_json(id)?,
                method: method.clone(),
                params,
            }),
            (Some(Value::String(method)), None) => Some(Self::Notification {
                method: method.clone(),
                params,
            }),
            (Some(_), _) => None,
            (None, Some(id)) => {
                let id = RequestId::from_json(id)?;
                let result = match (object.get("result"), object.get("error")) {
                    (_, Some(error)) if !error.is_null() => Err(ResponseError {
                        code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
                        message: error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                    }),
                    (Some(result), _) => Ok(result.clone()),
                    // `result` may be left out for a request whose result is null.
                    (None, _) => Ok(Value::Null),
                };
                Some(Self::Response { id, result })
            }
            (None, None) => None,
        }
    }

    /// Reads a message from its bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        Self::from_json(&serde_json::from_slice(bytes).ok()?)
    }
}
