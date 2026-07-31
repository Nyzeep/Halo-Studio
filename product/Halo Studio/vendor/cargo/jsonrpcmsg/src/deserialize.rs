//! JSON-RPC deserialization functions
use serde_json;
use crate::request::Request;
use crate::response::Response;
use crate::batch::Batch;

/// Error type for deserialization operations
#[derive(Debug)]
pub enum DeserializationError {
    /// JSON deserialization error
    JsonError(serde_json::Error),
}

impl std::fmt::Display for DeserializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeserializationError::JsonError(e) => write!(f, "JSON deserialization error: {}", e),
        }
    }
}

impl std::error::Error for DeserializationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DeserializationError::JsonError(e) => Some(e),
        }
    }
}

impl From<serde_json::Error> for DeserializationError {
    fn from(error: serde_json::Error) -> Self {
        DeserializationError::JsonError(error)
    }
}

/// Deserialize a JSON string to a Request
pub fn from_request_string(json: &str) -> Result<Request, DeserializationError> {
    Ok(serde_json::from_str(json)?)
}

/// Deserialize a JSON string to a Response
pub fn from_response_string(json: &str) -> Result<Response, DeserializationError> {
    Ok(serde_json::from_str(json)?)
}

/// Deserialize a JSON string to a Batch
pub fn from_batch_string(json: &str) -> Result<Batch, DeserializationError> {
    Ok(serde_json::from_str(json)?)
}

/// Deserialize a JSON Value to a Request
pub fn from_request_value(value: serde_json::Value) -> Result<Request, DeserializationError> {
    Ok(serde_json::from_value(value)?)
}

/// Deserialize a JSON Value to a Response
pub fn from_response_value(value: serde_json::Value) -> Result<Response, DeserializationError> {
    Ok(serde_json::from_value(value)?)
}

/// Deserialize a JSON Value to a Batch
pub fn from_batch_value(value: serde_json::Value) -> Result<Batch, DeserializationError> {
    Ok(serde_json::from_value(value)?)
}