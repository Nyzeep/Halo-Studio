//! JSON-RPC error types
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC error object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Error {
    /// A number that indicates the error type that occurred
    pub code: i32,
    /// A short description of the error
    pub message: String,
    /// Additional information about the error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Error {
    /// Create a new error with code and message
    pub fn new(code: i32, message: String) -> Self {
        Error {
            code,
            message,
            data: None,
        }
    }

    /// Create a new error with code, message, and data
    pub fn with_data(code: i32, message: String, data: Value) -> Self {
        Error {
            code,
            message,
            data: Some(data),
        }
    }

    /// Parse error (-32700)
    pub fn parse_error() -> Self {
        Error::new(-32700, "Parse error".to_string())
    }

    /// Invalid request error (-32600)
    pub fn invalid_request() -> Self {
        Error::new(-32600, "Invalid Request".to_string())
    }

    /// Method not found error (-32601)
    pub fn method_not_found() -> Self {
        Error::new(-32601, "Method not found".to_string())
    }

    /// Invalid params error (-32602)
    pub fn invalid_params() -> Self {
        Error::new(-32602, "Invalid params".to_string())
    }

    /// Internal error (-32603)
    pub fn internal_error() -> Self {
        Error::new(-32603, "Internal error".to_string())
    }

    /// Server error (-32000 to -32099)
    pub fn server_error(code: i32, message: String) -> Self {
        if code >= -32099 && code <= -32000 {
            Error::new(code, message)
        } else {
            Error::internal_error()
        }
    }
}