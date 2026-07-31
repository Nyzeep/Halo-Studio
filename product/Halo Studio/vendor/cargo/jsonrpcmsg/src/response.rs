//! JSON-RPC response types
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::error::Error;
use crate::request::Id;

/// JSON-RPC response object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    /// JSON-RPC version (2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jsonrpc: Option<String>,
    
    /// JSON-RPC version (1.1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    
    /// Result of the method invocation (on success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    
    /// Error object (on failure)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
    
    /// Request identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Id>,
}

impl Response {

    /// Alias for the current version: Create a new JSON-RPC 2.0 success response
    pub fn success(result: Value, id: Option<Id>) -> Self {
        Response::success_v2(result, id)
    }
    
    /// Alias for the current version: Create a new JSON-RPC 2.0 error response
    pub fn error(error: Error, id: Option<Id>) -> Self {
        Response::error_v2(error, id)
    }

    /// Create a new JSON-RPC 2.0 success response
    pub fn success_v2(result: Value, id: Option<Id>) -> Self {
        Response {
            jsonrpc: Some("2.0".to_string()),
            version: None,
            result: Some(result),
            error: None,
            id,
        }
    }
    
    /// Create a new JSON-RPC 2.0 error response
    pub fn error_v2(error: Error, id: Option<Id>) -> Self {
        Response {
            jsonrpc: Some("2.0".to_string()),
            version: None,
            result: None,
            error: Some(error),
            id,
        }
    }
    
    /// Create a new JSON-RPC 1.1 success response
    pub fn success_v1_1(result: Value, id: Option<Id>) -> Self {
        Response {
            jsonrpc: None,
            version: Some("1.1".to_string()),
            result: Some(result),
            error: None,
            id,
        }
    }
    
    /// Create a new JSON-RPC 1.1 error response
    pub fn error_v1_1(error: Error, id: Option<Id>) -> Self {
        Response {
            jsonrpc: None,
            version: Some("1.1".to_string()),
            result: None,
            error: Some(error),
            id,
        }
    }
    
    /// Create a new JSON-RPC 1.0 success response
    pub fn success_v1(result: Value, id: Option<Id>) -> Self {
        Response {
            jsonrpc: None,
            version: None,
            result: Some(result),
            error: None,
            id,
        }
    }
    
    /// Create a new JSON-RPC 1.0 error response
    pub fn error_v1(error: Error, id: Option<Id>) -> Self {
        Response {
            jsonrpc: None,
            version: None,
            result: None,
            error: Some(error),
            id,
        }
    }
    
    /// Check if this is a success response
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
    
    /// Check if this is an error response
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}