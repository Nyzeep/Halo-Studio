//! JSON-RPC request types
use serde::{Deserialize, Serialize};
use serde_json::{Value, Map};
use crate::version::Version;

/// Parameter types for JSON-RPC requests
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Params {
    /// Positional parameters as an array
    Array(Vec<Value>),
    /// Named parameters as an object
    Object(Map<String, Value>),
}

/// Identifier types for JSON-RPC requests
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    /// String identifier
    String(String),
    /// Numeric identifier
    Number(u64),
    /// Null identifier (for notifications)
    Null,
}

/// JSON-RPC request object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// JSON-RPC version (2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jsonrpc: Option<String>,
    
    /// JSON-RPC version (1.1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    
    /// Name of the method to be invoked
    pub method: String,
    
    /// Parameters for the method invocation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Params>,
    
    /// Request identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Id>,
}

impl Request {

    /// Alias for current version: Create a new JSON-RPC 2.0 request
    pub fn new(method: String, params: Option<Params>, id: Option<Id>) -> Self {
        Request::new_v2(method, params, id)
    }

    /// Create a new JSON-RPC 2.0 request
    pub fn new_v2(method: String, params: Option<Params>, id: Option<Id>) -> Self {
        Request {
            jsonrpc: Some("2.0".to_string()),
            version: None,
            method,
            params,
            id,
        }
    }
    
    /// Create a new JSON-RPC 1.1 request
    pub fn new_v1_1(method: String, params: Option<Params>, id: Option<Id>) -> Self {
        Request {
            jsonrpc: None,
            version: Some("1.1".to_string()),
            method,
            params,
            id,
        }
    }
    
    /// Create a new JSON-RPC 1.0 request
    pub fn new_v1(method: String, params: Option<Params>, id: Option<Id>) -> Self {
        Request {
            jsonrpc: None,
            version: None,
            method,
            params,
            id,
        }
    }

    /// Alias for current version: Create a notification (request without id) 
    pub fn notification(method: String, params: Option<Params>) -> Self {
        Request::notification_v2(method, params)
    }

    /// Create a notification (request without id)
    pub fn notification_v2(method: String, params: Option<Params>) -> Self {
        Request {
            jsonrpc: Some("2.0".to_string()),
            version: None,
            method,
            params,
            id: None,
        }
    }

    /// Check if this is a notification (no id field)
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }
    
    /// Get the version of this request
    pub fn version(&self) -> Option<Version> {
        if self.jsonrpc.is_some() {
            Version::from_str(self.jsonrpc.as_ref().unwrap())
        } else if self.version.is_some() {
            Version::from_str(self.version.as_ref().unwrap())
        } else {
            Some(Version::V1_0)
        }
    }
}