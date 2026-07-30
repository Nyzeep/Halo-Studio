//! JSON-RPC serialization functions
use serde_json;
use crate::request::Request;
use crate::response::Response;
use crate::batch::Batch;

/// Error type for serialization operations
#[derive(Debug)]
pub enum SerializationError {
    /// JSON serialization error
    JsonError(serde_json::Error),
}

impl std::fmt::Display for SerializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SerializationError::JsonError(e) => write!(f, "JSON serialization error: {}", e),
        }
    }
}

impl std::error::Error for SerializationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SerializationError::JsonError(e) => Some(e),
        }
    }
}

impl From<serde_json::Error> for SerializationError {
    fn from(error: serde_json::Error) -> Self {
        SerializationError::JsonError(error)
    }
}

/// Serialize a Request to a JSON string
pub fn to_request_string(request: &Request) -> Result<String, SerializationError> {
    Ok(serde_json::to_string(request)?)
}

/// Serialize a Response to a JSON string
pub fn to_response_string(response: &Response) -> Result<String, SerializationError> {
    Ok(serde_json::to_string(response)?)
}

/// Serialize a Batch to a JSON string
pub fn to_batch_string(batch: &Batch) -> Result<String, SerializationError> {
    Ok(serde_json::to_string(batch)?)
}

/// Serialize a Request to a JSON Value
pub fn to_request_value(request: &Request) -> Result<serde_json::Value, SerializationError> {
    Ok(serde_json::to_value(request)?)
}

/// Serialize a Response to a JSON Value
pub fn to_response_value(response: &Response) -> Result<serde_json::Value, SerializationError> {
    Ok(serde_json::to_value(response)?)
}

/// Serialize a Batch to a JSON Value
pub fn to_batch_value(batch: &Batch) -> Result<serde_json::Value, SerializationError> {
    Ok(serde_json::to_value(batch)?)
}